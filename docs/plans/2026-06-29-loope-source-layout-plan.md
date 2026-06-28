# Loope Source Layout Implementation Plan

> Pure refactor — relocate files into domain directories with facades. Use `git mv` to
> preserve history. Verify byte-identical run output. Commit once at the end.

**Spec:** [Source Layout Spec](../specs/2026-06-29-loope-source-layout-spec.md)

**Tech Stack:** Rust 2024, standard library only. No new crates.

---

## Tasks

### Task 1: Extract the domain model into `model.rs`

- [ ] `git mv` is not applicable (code lives inline in `lib.rs`); move the domain block
      (`Adapter`, `Role`, `LoopOptions`, `LoopStep`, `LoopPlan`, `list_adapters`,
      `generate_plan`, `generate_design_plan`, `prompt_for_step`, and their tests) into a
      new `src/model.rs`.
- [ ] `lib.rs` becomes thin: `pub mod model; pub mod adapter; pub mod engine;` plus
      `pub use model::{…}` re-exporting the domain vocabulary at the crate root.

### Task 2: Group the adapter domain

- [ ] `git mv src/event.rs src/adapter/event.rs`, `src/stub.rs src/adapter/stub.rs`,
      `src/subprocess.rs src/adapter/subprocess.rs`.
- [ ] In `adapter.rs` add `pub mod event; pub mod stub; pub mod subprocess;` and the
      facade re-exports (`StubInvoker`, `SubprocessInvoker`, `LoopEvent`).
- [ ] Fix intra-module paths: `crate::event` → `crate::adapter::event` in the moved
      files and consumers.

### Task 3: Group the engine domain

- [ ] `git mv src/executor.rs src/engine/executor.rs`, `src/workspace.rs
      src/engine/workspace.rs`, `src/review.rs src/engine/review.rs`.
- [ ] Create `engine.rs`: `pub mod executor; pub mod workspace; pub mod review;` + the
      executor facade re-exports.
- [ ] Fix paths inside the engine: `crate::workspace` → `crate::engine::workspace`,
      `crate::review` → `crate::engine::review`, `crate::event` →
      `crate::adapter::event`. Domain types stay `crate::{Adapter, Role, …}` (root
      re-export).

### Task 4: Group the binary's presentation under `cli`

- [ ] `git mv src/ui.rs src/cli/ui.rs`, `src/theme.rs src/cli/theme.rs`; create
      `src/cli.rs` with `pub mod ui; pub mod theme;`.
- [ ] `main.rs`: replace `mod theme; mod ui;` with `mod cli;` and reference
      `cli::ui` / `cli::theme`; fix `ui.rs`'s `crate::theme` → `crate::cli::theme`.
- [ ] Update the binary's library paths: `loope::executor` → `loope::engine`,
      `loope::stub`/`loope::subprocess` → `loope::adapter`, `loope::workspace` →
      `loope::engine::workspace`, `loope::event` → `loope::adapter::event`.

### Task 5: Verify + docs

- [ ] `cargo build`, `cargo test`, `cargo clippy --all-targets` all clean; no new crates.
- [ ] Before/after `loope run --dry-run "Add login"`: `report.md` + `run.json` identical.
- [ ] Update `docs/guide/usage.md` (or `docs/README.md`) if it references the source
      layout; link this spec/plan from the README SDD section. Commit once.
