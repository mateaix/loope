# Loope Desktop Hub Plan

Implementation plan for [[2026-06-29-loope-desktop-hub-spec]]. The work is sequenced so the
**std-only core lands and is tested before any GUI exists**, then the desktop app is built
on top of it. Each task is a coherent, independently verifiable unit. The `loope` crate
stays `deps = 1` throughout; the desktop app is a separate workspace member with its own
dependencies, introduced only in T3.

Convention reminder: after any change to commands/flags/options/run-dir layout, update
`docs/guide/usage.md` and the README in the same change.

---

## T1 — Workspace + `hub` core scaffolding (std-only)

Restructure into a Cargo workspace and stand up the shared brain.

- Convert the repo to a workspace: move the current crate to `crates/loope/` (lib + `loope`
  bin unchanged); add a root workspace `Cargo.toml`. Verify the CLI/TUI build, test, and
  install exactly as before; `deps` for `loope` stays 1.
- Add the `loope::hub` module group with:
  - `registry.rs` — `AgentDescriptor`, `Capabilities` bitset, `AgentRegistry` with cached
    `detect()` (availability + version), generalizing today's `doctor` probing.
  - `store.rs` — the `~/.loope/` metadata store (atomic JSON read/write; create on first
    use), holding projects, session names, presets, app state.
- Unit tests: registry detection + cache TTL (with an injected prober), store round-trip.
- **Verify:** `loope` crate builds/tests/clippy green in both feature configs; deps = 1.

## T2 — Project / session model + search (std-only)

- `project.rs` — `Project` (source path + aggregated sessions); discovery merging
  `~/.loope/projects.json` with `.loope/runs` found under known roots; add/remove/hide.
- `session.rs` — `Session` over `NNNN-slug` runs: list grouped by project, friendly naming,
  resume affordance (locate the run dir + its config for a follow-up).
- `search.rs` — full-text scan of persisted run artifacts (`result`, `transcript`,
  `events.jsonl`) returning ranked matches with a preview + jump target.
- Unit tests over fixture `.loope/runs` trees: discovery, grouping, naming, search ranking.
- **Verify:** core green; no new deps.

## T3 — Extend the normalized cell model (std-only)

The single source of truth for all surfaces.

- Extend the normalized vocabulary from flat `LoopEvent` into the `Cell` model
  (`ExecCommand` with running/done/failed state + exit code, `Diff` with hunks, `Markdown`,
  `Reasoning`, `ActionLine`, `Notice`). Keep back-compat: today's `LoopEvent`s map onto
  cells; persistence (`events.jsonl`) round-trips the new kinds.
- Extend the adapter parsers to populate the richer kinds (exec output, diff hunks,
  reasoning) the CLIs already emit; existing parser tests stay green.
- Golden tests: adapter JSONL fixtures → expected cell sequences.
- Keep the ratatui TUI compiling against the extended model (it may ignore new kinds for
  now); its snapshot tests stay green.
- **Verify:** core + TUI green; round-trip + golden tests pass; deps unchanged.

## T4 — Desktop app scaffold + design tokens

First introduction of the GUI stack, isolated under `apps/desktop/`.

- Scaffold the desktop application (native backend `src-backend/` depending on `loope` as a
  path dep; component-based front-end `src-ui/`). Its own lockfile/deps; **not** part of the
  root `cargo build` default.
- Derive the GUI **design tokens** (CSS variables) from Loope's brand palette so the app
  matches the TUI; light/dark themes; the app shell (title bar, theme switch, nav).
- A first backend IPC command: list agents from `AgentRegistry` → render the `AgentSwitcher`
  with ✓/✗/version + install hints. Proves the backend → core → UI path end to end.
- **Verify:** the app launches and shows real detected agents; the `loope` crate is
  untouched (deps = 1).

## T5 — Execution bridge + live cell rendering

The heart: present the plan and the content live.

- Backend event forwarder: a `StepObserver` impl that serializes engine events/cells and
  emits them to the webview (the GUI analogue of `TuiObserver`); a `run` command that spawns
  the executor on a worker thread and streams.
- Front-end: the streaming store (reduces deltas into the active cell, commits on complete);
  `HistoryView` + per-cell components (`ExecCell`, `DiffCell`, `MarkdownCell`,
  `ReasoningCell`, `ActionCell`, `NoticeCell`) with compact/expanded forms.
- `PipelinePanel` visualizes the *plan*: chosen agents, `implement → review → verify` per
  iteration, convergence; `RunControls` start/stop (reusing the engine's cooperative cancel).
- Tests: the forwarder against synthetic `StepOutcome`s; the store reducer + each cell
  component against fixture cell streams.
- **Verify:** a real loop runs from the GUI and renders live as pipeline + typed cells.

## T6 — Session & project management UI

- `ProjectList` + `SessionList` (grouped by project, friendly names, resume) over the T2
  core; the browser route: project → session → its pipeline + cells + diff.
- Full-text search box wired to `search.rs`, with result previews and jump-to.
- **Verify:** browse + search work over real `.loope/runs`.

## T7 — Visual config editor + presets

- `config.rs` (core): `LoopOptions` presets — save/switch/backup/restore in
  `~/.loope/presets.json`.
- `ConfigForm` + `PresetManager` (front-end): every loop option as a field; the agent
  switcher feeds implementer/reviewer roles; backup before edits, one-click restore.
- The form produces the same `LoopConfig` the engine consumes.
- **Verify:** edit options + presets, run with them, restore a backup.

## T8 — Convergence card, polish, verify & docs

- The `caught & fixed` highlight (already in core) becomes the `ConvergenceCard` hero on a
  successful run; empty/error/loading states across views.
- Full verification: `loope` crate green (build/test/clippy, both feature configs, deps = 1);
  desktop app builds and runs through a full loop; component + core test suites pass.
- Docs: a "Loope Desktop" section in `docs/guide/usage.md` (what it is, how to build/run,
  data locations), README updates (feature blurb + this SDD pair under "SDD artifacts"),
  and `docs/README.md` index entry.

---

## Sequencing notes

- **T1–T3 are pure core** and ship value to the CLI/TUI too (registry, projects, search,
  richer cells) — they can land and be reviewed before any GUI decision is locked.
- **T4 is the only task that introduces the GUI dependency tree**, and it is quarantined to
  `apps/desktop/`. If the desktop direction ever changes, T1–T3 remain useful.
- Each task keeps the `loope` crate at `deps = 1` and the existing CLI/TUI suites green.

## Related

- Spec: [[2026-06-29-loope-desktop-hub-spec]]
- [[2026-06-29-loope-tui-spec]] · [[2026-06-28-loope-live-visibility-spec]] ·
  [[2026-06-29-loope-convergence-highlight-spec]]
