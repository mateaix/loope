# Loope TUI Spec (interactive terminal UI)

## Background

Loope already renders a polished **streaming print feed** during a run (spinner, live
activity, per-step results) and prints reports/diffs for `show`. But the output is
*linear and ephemeral*: once a run scrolls past you cannot navigate it, scroll a long
diff, drill into a step's transcript, or compare iterations. There is no way to browse
the history of runs interactively, and watching a long multi-iteration run means staring
at a scrolling log.

This spec adds an **interactive full-screen TUI** with two entry points:

- `loope tui` — an interactive **browser** over `.loope/runs/`: pick a run, see its steps
  grouped by iteration, and inspect each step's result, diff, and transcript in a preview
  pane.
- `loope run --tui` — a **live dashboard** that watches the loop execute in full screen
  (iterations, steps, streaming actions, cumulative diff) and lands in the browser view
  when it converges.

The TUI is built with **ratatui + crossterm** — the de-facto-standard Rust TUI stack —
behind an **optional `tui` cargo feature**, so the default build and the `loope` library
stay dependency-free (std only). UX is modeled on high-star ratatui apps: a list+preview
layout (à la yazi/atuin) with a multi-pane focus model and a key-hint footer (à la
gitui).

## Goals

1. `loope tui` opens a full-screen interactive browser of past runs with keyboard
   navigation, a run list, a per-run step view (grouped by iteration), and a preview pane
   that shows the selected step's result / cumulative diff / transcript.
2. `loope run --tui` shows the **same** views live as the loop runs, updating on every
   step and event, then settles into the browser view for the finished run.
3. The TUI lives **only** in the binary, behind a `tui` feature. The default build
   (`cargo build`) compiles with **zero new dependencies**; the `loope` *library* never
   references ratatui/crossterm. `cargo build --features tui` (or `cargo install --features
   tui`) enables it.
4. Live mode reuses the engine's existing [`StepObserver`] abstraction: a TUI observer
   forwards events over a channel to the UI thread while the executor runs on a worker
   thread. The engine stays entirely UI-agnostic.
5. Elegant, idiomatic ratatui: an immediate-mode `App` + `draw(frame, &app)` with a small
   `Action` event vocabulary, a disk/live-agnostic view model, and Loope's brand palette
   mapped to ratatui styles.
6. No regressions: the default-feature suite passes unchanged; `cargo clippy` clean in
   both feature configurations; the existing print-feed renderer is untouched and remains
   the default for `loope run`.

## Non-Goals (v1)

- Editing, re-running, or applying runs from inside the TUI (browse/inspect only; `apply`
  stays a CLI command). A later version may add an "apply" key.
- Mouse-driven interaction beyond optional wheel scroll; the TUI is keyboard-first.
- Windows-specific terminal tuning (crossterm is cross-platform, but we target the
  macOS/Linux terminals Loope already supports).
- Replacing the streaming print feed; `loope run` without `--tui` keeps today's behavior.

## Core Concepts

### Feature gating

```toml
[features]
tui = ["dep:ratatui", "dep:crossterm"]

[dependencies]
ratatui   = { version = "0.29", optional = true }
crossterm = { version = "0.28", optional = true }
```

- Default features: none → std-only, badge stays honest.
- All TUI code is under `src/cli/tui/` and `#[cfg(feature = "tui")]`.
- `loope tui` / `loope run --tui` are recognized always; without the feature they print a
  one-line hint: *"the TUI requires a build with `--features tui`"* and exit `2`.

### Architecture

```text
src/cli/tui/                     (#[cfg(feature = "tui")])
  mod.rs        entry points (browse / live) + terminal lifecycle (RAII restore) + loop
  app.rs        App state: mode, panes, selection/scroll, loaded run, live channel
  action.rs     Action enum + crossterm KeyEvent → Action mapping (keybindings)
  model.rs      view model: RunEntry, RunDetail, StepView — built from disk OR live events
  view/
    mod.rs      draw(frame, &app): header + body(list | detail) + footer
    runs.rs     the run-list pane
    detail.rs   the per-run step list (grouped by iteration) + preview
    diff.rs     scrollable colored diff widget
  observer.rs   TuiObserver: impl StepObserver, forwards to the App over a channel
  style.rs      Loope palette → ratatui Style/Color
```

- **Immediate mode.** `App` holds all state; the event loop is `draw → wait for event →
  update`. Rendering is a pure function of `App`.
