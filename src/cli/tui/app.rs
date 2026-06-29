//! The TUI application state. Rendering is a pure function of `App`; input is folded in
//! through [`App::update`] one [`Action`] at a time (an immediate-mode design).

use std::path::{Path, PathBuf};

use super::action::Action;
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
    pub activity: Vec<String>,
    spinner: usize,
}

impl App {
    /// The home screen: a prompt to type a requirement, with past runs available to
    /// browse. This is the front door (`loope` with no arguments).
    pub fn home(runs_dir: &Path) -> Self {
        let mut app = Self::new(runs_dir);
        app.screen = Screen::Home;
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
            spinner: 0,
        }
    }

    // --- Home-screen text input -------------------------------------------------

    pub fn input_char(&mut self, c: char) {
        self.error = None;
        self.input.push(c);
    }

    pub fn input_backspace(&mut self) {
        self.input.pop();
    }

    /// Queue the typed requirement for launch (no-op if blank).
    pub fn input_submit(&mut self) {
        let requirement = self.input.trim().to_string();
        if !requirement.is_empty() {
            self.submit = Some(requirement);
            self.input.clear();
        }
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
            }
            LiveMsg::Activity(line) => {
                self.activity.push(line);
                if self.activity.len() > 200 {
                    self.activity.remove(0);
                }
            }
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
