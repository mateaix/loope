# Loope Agent Integration Implementation Plan (v0.2)

> **For agentic workers:** implement this plan task-by-task with tests first. Steps use
> checkbox (`- [x]`) syntax for tracking. Keep every automated test hermetic — the loop
> runs through a stub adapter (`--dry-run`) so CI needs no external binaries or network.

**Goal:** Make the loop actually execute by driving the `claude` and `codex` CLIs as
subprocesses, each in a managed per-run workspace, with artifacts passed between steps
and gates evaluated against real outputs.

**Spec:** [Agent Integration Spec (v0.2)](../specs/2026-06-28-loope-agent-integration-spec.md)

**Architecture:** The library grows three layers on top of the existing planner:

1. **Adapter execution** — an `AdapterSpec` (data describing how to launch a CLI) plus an
   `Invoker` trait with two implementations: `SubprocessInvoker` (real CLIs) and
   `StubInvoker` (deterministic, used by `--dry-run` and all tests).
2. **Workspace** — a `RunWorkspace` that owns `.loope/runs/<run-id>/`, seeds the shared
   `workspace/` tree, hands each agent a private `home/`, and does atomic writes.
3. **Executor** — `execute_plan` walks the plan, builds prompts (with upstream
   artifacts), invokes each step, persists prompt/transcript/result, evaluates gates,
   and produces a `LoopRun` + `report.md`.

The CLI gains `run`, `runs`, and `show`. The planner, `plan`, and `adapters` are
unchanged.

**Tech Stack:** Rust 2024 edition, standard library only, Cargo tests. No new crates.

---

## Tasks

### Task 1: Adapter spec + invocation model (lib)

- [x] Add `AdapterSpec` describing each adapter: program, env-var override, invocation
      mode (headless), prompt delivery, output format, `write_capable`.
- [x] Resolve the program name from env (`LOOPE_CLAUDE_BIN`, `LOOPE_CODEX_BIN`) with a
      default and a "missing binary" outcome.
- [x] Define `AgentInvocation` (prompt, workspace dir, home dir, read-only flag) and
      `InvocationResult` (final message, changed files, success, raw transcript).
- [x] Define an `Invoker` trait: `invoke(&AgentInvocation) -> InvocationResult`.
- [x] Unit tests for env override resolution and spec lookup per adapter.

### Task 2: Stub invoker (hermetic execution)

- [x] Implement `StubInvoker` producing deterministic, role-aware results (implementer
      "writes" a stub change, reviewer emits a blockers-first review, verifier reports
      checks). No randomness, no clock, no I/O beyond the provided dirs.
- [x] Unit tests asserting stub output is deterministic and role-appropriate.

### Task 3: Run workspace (lib)

- [x] Implement `RunWorkspace` rooted at `.loope/runs/<run-id>/`; allocate the next
      `run-NNNN` id by scanning existing runs.
- [x] Seed `workspace/` from a source dir (copy by default; in-place option records the
      real path without copying).
- [x] Create a private `agents/<role>-<adapter>/home/` per step; sanitize/hash any id
      used in a path so it cannot escape the run root.
- [x] Atomic write helper (temp file in same dir + rename) for all run state files.
- [x] Unit tests for id allocation, path sanitization, and atomic writes (use a temp
      base dir, not the repo).

### Task 4: Executor + artifact passing (lib)

- [x] Implement `execute_plan(plan, &mut RunWorkspace, &dyn Invoker, options) ->
      LoopRun`, looping over steps.
- [x] Build each step's prompt from role + requirement + upstream artifacts (reviewer
      gets the implementer result; second implement turn gets the review).
- [x] Persist `prompt.md`, `transcript.jsonl`, `result.md` per step; write `run.json`.
- [x] Evaluate gates against real results; a blocking failure halts the loop.
- [x] `LoopRun::to_report_markdown()` + `all_passed()`; write `report.md`.
- [x] Unit tests: full loop via `StubInvoker`, artifact-passing assertions, and a
      forced gate failure halting the loop.

### Task 5: Subprocess invoker (real CLIs)

- [x] Implement `SubprocessInvoker`: launch the resolved program in headless mode with
      the workspace as cwd, the private home via env, prompt via arg/stdin, read-only
      where not `write_capable`; capture stdout/stderr with a timeout.
- [x] Parse the captured stream (JSON lines where available, else raw text) into
      `InvocationResult`; map a missing binary / nonzero exit to a failed step.
- [x] Keep this path out of automated tests (documented manual verification only).

### Task 6: CLI wiring

- [x] Add `loope run [--design] [--workdir DIR] [--in-place] [--approve auto|manual]
      [--dry-run] <requirement>`; choose `StubInvoker` for `--dry-run`/missing binaries,
      else `SubprocessInvoker`.
- [x] Add `loope runs` (list runs) and `loope show <run-id>` (print report).
- [x] Implement `--approve manual` (print prompt + workspace, wait for confirmation).
- [x] Update `print_help` with the new commands and flags.

### Task 7: Tests + verification

- [x] Integration test: `loope run --dry-run "..."` into a temp base dir; assert the run
      directory layout, `report.md` contents, and exit code.
- [x] Integration test: `loope runs` and `loope show` against a produced run.
- [x] Ensure existing v0.1 tests still pass; `cargo test` green with no binaries/network.
- [x] Run `cargo run -- run --dry-run "Add login"` and confirm the produced run dir.

### Task 8: Docs

- [x] Update `README.md`: `run`/`runs`/`show`, `--dry-run`, workspace layout, and a
      manual checklist for the real-CLI path.
- [x] Link this plan and the integration spec from the README SDD section.
