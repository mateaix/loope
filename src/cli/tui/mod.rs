//! Interactive terminal UI, built on ratatui (which re-exports its crossterm backend).
//!
//! Compiled only with `--features tui`; the default build and the `loope` library stay
//! dependency-free. The front door is [`run_home`] — a prompt you type a requirement into,
//! watch run live, then browse. [`run_browser`] and [`run_live`] are the `loope tui` and
//! `loope run --tui` entry points; all three share one [`app_loop`].

mod action;
mod app;
mod command;
mod config;
mod model;
mod observer;
mod style;
mod view;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use loope::adapter::Invoker;
use loope::engine::workspace::RunWorkspace;
use loope::engine::{LoopConfig, execute_loop};

use action::action_from_key;
use app::{App, Screen};
use config::RunOptions;
use observer::{LiveMsg, TuiObserver};

/// How long the loop waits for a key before redrawing (drives the spinner).
const TICK: Duration = Duration::from_millis(80);

/// The interactive home screen (`loope` with no arguments): type a requirement, watch it
/// run, browse the result, repeat. `dry_run` uses the stub agents.
pub fn run_home(cwd: &Path, dry_run: bool) -> io::Result<()> {
    let session = Session::new(cwd.to_path_buf());
    let mut app = App::home(&session.base, dry_run);
    let mut terminal = ratatui::init();
    let result = app_loop(&mut terminal, &mut app, Some(&session), None, None);
    ratatui::restore();
    result
}

/// Browse the runs under `runs_dir` (`loope tui`); no new runs can be launched.
pub fn run_browser(runs_dir: &Path) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = app_loop(&mut terminal, &mut App::new(runs_dir), None, None, None);
    ratatui::restore();
    result
}

/// Watch a pre-configured run live (`loope run --tui`), then browse it.
pub fn run_live(
    config: LoopConfig,
    workspace: RunWorkspace,
    invoker: Box<dyn Invoker + Send + Sync>,
) -> io::Result<()> {
    let run_id = workspace.run_id.clone();
    let runs_dir = workspace
        .root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace.root.clone());
    let mut app = App::new_live(run_id, &runs_dir);

    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let observer = TuiObserver::new(tx);
        let _ = execute_loop(&config, &workspace, invoker.as_ref(), Some(&observer));
    });

    let mut terminal = ratatui::init();
    let result = app_loop(&mut terminal, &mut app, None, Some(rx), Some(worker));
    ratatui::restore();
    result
}

/// Launches runs from the home prompt using the app's current [`RunOptions`].
struct Session {
    cwd: PathBuf,
    base: PathBuf,
}

/// A spawned run the UI is watching.
struct RunHandle {
    run_id: String,
    run_dir: PathBuf,
    rx: Receiver<LiveMsg>,
    worker: JoinHandle<()>,
}

impl Session {
    fn new(cwd: PathBuf) -> Self {
        let base = cwd.join(".loope").join("runs");
        Self { cwd, base }
    }

    /// Start a run for `requirement` on a worker thread per `options`, returning its live
    /// channel.
    fn start(&self, requirement: String, options: &RunOptions) -> io::Result<RunHandle> {
        let workspace = RunWorkspace::create(&self.base, &self.cwd, false)?;
        let run_id = workspace.run_id.clone();
        let run_dir = workspace.root.clone();
        let config = options.config(requirement);
        let invoker = options.make_invoker();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let observer = TuiObserver::new(tx);
            let _ = execute_loop(&config, &workspace, invoker.as_ref(), Some(&observer));
        });
        Ok(RunHandle {
            run_id,
            run_dir,
            rx,
            worker,
        })
    }
}

/// The one event loop shared by every entry point: draw, read a key (text input on the
/// home screen, actions elsewhere), launch a queued requirement, and drain live updates.
fn app_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    session: Option<&Session>,
    mut rx: Option<Receiver<LiveMsg>>,
    mut worker: Option<JoinHandle<()>>,
) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| view::draw(frame, app))?;

        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
        {
            handle_key(app, key, session.is_some());
        }

        if let Some(requirement) = app.take_submit()
            && let Some(session) = session
        {
            match session.start(requirement, &app.options) {
                Ok(handle) => {
                    app.begin_live(handle.run_id, handle.run_dir);
                    rx = Some(handle.rx);
                    worker = Some(handle.worker);
                }
                Err(err) => app.set_error(format!("could not start run: {err}")),
            }
        }

        if let Some(channel) = rx.as_ref()
            && drain(app, channel)
        {
            rx = None;
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }

        if app.live {
            app.tick();
        }
    }
    Ok(())
}

