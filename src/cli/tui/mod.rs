//! Interactive terminal UI, built on ratatui (which re-exports its crossterm backend).
//!
//! Compiled only with `--features tui`; the default build and the `loope` library stay
//! dependency-free. Two entry points: [`run_browser`] explores `.loope/runs/` and
//! [`run_live`] watches a loop execute.

mod action;
mod app;
mod model;
mod observer;
mod style;
mod view;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event};

use loope::adapter::Invoker;
use loope::engine::workspace::RunWorkspace;
use loope::engine::{LoopConfig, execute_loop};

use action::action_from_key;
use app::App;
use observer::{LiveMsg, TuiObserver};

/// How long the live loop waits for a key before redrawing (drives the spinner).
const TICK: Duration = Duration::from_millis(80);

/// Browse the runs under `runs_dir` interactively. Returns when the user quits.
///
/// [`ratatui::init`] switches to the alternate screen + raw mode and installs a panic
/// hook that restores the terminal; [`ratatui::restore`] undoes it on the way out, so the
/// terminal is left clean even if the event loop errors.
pub fn run_browser(runs_dir: &Path) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = browse_loop(&mut terminal, &mut App::new(runs_dir));
    ratatui::restore();
    result
}

/// Run the loop in a full-screen live dashboard. The executor runs on a worker thread and
/// streams updates over a channel; when it finishes the view settles into the browser.
pub fn run_live(
    config: LoopConfig,
    workspace: RunWorkspace,
    invoker: Box<dyn Invoker + Send + Sync>,
) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let run_id = workspace.run_id.clone();
    let runs_dir = workspace
        .root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace.root.clone());
    let mut app = App::new_live(run_id, &runs_dir);

    let worker = std::thread::spawn(move || {
        let observer = TuiObserver::new(tx);
        let _ = execute_loop(&config, &workspace, invoker.as_ref(), Some(&observer));
        // `tx` drops here → the channel disconnects → the UI sees completion.
    });

    let mut terminal = ratatui::init();
    let result = live_loop(&mut terminal, &mut app, &rx);
    ratatui::restore();

    // If the run finished, the worker has already exited; reap it. If the user quit early,
    // don't block — returning ends the process.
    if app.live_done {
        let _ = worker.join();
    }
    result
}

/// Browse mode blocks on input — there is no ticking work to do.
fn browse_loop(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| view::draw(frame, app))?;
        if let Event::Key(key) = event::read()?
            && let Some(action) = action_from_key(key)
        {
            app.update(action);
        }
    }
    Ok(())
}

/// Live mode polls for input on a short tick so the spinner animates and queued updates
/// are drained even while the user is idle. After the run finishes it keeps running as a
/// browser over the (now reloaded) run.
fn live_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &Receiver<LiveMsg>,
) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| view::draw(frame, app))?;

        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
            && let Some(action) = action_from_key(key)
        {
            app.update(action);
        }

        drain(app, rx);
        if app.live {
            app.tick();
        }
    }
    Ok(())
}

/// Apply every queued live update; on disconnect, settle into browse mode.
fn drain(app: &mut App, rx: &Receiver<LiveMsg>) {
    loop {
        match rx.try_recv() {
            Ok(msg) => app.apply_live(msg),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                if app.live {
                    app.finish_live();
                }
                break;
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
