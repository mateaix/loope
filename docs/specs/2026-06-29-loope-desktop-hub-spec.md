# Loope Desktop Hub Spec (multi-agent management + visual execution)

## Background

Loope today is a terminal tool: a std-only Rust **core** (the loop engine, adapters, run
workspaces) plus two front-ends over it — the streaming print feed (`loope run`) and the
interactive ratatui TUI (`loope`, `loope tui`, `loope run --tui`). Both front-ends are
*consumers of one event stream*: the engine drives a `StepObserver` and emits a small
normalized `LoopEvent` vocabulary (`Model` / `Action` / `Message` / `Usage`), which the
print renderer and the `TuiObserver` each render their own way. The engine itself is
entirely UI-agnostic.

That architecture has a natural next step the terminal cannot reach. Developers now juggle
several CLI coding agents (Claude, Codex, OpenCode) across many projects, and the most
valuable thing Loope produces — a loop of *implement → review → verify* converging on a
fix — is exactly the kind of rich, branching, diff-and-reasoning-heavy content that a
graphical surface presents far better than a scrolling terminal. Diffs want color and
collapse; reasoning wants to fold away; a multi-iteration pipeline wants to be a diagram,
not a log.

This spec adds **Loope Desktop**: a graphical desktop application that (1) **manages the
several agent CLIs** as first-class, switchable tools, and (2) **visually presents both the
loop's execution *plan* and the agents' execution *content*** — the commands they run, the
files they edit, the diffs they produce, their reasoning and messages — beautifully and
live.

The guiding invariant: **one event vocabulary, three surfaces.** Loope Desktop is a *third
consumer* of the same engine stream that already powers the print feed and the TUI. The
engine and the core stay UI-agnostic; the desktop app subscribes to the stream just as the
`TuiObserver` does, then renders it with graphical components.

## Goals

1. **A desktop GUI** that opens to a home workspace (pick a project, type a requirement,
   run the loop) and presents runs graphically — replacing scattered terminals with one
   focused window.
2. **Multi-agent registry**: detect the installed agent CLIs, show availability + version,
   surface install hints for missing ones, and let the user choose which agent implements
   and which review — visually, instead of flags.
3. **Visual execution rendering**: render the loop's *plan* (the implement → review →
   verify pipeline, iterations, convergence) and the agents' *content* as a stream of typed
   **cells** — exec command + output, diffs, assistant markdown, reasoning, errors — live
   as the run executes and in replay.
4. **Session & project management**: projects (source paths) aggregate their runs
   (sessions); browse runs grouped by project, name and resume them, and full-text search
   across past run transcripts.
5. **Visual configuration**: a form-based editor for the loop options (iterations, agents,
   verify command, design toggle, permission mode, env) with named, switchable **presets**
   and backup/restore — instead of remembering flags.
6. **No regression to the core's identity.** The `loope` library and CLI stay std-only
   (the `deps = 1` badge stays honest). The desktop app is a **separate, opt-in workspace
   member** with its own dependency tree; building or shipping it never touches the core's
   dependency set. The print feed and ratatui TUI are unchanged.

## Non-Goals (v1)

- An embedded raw terminal emulator (we render the *normalized* stream, not a PTY).
- Multimodal authoring beyond attaching an image to a requirement (no in-app annotation).
- Runtime/dynamic agent plugins — the agent registry is compile-time (data-driven, but not
  hot-loaded).
- Mobile or web-hosted deployment; this is a desktop application.
- Replacing the CLI or TUI. All three front-ends remain first-class over the same core.
- Windows-specific tuning beyond what the cross-platform desktop framework gives for free
  (we target the macOS/Linux setups Loope already supports first).

## Core Concepts

### Workspace layout (the elegant split)

Loope becomes a small **Cargo workspace** so the desktop app can depend on the core without
contaminating the core's dependencies. The existing `loope` crate stays at the repo root and
*is* the workspace root (a root-package workspace — no files move); the desktop app joins as
a member that depends on `loope` by path:

```text
loope/                         workspace root = the `loope` crate (std only, deps = 1)
  src/                         engine · adapters · model · cli · the new `hub` module group
  Cargo.toml                   [package] loope + [workspace]
  apps/
    desktop/                   the desktop application — its own dependency tree
      src-backend/             thin native backend: depends on `loope` (path dep), exposes
                               the hub over IPC, forwards the engine event stream to the view
      src-ui/                  the component-based front-end (typed cell model + components)
```

- The `loope` crate keeps `default = []` (std only). Its badge and tests are unchanged.
- `apps/desktop` is **not** built by `cargo build` at the root by default; it has its own
  build entry. The core never imports anything from it.
- The desktop backend is *glue*: every capability lives in the std-only `loope::hub` core
  (so it is unit-testable without a GUI and reusable by the CLI/TUI); the backend just maps
  hub functions to IPC commands and pipes the event stream to the webview.

