//! The TUI application state. Rendering is a pure function of `App`; input is folded in
//! through [`App::update`] one [`Action`] at a time (an immediate-mode design).

use std::path::{Path, PathBuf};

use loope::Adapter;
use loope::adapter::event::LoopEvent;
use loope::adapter::{AdapterStatus, check_adapters};

use super::action::Action;
use super::command::{self, Command};
use super::config::RunOptions;
use super::model::{RunDetail, RunEntry, load_run, load_runs};
use super::observer::LiveMsg;

/// Spinner frames for the live header / active step.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Which pane currently has keyboard focus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
    Runs,
    Detail,
}

/// What the preview region under the step list shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Preview {
    Result,
    Diff,
    Transcript,
    Activity,
}

/// Which screen the TUI is showing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    /// The prompt: type a requirement and launch a run.
    Home,
    /// A run is executing.
    Live,
    /// Browsing finished runs.
    Browse,
}

/// All TUI state.
pub struct App {
    base: PathBuf,
    pub screen: Screen,
    /// The prompt text being typed on the home screen.
    pub input: String,
    /// Set when the user submits a requirement; the event loop launches it.
    submit: Option<String>,
    /// A transient error to surface (e.g. a run failed to start).
    pub error: Option<String>,
    /// The run configuration slash commands mutate.
    pub options: RunOptions,
    /// A transient status message from the last command.
    pub message: Option<String>,
    /// Local availability of the agent CLIs, self-checked on entering the home screen.
    pub agents: Vec<AdapterStatus>,
    /// Selected entry in the slash-command palette.
    palette_index: usize,
    pub runs: Vec<RunEntry>,
    pub runs_selected: usize,
    pub detail: Option<RunDetail>,
    pub detail_selected: usize,
    pub focus: Focus,
    pub preview: Preview,
    pub preview_scroll: u16,
    pub show_help: bool,
    pub should_quit: bool,
    // Live mode: populated from observer messages.
    pub live: bool,
    pub live_done: bool,
    pub live_iter: Option<(usize, usize)>,
    pub active: Option<String>,
    /// The active step's normalized event stream (actions + messages).
    pub activity: Vec<LoopEvent>,
    /// The active step's model, if the CLI reported one.
    pub model: Option<String>,
    /// The active step's latest (input, output) token counts, if reported.
    pub tokens: Option<(u64, u64)>,
    spinner: usize,
}

impl App {
    /// The home screen: a prompt to type a requirement (or a `/` command), with past runs
    /// available to browse. This is the front door (`loope` with no arguments).
    pub fn home(runs_dir: &Path, dry_run: bool) -> Self {
        let mut app = Self::new(runs_dir);
        app.screen = Screen::Home;
        app.options = RunOptions::new(dry_run);
        app.agents = check_adapters(); // self-check the local agent CLIs on entry
        app
    }

    /// A browse-only app over a runs directory (`loope tui`), newest run selected.
    pub fn new(runs_dir: &Path) -> Self {
        let runs = load_runs(runs_dir);
        let mut app = Self::empty(runs_dir.to_path_buf());
        app.screen = Screen::Browse;
        app.runs = runs;
        app.reload_detail();
        app
    }

    /// A live app for `loope run --tui`: an in-progress run accumulating from observer
    /// messages, with the detail pane focused.
    pub fn new_live(run_id: String, runs_dir: &Path) -> Self {
        let mut app = Self::empty(runs_dir.to_path_buf());
        app.begin_live(run_id, runs_dir.to_path_buf());
        app
    }

    fn empty(base: PathBuf) -> Self {
        Self {
            base,
            screen: Screen::Browse,
            input: String::new(),
            submit: None,
            error: None,
            options: RunOptions::new(false),
            message: None,
            agents: Vec::new(),
            palette_index: 0,
            runs: Vec::new(),
            runs_selected: 0,
            detail: None,
            detail_selected: 0,
            focus: Focus::Runs,
            preview: Preview::Result,
            preview_scroll: 0,
            show_help: false,
            should_quit: false,
            live: false,
            live_done: false,
            live_iter: None,
            active: None,
            activity: Vec::new(),
            model: None,
            tokens: None,
            spinner: 0,
        }
    }

