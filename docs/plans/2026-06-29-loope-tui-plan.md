# Loope TUI Implementation Plan

> Build an interactive ratatui/crossterm TUI behind an optional `tui` feature. The default
> build stays std-only and the library never sees ratatui. Implement task-by-task, keep
> both feature configurations green, commit once at the end.

**Spec:** [TUI Spec](../specs/2026-06-29-loope-tui-spec.md)

**Stack:** Rust 2024. `ratatui` + `crossterm` behind `--features tui`; the default build and
the `loope` library remain dependency-free. Model UX on ratatui's high-star apps
(gitui multi-pane + footer, yazi/atuin list+preview).

**Architecture:** `App` (immediate-mode state) + `draw(frame, &app)`; a small `Action`
event vocabulary; a disk/live-agnostic view model; a `StepObserver` channel bridge for
live mode so the engine stays UI-agnostic.

---

## Tasks

### Task 1: Feature scaffolding + CLI wiring

- [ ] `Cargo.toml`: add `[features] tui = ["dep:ratatui", "dep:crossterm"]` and the two
      optional deps (pinned). Default features empty.
- [ ] Create `src/cli/tui/mod.rs` gated `#[cfg(feature = "tui")]`; declare `pub mod tui;`
      in `cli.rs` (also gated).
- [ ] `main.rs`: recognize `loope tui` and `loope run --tui` always. With the feature,
      dispatch into the TUI; without it, print *"the TUI requires `--features tui`"* and
      exit `2`. `--tui` errors cleanly if stdout is not a TTY.
- [ ] Verify: `cargo build`, `cargo build --features tui`, `cargo test` all green; library
      crate has no ratatui/crossterm references.

### Task 2: Terminal lifecycle + event loop + actions

- [ ] `tui/mod.rs`: a RAII `Terminal` guard — enter alternate screen + raw mode on init,
      restore on drop; install a panic hook that restores first.
- [ ] `tui/app.rs`: `App` skeleton (mode, focus, selection/scroll, `should_quit`).
- [ ] `tui/action.rs`: `Action` enum + `KeyEvent → Action` mapping (arrows + vim keys,
      Tab, Enter/Esc, d/t/g/G/PgUp/PgDn/r/?, q/Ctrl-C).
- [ ] Event loop: `draw → read event → App::update(action)`; `q` quits and restores the
      terminal cleanly. Renders a header + empty body + footer.

### Task 3: View model + browse navigation

- [ ] `tui/model.rs`: `RunEntry` (id, outcome, iterations, changed stats), `RunDetail`
      (steps grouped by iteration), `StepView` (role, adapter, gate, verdict, changes,
      message). `load_runs(base)` and `load_run(dir)` read `run.json` / `report.md` /
      `agents/*/result.md`.
- [ ] `tui/view/`: `draw` = header + body(list | detail) + footer; `runs.rs` renders the
      run list with a `ListState`; `detail.rs` renders steps grouped by iteration.
- [ ] Wire navigation: move selection in the focused pane, Enter focuses detail, Esc
      returns; selecting a run loads its `RunDetail`.

### Task 4: Preview pane (result / diff / transcript)

- [ ] `tui/view/diff.rs`: a scrollable, colored diff widget (reuse hunk/gutter/`+`/`−`
      coloring semantics from the print renderer, expressed as ratatui `Line`/`Span`).
- [ ] Preview shows the focused step's `result.md` message; `d` toggles the step's
      `changes.diff` (or the run-level cumulative diff); `t` toggles `transcript.jsonl`.
- [ ] PgUp/PgDn/g/G scroll the preview; long content clamps without panicking.

### Task 5: Live mode (`loope run --tui`)

- [ ] `tui/observer.rs`: `TuiObserver` implements `StepObserver`, forwarding
      `on_iteration_start` / `on_step_start` / `on_event` / `on_step_finish` over an
      `mpsc` channel as `LiveMsg`.
- [ ] `tui/mod.rs::run_live`: spawn the executor on a worker thread with the observer;
      the UI thread drains the channel each tick, updates the live `RunDetail`, and draws.
      On completion, switch to the finished run's browse view.
- [ ] Header shows `iteration k/N` + spinner while running; the activity feed for the
      active step streams from `on_event`.

### Task 6: Style, help, tests, docs

- [ ] `tui/style.rs`: map Loope's palette (Claude blue, Codex orange, pass/fail) to
      ratatui `Style`. Add a `?` help overlay.
- [ ] Tests (feature build): `KeyEvent → Action` table; `RunDetail` from disk vs. from
      synthetic `StepOutcome`s; ratatui `TestBackend` snapshot of the browse + live frames.
- [ ] `cargo clippy` clean **with and without** `--features tui`; default suite unchanged;
      no external project named anywhere.
- [ ] Docs: `docs/guide/usage.md` (the `tui` feature, `loope tui`, `loope run --tui`,
      keybindings) + README (feature note, keep the std-only-by-default framing). Link the
      spec/plan from the README. Commit once.