### The shared brain: `loope::hub` (std-only core)

All hub capability is implemented in the core as a new module group, UI-agnostic and
dependency-free, so the GUI backend stays thin and the logic stays testable:

```text
src/hub/
  registry.rs    AgentRegistry: descriptors, detection (version + availability), capabilities,
                 install hints. Generalizes today's CLI probing into a queryable registry.
  project.rs     Project model: a source path + its aggregated runs (sessions); discovery.
  session.rs     Session = a run; naming, grouping, resume affordances over .loope/runs.
  search.rs      Full-text search across persisted run transcripts/results/events.
  store.rs       Hub metadata store under ~/.loope/ (projects, session names, presets, state).
  config.rs      LoopOptions presets: named bundles, switch, backup/restore.
```

Data-location split (mirrors how the core already persists runs):

| Path | Holds |
| --- | --- |
| `<project>/.loope/runs/NNNN-slug/` | per-run artifacts (already exists): steps, diffs, transcript, events.jsonl, highlight |
| `~/.loope/` | hub metadata: `projects.json`, `session-names.json`, `presets.json`, `state.json` |

Agent data (`~/.claude/`, `~/.codex/`, …) is read where the agents already keep it; the hub
never copies or owns it.

### Multi-agent registry

A descriptor per supported agent, owned by the core, rendered by the GUI:

```text
AgentDescriptor {
  id, display_name,
  binary,                      the CLI the adapter drives
  capabilities: Capabilities,  bitset: streams text deltas / tool calls / reasoning /
                               images / resume / config — so the UI adapts, never hardcodes
  install_hint,                e.g. the package to install when missing
  detected: Option<Detected>,  availability + version, probed with a short TTL cache
}
```

- `AgentRegistry::detect()` generalizes today's `loope doctor` probing into a cached,
  queryable list. The GUI shows each agent with ✓ / ✗ / version and an install hint when
  missing.
- The user picks the **implementer** and the **reviewer(s)** from available agents; this is
  exactly the `LoopOptions` the CLI already exposes, surfaced as a visual switcher. The
  registry is the single source of truth for "which agents exist and what can they do."

### Visual execution: the cell stream

The terminal renders the event stream as text; the GUI renders it as a list of typed
**cells**. We extend the core's normalized vocabulary from today's flat `LoopEvent` into a
small, explicit **cell model** that is the single source of truth for *all three* surfaces
(the TUI can adopt the richer cells incrementally; the print feed already handles the
existing subset):

```text
Cell =
  | ExecCommand { command, output, exit_code, state: Running | Done | Failed }
  | Diff        { file, change: Add|Modify|Delete, hunks }
  | Markdown    { text }            assistant output
  | Reasoning   { text }            "thinking" — folded by default
  | ActionLine  { kind, target }    today's Action (read/edit/write/run/search)
  | Notice      { level, text }     model banner, usage, errors
```

Rendering principles, adapted as *concepts* from mature terminal-agent UIs into graphical
components:

- **One component per cell kind** (`ExecCell`, `DiffCell`, `MarkdownCell`, `ReasoningCell`,
  `ActionCell`, `NoticeCell`) — a discriminated union in the view layer, each variant
  self-contained (its own render, its own collapse/expand). New cell kinds are additive.
- **Compact display vs. full transcript.** Each cell renders a compact form in the live
  pipeline view and a complete form in a transcript drawer (long output is summarized with
  an expander, never lost).
- **Streaming with commit-on-complete.** Deltas mutate a single *active* cell in place;
  markdown commits at block boundaries (so half-formed blocks never flash); exec output
  appends until the command finishes, then the cell freezes with its exit code and timing.
- **The plan is a diagram.** Above the cell stream, a **pipeline panel** visualizes the loop
  *plan*: the agents chosen, `implement → review → verify` per iteration, and convergence —
  so the user sees the *scheme* of the run, not just its output. The existing convergence
  **highlight** (`caught & fixed`) becomes a hero card.

The adapter parsers already classify the CLIs' JSONL into normalized events; v1 extends
them to populate the richer cell kinds (exec output, diff hunks, reasoning) the agents
already emit.

### Session & project management

- A **Project** is a source path Loope has run against; it aggregates that path's runs.
  Discovery merges the `~/.loope/projects.json` list with any `.loope/runs` found under
  known roots.
- A **Session** is a run (`NNNN-slug`). Sessions are browsable grouped by project, can be
  given a friendly name (stored in `~/.loope/session-names.json`), reopened to inspect, and
  used as the basis for a follow-up run.
- **Full-text search** scans the persisted run artifacts (`result`, `transcript`,
  `events.jsonl`) and returns matches with a preview and a jump target — implemented in the
  std-only core so the CLI could expose it too.