    // --- Home-screen text input -------------------------------------------------

    pub fn input_char(&mut self, c: char) {
        self.error = None;
        self.message = None;
        self.input.push(c);
        self.palette_index = 0;
    }

    pub fn input_backspace(&mut self) {
        self.input.pop();
        self.palette_index = 0;
    }

    /// Queue the typed requirement for launch (no-op if blank).
    pub fn input_submit(&mut self) {
        let requirement = self.input.trim().to_string();
        if !requirement.is_empty() {
            self.submit = Some(requirement);
            self.input.clear();
        }
    }

    // --- Slash commands ---------------------------------------------------------

    /// True when the prompt holds a `/` command rather than a requirement.
    pub fn command_mode(&self) -> bool {
        self.input.starts_with('/')
    }

    /// The palette entries matching the current input.
    pub fn palette(&self) -> Vec<&'static command::Spec> {
        command::matches(&self.input)
    }

    /// The clamped palette selection index.
    pub fn palette_selected(&self) -> usize {
        let len = self.palette().len();
        if len == 0 { 0 } else { self.palette_index.min(len - 1) }
    }

    pub fn palette_move(&mut self, down: bool) {
        let len = self.palette().len();
        if len == 0 {
            return;
        }
        let current = self.palette_index.min(len - 1);
        self.palette_index = if down {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
    }

    /// Replace the input with the selected command name (ready for arguments).
    pub fn complete_palette(&mut self) {
        let matches = self.palette();
        if let Some(spec) = matches.get(self.palette_selected()) {
            self.input = format!("/{} ", spec.name);
        }
    }

    /// Leave command mode, clearing the input (the caller quits if already empty).
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.palette_index = 0;
    }

    /// Parse and run the typed command, setting a status message.
    pub fn run_command(&mut self) {
        match command::parse(&self.input) {
            Ok(cmd) => {
                self.apply_command(cmd);
                self.clear_input();
            }
            Err(err) => self.message = Some(err),
        }
    }

    fn apply_command(&mut self, cmd: Command) {
        match cmd {
            Command::Iters(n) => {
                self.options.max_iters = n;
                self.note(format!("max-iters = {n}"));
            }
            Command::Preset(name) => match preset(&name) {
                Some((implementer, reviewers)) => {
                    self.options.implementer = implementer;
                    self.options.reviewers = reviewers;
                    self.note(format!("preset {name}"));
                }
                None => self.note(format!("unknown preset: {name}")),
            },
            Command::Implementer(adapter) => {
                self.options.implementer = adapter;
                self.note(format!("implementer = {}", adapter.as_str()));
            }
            Command::Reviewers(adapters) => {
                let summary = adapters.iter().map(|a| a.as_str()).collect::<Vec<_>>().join("+");
                self.options.reviewers = adapters;
                self.note(format!("reviewers = {summary}"));
            }
            Command::Verify(cmd) => {
                self.note(match &cmd {
                    Some(c) => format!("verify = {c}"),
                    None => "verify cleared".to_string(),
                });
                self.options.verify_command = cmd;
            }
            Command::ToggleDesign => {
                self.options.include_design = !self.options.include_design;
                self.note(format!("design {}", on_off(self.options.include_design)));
            }
            Command::ToggleDry => {
                self.options.dry_run = !self.options.dry_run;
                self.note(format!("dry-run {}", on_off(self.options.dry_run)));
            }
            Command::Apply => self.apply_selected_run(),
            Command::Doctor => {
                self.agents = check_adapters();
                let found = self.agents.iter().filter(|a| a.available).count();
                self.note(format!("re-checked agents: {found}/{} available", self.agents.len()));
            }
            Command::Browse => {
                if self.runs.is_empty() {
                    self.note("no runs to browse".to_string());
                } else {
                    self.screen = Screen::Browse;
                }
            }
            Command::Help => self.show_help = true,
            Command::Quit => self.should_quit = true,
        }
    }

    fn note(&mut self, message: String) {
        self.message = Some(message);
    }

    /// Copy the selected run's changed files into the working directory.
    fn apply_selected_run(&mut self) {
        let Some(id) = self.selected_run().map(|r| r.id.clone()) else {
            self.note("no run to apply".to_string());
            return;
        };
        let run_dir = self.base.join(&id);
        let workspace = run_dir.join("workspace");
        let target = self
            .base
            .parent()
            .and_then(Path::parent)
            .unwrap_or(&self.base)
            .to_path_buf();
        let Ok(listing) = std::fs::read_to_string(run_dir.join("changed-files.txt")) else {
            self.note(format!("{id}: nothing to apply"));
            return;
        };
        let mut applied = 0usize;
        for rel in listing.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let from = workspace.join(rel);
            let to = target.join(rel);
            if !from.is_file() {
                continue;
            }
            if let Some(parent) = to.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(&from, &to).is_ok() {
                applied += 1;
            }
        }
        self.note(format!("applied {applied} file(s) from {id}"));
    }

    /// Take a queued requirement, if any (called by the event loop to start a run).
    pub fn take_submit(&mut self) -> Option<String> {
        self.submit.take()
    }

    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
    }

    /// Switch between the home prompt and browsing history.
    pub fn toggle_home_browse(&mut self) {
        self.screen = match self.screen {
            Screen::Home => Screen::Browse,
            _ => Screen::Home,
        };
    }

    /// Begin a fresh live run, resetting the accumulating detail.
    pub fn begin_live(&mut self, run_id: String, run_dir: PathBuf) {
        self.detail = Some(RunDetail {
            dir: run_dir,
            id: run_id,
            ..Default::default()
        });
        self.detail_selected = 0;
        self.focus = Focus::Detail;
        self.preview = Preview::Result;
        self.preview_scroll = 0;
        self.screen = Screen::Live;
        self.live = true;
        self.live_done = false;
        self.live_iter = None;
        self.active = None;
        self.activity.clear();
        self.model = None;
        self.tokens = None;
        self.error = None;
    }

    /// Advance the spinner (called each UI tick in live mode).
    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
    }

    pub fn spinner_char(&self) -> &'static str {
        SPINNER[self.spinner % SPINNER.len()]
    }

    /// Fold one live update into the state.
    pub fn apply_live(&mut self, msg: LiveMsg) {
        match msg {
            LiveMsg::Iteration { n, total } => self.live_iter = Some((n, total)),
            LiveMsg::StepStart { role, adapter } => {
                self.active = Some(format!("{role} · {adapter}"));
                self.activity.clear();
                self.model = None;
                self.tokens = None;
            }
            LiveMsg::Event(event) => match event {
                LoopEvent::Model { name } => self.model = Some(name),
                LoopEvent::Usage { input_tokens, output_tokens } => {
                    self.tokens = Some((input_tokens, output_tokens));
                }
                action_or_message => {
                    self.activity.push(action_or_message);
                    if self.activity.len() > 500 {
                        self.activity.remove(0);
                    }
                }
            },
            LiveMsg::StepFinish(step) => {
                if let Some(detail) = self.detail.as_mut() {
                    detail.steps.push(*step);
                    self.detail_selected = detail.steps.len().saturating_sub(1);
                }
                self.active = None;
            }
        }
    }

    /// The run finished: drop live chrome and reload it from disk so diffs/transcripts
    /// become browsable.
    pub fn finish_live(&mut self) {
        self.live = false;
        self.live_done = true;
        self.active = None;
        self.screen = Screen::Browse;
        let id = self.detail.as_ref().map(|d| d.id.clone());
        self.runs = load_runs(&self.base);
        self.runs_selected = id
            .and_then(|id| self.runs.iter().position(|r| r.id == id))
            .unwrap_or(0);
        self.reload_detail();
    }

    pub fn selected_run(&self) -> Option<&RunEntry> {
        self.runs.get(self.runs_selected)
    }

    /// Fold one user intent into the state.
    pub fn update(&mut self, action: Action) {
        if self.show_help {
            match action {
                Action::Quit => self.should_quit = true,
                _ => self.show_help = false,
            }
            return;
        }

        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.show_help = true,
            Action::Tab => self.toggle_focus(),
            Action::Right | Action::Enter => self.enter_detail(),
            Action::Left | Action::Back => self.focus = Focus::Runs,
            Action::Up => self.move_up(),
            Action::Down => self.move_down(),
            Action::Top => self.move_to_edge(true),
            Action::Bottom => self.move_to_edge(false),
            Action::ToggleDiff => self.set_preview(Preview::Diff),
            Action::ToggleTranscript => self.set_preview(Preview::Transcript),
            Action::ToggleActivity => self.set_preview(Preview::Activity),
            Action::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(10),
            Action::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(10),
            Action::Refresh => self.refresh(),
        }
    }

    fn enter_detail(&mut self) {
        if self.detail.as_ref().is_some_and(|d| !d.steps.is_empty()) {
            self.focus = Focus::Detail;
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Runs => Focus::Detail,
            Focus::Detail => Focus::Runs,
        };
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Runs if self.runs_selected > 0 => {
                self.runs_selected -= 1;
                self.reload_detail();
            }
            Focus::Detail if self.detail_selected > 0 => {
                self.detail_selected -= 1;
                self.preview_scroll = 0;
            }
            _ => {}
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Runs if self.runs_selected + 1 < self.runs.len() => {
                self.runs_selected += 1;
                self.reload_detail();
            }
            Focus::Detail if self.detail_selected + 1 < self.step_count() => {
                self.detail_selected += 1;
                self.preview_scroll = 0;
            }
            _ => {}
        }
    }

    fn move_to_edge(&mut self, top: bool) {
        match self.focus {
            Focus::Runs => {
                self.runs_selected = if top { 0 } else { self.runs.len().saturating_sub(1) };
                self.reload_detail();
            }
            Focus::Detail => {
                self.detail_selected = if top { 0 } else { self.step_count().saturating_sub(1) };
                self.preview_scroll = 0;
            }
        }
    }

    /// Toggle a preview kind on, or back to the result when pressed again.
    fn set_preview(&mut self, kind: Preview) {
        self.preview = if self.preview == kind { Preview::Result } else { kind };
        self.preview_scroll = 0;
    }

    fn refresh(&mut self) {
        let id = self.selected_run().map(|r| r.id.clone());
        self.runs = load_runs(&self.base);
        self.runs_selected = id
            .and_then(|id| self.runs.iter().position(|r| r.id == id))
            .unwrap_or(0);
        self.reload_detail();
    }

    fn reload_detail(&mut self) {
        self.detail = self
            .selected_run()
            .map(|r| self.base.join(&r.id))
            .and_then(|dir| load_run(&dir));
        self.detail_selected = 0;
        self.preview = Preview::Result;
        self.preview_scroll = 0;
    }

    fn step_count(&self) -> usize {
        self.detail.as_ref().map_or(0, |d| d.steps.len())
    }
}

/// Expand a preset name into (implementer, reviewers), mirroring the CLI presets.
fn preset(name: &str) -> Option<(Adapter, Vec<Adapter>)> {
    Some(match name {
        "claude-codex" => (Adapter::Claude, vec![Adapter::Codex]),
        "codex-claude" => (Adapter::Codex, vec![Adapter::Claude]),
        "claude-solo" => (Adapter::Claude, vec![Adapter::Claude]),
        "dual-review" => (Adapter::Claude, vec![Adapter::Codex, Adapter::Claude]),
        "opencode-codex" => (Adapter::OpenCode, vec![Adapter::Codex]),
        _ => return None,
    })
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}
