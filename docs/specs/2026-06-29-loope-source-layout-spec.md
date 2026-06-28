# Loope Source Layout Spec (module reorganization)

## Background

`src/` is a flat pile of eleven `.rs` files — `lib.rs`, `main.rs`, `adapter.rs`,
`event.rs`, `executor.rs`, `review.rs`, `stub.rs`, `subprocess.rs`, `workspace.rs`,
`ui.rs`, `theme.rs` — with no grouping. The crate root (`lib.rs`) also carries the whole
domain model (`Adapter`, `Role`, `LoopPlan`, `LoopStep`, `LoopOptions`, plan generation)
inline. As the codebase has grown (executor/ui/subprocess/workspace are each 650–960
lines) the flat layout no longer communicates the architecture: which files are the
**domain vocabulary**, which **drive agents**, which **run the loop**, and which are
**binary-only presentation**.

This change reorganizes the source into directory-grouped modules following idiomatic
Rust (the 2018+ *parent-file + sibling-directory* convention, no `mod.rs`), with each
parent module presenting a small facade. It is a **pure refactor**: no behavior, CLI
surface, output, or run-directory format changes.

## Goals

1. Group source files by architectural role into four domains: **model**, **adapter**,
   **engine**, **cli**.
2. Use the idiomatic `foo.rs` + `foo/` layout (no `mod.rs` files); each parent module
   re-exports its children's primary types as a facade.
3. Keep the crate root (`lib.rs`) thin: the module tree plus a re-export of the domain
   vocabulary, so `loope::Adapter` / `loope::Role` / `loope::generate_plan` stay stable.
4. Preserve git history for moved files (`git mv`).
5. Zero behavior change: identical CLI, output, artifacts, and exit codes; the full test
   suite passes unchanged (only `use` paths in tests update); `cargo clippy` clean; no
   new crates.

## Non-Goals

- No renaming of public types or functions (only their module *location* changes).
- No splitting of large files into smaller ones (that is separate follow-up work).
- No change to the binary's behavior, flags, or rendered output.
- No new abstractions, traits, or indirection — this is relocation + facades only.

## Target layout

```text
src/
  lib.rs                  crate root: module tree + domain re-exports (thin)
  main.rs                 binary entry; declares `mod cli;`

  model.rs                the loop vocabulary & plan generation (no I/O):
                            Adapter, Role, LoopOptions, LoopStep, LoopPlan,
                            list_adapters, generate_plan, generate_design_plan,
                            prompt_for_step

  adapter.rs              agent-integration facade: Invoker, AgentInvocation,
                          InvocationResult; `pub mod event/stub/subprocess;`
                          + `pub use stub::StubInvoker;`
                          + `pub use subprocess::SubprocessInvoker;`
                          + `pub use event::LoopEvent;`
  adapter/
    event.rs              normalized agent events + JSONL serialization
    stub.rs               hermetic StubInvoker (dry-run / tests)
    subprocess.rs         SubprocessInvoker (drives the real CLIs)

  engine.rs               execution facade: `pub mod executor/workspace/review;`
                          + `pub use executor::{LoopConfig, LoopRun, StepObserver,
                            StepOutcome, StopReason, execute_loop};`
  engine/
    executor.rs           execute_loop + the iteration model
    workspace.rs          RunWorkspace, snapshots, diffs
    review.rs             ReviewVerdict + parse_review_verdict

  cli.rs                  binary-only presentation: `pub mod ui; pub mod theme;`
  cli/
    ui.rs                 banner, pipeline, live renderer, report/diff printing
    theme.rs              color capability + token helpers
```

### Domain taxonomy & rationale

- **model** — the pure vocabulary of a loop (roles, adapters, the plan and its steps,
  prompt text). No filesystem, process, or rendering. Everything else depends on it.
- **adapter** — *how an agent is invoked and what it emits.* The `Invoker` trait and its
  two implementations (`stub`, `subprocess`) plus the normalized `event` stream they
  produce. Events live here because adapters are what produce them.
- **engine** — *running the loop.* `executor` orchestrates iterations; `workspace`
  manages the per-run directory, snapshots, and diffs; `review` interprets reviewer
  verdicts for the convergence gate. The engine depends on `model` and `adapter`.
- **cli** — binary-only presentation (`ui`, `theme`). Not part of the library API.

## Module paths (before → after)

| Item | Before | After |
| --- | --- | --- |
| Domain types/fns | `loope::Adapter`, `loope::Role`, `loope::generate_plan`, … | unchanged (re-exported at crate root from `model`) |
| Events | `loope::event` | `loope::adapter::event` (`loope::adapter::LoopEvent` facade) |
| Stub invoker | `loope::stub::StubInvoker` | `loope::adapter::StubInvoker` |
| Subprocess invoker | `loope::subprocess::SubprocessInvoker` | `loope::adapter::SubprocessInvoker` |
| Executor | `loope::executor::{execute_loop, …}` | `loope::engine::{execute_loop, …}` |
| Workspace | `loope::workspace::RunWorkspace` | `loope::engine::workspace::RunWorkspace` |
| Review | `loope::review::*` | `loope::engine::review::*` |
| UI / theme | `crate::{ui, theme}` (binary) | `crate::cli::{ui, theme}` (binary) |

The crate root re-exports the domain vocabulary, so the pervasive `use crate::{Adapter,
Role, LoopStep}` / `loope::Adapter` references do not change. The handful of explicit
module-path references (`crate::event`, `crate::workspace`, `crate::review`,
`crate::stub`; and the binary's `loope::executor`/`loope::stub`/etc.) update to the new
canonical paths — there is no backward-compatibility aliasing of module paths (that would
defeat the point).

## Acceptance Criteria

- `src/` contains the four-domain tree above; `git log --follow` works on every moved
  file (history preserved).
- No `mod.rs` files; each parent uses the `foo.rs` + `foo/` convention with a facade
  re-export.
- `lib.rs` declares only `pub mod model; pub mod adapter; pub mod engine;` plus the
  domain re-export; the domain model code lives in `model.rs`.
- `cargo build`, `cargo test`, and `cargo clippy --all-targets` are clean; the test
  suite passes with only `use`-path edits; no new dependencies.
- `loope run --dry-run` produces byte-identical report/run.json/run-dir structure as
  before the change.

## Testing Strategy

- The existing unit + integration suites are the regression net: they exercise plan
  generation, the executor, workspace diffs, adapters, and the CLI end to end. They must
  pass after only `use`-path updates.
- Before/after `loope run --dry-run "Add login"` diff of `report.md` + `run.json` to
  confirm identical output.
- `cargo clippy --all-targets` clean; `cargo tree` shows no new crates.

## Related

- [[2026-06-28-loope-iterative-loop-spec]] — the v1.0 engine this reorganizes.