### Visual configuration

The loop options the CLI exposes via flags become a form:

- Fields: iterations, implementer, reviewer(s), verify command, design-contract toggle,
  dry-run, permission mode, env, per-adapter overrides.
- **Presets**: named `LoopOptions` bundles in `~/.loope/presets.json`, switchable in one
  click, with automatic backup before edits and one-click restore.
- The preset/agent selections feed the same `LoopConfig` the engine already consumes — the
  form is a view over existing options, not a new configuration system.

### Componentization & theming

- **Backend** is a flat set of IPC commands that map 1:1 to `loope::hub` functions, plus one
  event forwarder: a `StepObserver` implementation that serializes engine events/cells and
  emits them to the webview — the GUI's analogue of `TuiObserver`. Thin and testable.
- **Front-end** is a component library: `AgentSwitcher`, `ProjectList`, `SessionList`,
  `PipelinePanel`, `HistoryView` + the per-cell `*Cell` components, `ConfigForm`,
  `PresetManager`, `RunControls`, `ConvergenceCard`. A small store subscribes to the event
  channel and reduces deltas into the active cell (the same shape as the TUI's accumulator).
- **One identity, two surfaces.** The visual language is **Liquid Glass** — a translucent,
  layered, specular-lit material system — defined in full (tokens, materials, motion, and a
  surface-by-surface mapping from the TUI) in [[2026-06-29-loope-liquid-glass-design-spec]].
  Loope's brand palette and semantics are the accent/agent/state colors, so the terminal and
  the desktop app are obviously the same tool. Light / dark themes. Every desktop surface is
  a TUI capability restyled as glass — the functionality is inherited, not reinvented.

## User-Facing Surface (additions)

- A desktop application launcher (the existing `loope` CLI/TUI are unchanged).
- Home: project picker + requirement prompt + agent switcher + "run".
- Live run: the pipeline panel (plan) over the streaming cell history (content), with a
  stop control (reusing the engine's cooperative cancel) and a convergence card on success.
- Browser: projects → sessions → a run's pipeline + cells + diff, with full-text search.
- Settings: the config form + preset manager + agent registry view with install hints.

## Acceptance Criteria

- `cargo build` / `cargo test` at the workspace root for the **`loope` crate** compile with
  **no new dependencies** (deps still 1, std only); the existing CLI/TUI test suites pass
  unchanged; `cargo clippy` clean with and without `--features tui`.
- The new `loope::hub` core (registry, project/session model, search, store, config presets)
  is std-only and unit-tested without any GUI.
- The desktop app builds from its own entry point, opens, detects the agents (✓/✗/version +
  install hints), runs a loop against a chosen project, and renders the run live as a
  pipeline + typed cells (exec/diff/markdown/reasoning), ending in a convergence card on
  success — driven entirely by the engine's `StepObserver` stream.
- Projects/sessions browse and full-text search work over real `.loope/runs`; the config
  form edits options and presets with backup/restore.
- No reference to any external project by name anywhere in the tree (code, comments, docs,
  commit messages).

## Testing Strategy

- **Core (default build, no GUI):** unit tests for the registry detection/cache, project &
  session discovery and grouping, full-text search ranking, the preset store
  (save/switch/backup/restore), and the extended cell model's serialize/parse round-trip.
- **Cell projection:** golden tests that adapter JSONL fixtures project into the expected
  cell sequence (exec output, diff hunks, reasoning) — pure, no process spawning.
- **Backend:** the IPC command layer tested against the core with a stub registry/invoker;
  the event forwarder tested by feeding synthetic `StepOutcome`s and asserting the emitted
  cell events.
- **Front-end components:** each `*Cell` and the `HistoryView` reducer tested against fixture
  cell streams (running → done exec, growing diff, streamed markdown commit boundaries).
- **Regression:** the std-only `loope` suite and the ratatui TUI snapshot tests stay green;
  a CI check asserts the `loope` crate's dependency count is unchanged.

## Related

- [[2026-06-29-loope-tui-spec]] — the ratatui front-end; the desktop app is a third consumer
  of the same `StepObserver` stream, not a replacement.
- [[2026-06-28-loope-live-visibility-spec]] — the normalized `LoopEvent` vocabulary the cell
  model extends.
- [[2026-06-28-loope-live-rendering-spec]] — the streaming renderer whose diff/exec concepts
  the cell components elevate graphically.
- [[2026-06-29-loope-convergence-highlight-spec]] — the `caught & fixed` highlight that
  becomes the desktop convergence card.
- [[2026-06-28-loope-agent-integration-spec]] — the adapters/invocation the registry
  generalizes.
- [[2026-06-29-loope-source-layout-spec]] — the domain-grouped core the `hub` module joins.
