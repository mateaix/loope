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

/// All TUI state.
pub struct App {
    base: PathBuf,
    pub runs: Vec<RunEntry>,
    pub runs_selected: usize,
    pub detail: Option<RunDetail>,
    pub detail_selected: usize,
    pub focus: Focus,
    pub preview: Preview,
    pub preview_scroll: u16,
    pub show_help: bool,
    pub should_quit: bool,
    // Live mode (`loope run --tui`): populated from observer messages.
    pub live: bool,
    pub live_done: bool,
    pub live_iter: Option<(usize, usize)>,
    pub active: Option<String>,
    pub activity: Vec<String>,
    spinner: usize,
}

impl App {
    /// Build the app from a runs directory, selecting and loading the newest run.
    pub fn new(runs_dir: &Path) -> Self {
        let runs = load_runs(runs_dir);
        let mut app = Self::empty(runs_dir.to_path_buf());
        app.runs = runs;
        app.reload_detail();
        app
    }

    /// A live app for `loope run --tui`: an in-progress run accumulating from observer
    /// messages, with the detail pane focused.
    pub fn new_live(run_id: String, runs_dir: &Path) -> Self {
        let mut app = Self::empty(runs_dir.to_path_buf());
        app.detail = Some(RunDetail {
            dir: runs_dir.join(&run_id),
            id: run_id,
            ..Default::default()
        });
        app.focus = Focus::Detail;
        app.live = true;
        app
    }

    fn empty(base: PathBuf) -> Self {
        Self {
            base,
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