- **One view model, two sources.** `RunDetail`/`StepView` are populated either by reading
  `.loope/runs/<id>/` from disk (browse) or by accumulating `StepOutcome`s from the live
  observer. The view layer does not care which.
- **Library stays clean.** `TuiObserver` (binary) implements the library's `StepObserver`
  and sends over an `mpsc` channel; the executor runs on a worker thread. No ratatui type
  ever crosses into `loope` the library.

### Views & layout

```text
┌ ∞ loope · run-0004 · converged · 3 steps ─────────────────────────┐  header
├─────────────┬─────────────────────────────────────────────────────┤
│ runs        │ iteration 1                                          │
│ > run-0004  │  ✓ 1 implementer · Claude   change produced  +10 −0  │
│   run-0003  │  ✓ 2 reviewer    · Codex    PASS                     │  body:
│   run-0002  │  ✓ 3 verifier    · Generic  verification passed      │  list | detail
│             │ ── preview ─────────────────────────────────────────│
│             │  diff src/lib.rs                                     │
│             │  @@ -0,0 +1,3 @@                                     │
│             │  + pub fn multiply…                                  │
├─────────────┴─────────────────────────────────────────────────────┤
│ ↑/↓ move · → enter · ← back · tab pane · d diff · t transcript · q │  footer
└────────────────────────────────────────────────────────────────────┘
```

- **Browse:** left = run list; right = the selected run's steps; a preview region shows
  the focused step's result / diff / transcript.
- **Live:** identical chrome; the header shows `iteration k/N` and a spinner; steps stream
  in and update in place; the cumulative diff grows. On convergence the footer/header
  switch to the finished state and normal browse navigation resumes.

### Keybindings

| Key | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | move selection |
| `→`/`l`/`Enter` | enter / focus detail |
| `←`/`h`/`Esc` | back / focus list |
| `Tab` | cycle focus pane |
| `d` | toggle the diff preview |
| `t` | toggle the transcript preview |
| `g`/`G` | top / bottom |
| `PgUp`/`PgDn` | scroll preview |
| `r` | refresh run list (browse) |
| `?` | help overlay |
| `q`/`Ctrl-C` | quit |

A small `action.rs` maps `KeyEvent → Action`; the App interprets `Action`s. (A
user-configurable keymap is out of scope for v1 but the indirection leaves room for it.)

## CLI Surface (additions)

```bash
loope tui                         # browse .loope/runs interactively
loope run --tui "..."             # run the loop in a live full-screen dashboard
```

`--tui` is mutually exclusive with the print feed; it implies a TTY (errors out cleanly
if stdout is not a terminal). Without the `tui` feature, both paths print the rebuild
hint and exit `2`.

## Acceptance Criteria

- `cargo build` (no features) compiles with **no new dependencies**; the `loope` library
  has zero ratatui/crossterm references; the full existing test suite passes.
- `cargo build --features tui` builds the TUI; `loope tui` browses runs and `loope run
  --tui` watches a run live, both with the keybindings above; quitting restores the
  terminal cleanly (alternate screen left, cursor shown) even on error/panic.
- Live mode reflects each step/iteration as it happens via the `StepObserver` channel and
  ends in the finished run's browse view.
- `cargo clippy` is clean with and without `--features tui`; no reference to any external
  project by name anywhere in the tree.

## Testing Strategy

- **Pure model tests** (default build, no feature): loading a `RunDetail` from a run
  directory; accumulating `StepView`s from synthetic `StepOutcome`s; both yield the same
  shape.
- **Action mapping tests** (feature build): `KeyEvent → Action` table (arrows/vim keys,
  Tab, d/t, q, Ctrl-C).
- **Render snapshot tests** (feature build): ratatui `TestBackend` renders the browse and
  live frames for a fixed `App` state; assert key cells/labels appear. No real terminal.
- **Lifecycle**: a RAII terminal guard restores the screen on drop; a panic hook restores
  before unwinding (manual check + a guard unit test).
- CI/default remains hermetic and dependency-free.

## Related

- [[2026-06-28-loope-live-rendering-spec]] — the streaming print renderer the TUI
  complements (not replaces).
- [[2026-06-28-loope-iterative-loop-spec]] — the iterations/steps the TUI visualizes.
- [[2026-06-29-loope-source-layout-spec]] — the `cli` domain the TUI extends.