/// Interpret a key for the current screen. Returns nothing; mutates the app.
fn handle_key(app: &mut App, key: KeyEvent, can_launch: bool) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    if app.screen == Screen::Home {
        if app.command_mode() {
            // Slash-command palette: navigate, complete, run, or leave command mode.
            match key.code {
                KeyCode::Char('c') if ctrl => app.should_quit = true,
                KeyCode::Up => app.palette_move(false),
                KeyCode::Down => app.palette_move(true),
                KeyCode::Tab => app.complete_palette(),
                KeyCode::Enter => app.run_command(),
                KeyCode::Esc => app.clear_input(),
                KeyCode::Backspace => app.input_backspace(),
                KeyCode::Char(c) if !ctrl => app.input_char(c),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Enter => app.input_submit(),
            KeyCode::Backspace => app.input_backspace(),
            KeyCode::Esc => app.should_quit = true,
            KeyCode::Char('c') if ctrl => app.should_quit = true,
            KeyCode::Tab if !app.runs.is_empty() => app.toggle_home_browse(),
            KeyCode::Char(c) if !ctrl => app.input_char(c),
            _ => {}
        }
        return;
    }

    // Browse/Live: Esc returns to the prompt when this session can launch runs.
    if can_launch && app.screen == Screen::Browse && key.code == KeyCode::Esc {
        app.toggle_home_browse();
        return;
    }
    if let Some(action) = action_from_key(key) {
        app.update(action);
    }
}

/// Apply every queued live update; returns `true` once the run's channel disconnects.
fn drain(app: &mut App, rx: &Receiver<LiveMsg>) -> bool {
    loop {
        match rx.try_recv() {
            Ok(msg) => app.apply_live(msg),
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                if app.live {
                    app.finish_live();
                }
                return true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_runs() -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "loope-tui-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let run = dir.join("run-0001");
        fs::create_dir_all(&run).unwrap();
        fs::write(
            run.join("run.json"),
            "{\"converged\":true,\"iterations\":1,\"stop_reason\":\"converged\",\"steps\":[{\"role\":\"implementer\"}]}",
        )
        .unwrap();
        fs::write(
            run.join("report.md"),
            "- Run: run-0001\n- Outcome: converged\n- Iterations: 1\n\n## Steps\n\n### Iteration 1\n\n1. **implementer via Claude** — PASS\n   - Message: hi\n",
        )
        .unwrap();
        dir
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
        }
        out
    }

    #[test]
    fn browse_frame_renders() {
        let dir = temp_runs();
        let app = App::new(&dir);
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("loope"));
        assert!(text.contains("run-0001"));
        assert!(text.contains("implementer"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn home_frame_shows_prompt_and_typed_input() {
        let dir = temp_runs();
        let mut app = App::home(&dir, true);
        app.input_char('a');
        app.input_char('d');
        app.input_char('d');
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("loope"));
        assert!(text.contains("add"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn submit_queues_then_clears_input() {
        let dir = temp_runs();
        let mut app = App::home(&dir, true);
        for c in "do it".chars() {
            app.input_char(c);
        }
        app.input_submit();
        assert_eq!(app.take_submit().as_deref(), Some("do it"));
        assert!(app.input.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_command_sets_run_options() {
        let dir = temp_runs();
        let mut app = App::home(&dir, true);
        for c in "/iters 7".chars() {
            app.input_char(c);
        }
        assert!(app.command_mode());
        app.run_command();
        assert_eq!(app.options.max_iters, 7);
        assert_eq!(app.options.config("x".to_string()).max_iters, 7);
        assert!(app.input.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_mode_frame_shows_palette() {
        let dir = temp_runs();
        let mut app = App::home(&dir, true);
        app.input_char('/');
        app.input_char('i');
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("commands"));
        assert!(text.contains("/iters"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_and_help_frames_render() {
        let empty = std::env::temp_dir().join("loope-tui-empty-does-not-exist");
        let mut app = App::new(&empty);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        app.update(action::Action::Help);
        terminal.draw(|frame| view::draw(frame, &app)).unwrap();
        assert!(buffer_text(&terminal).contains("keys"));
    }
}
