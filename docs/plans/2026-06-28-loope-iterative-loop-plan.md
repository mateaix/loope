# Loope Iterative Loop Implementation Plan (v1.0)

> Implement task-by-task with tests first. Keep the suite hermetic (the stub converges
> in one iteration). Commit once at the end.

**Spec:** [Iterative Loop Spec (v1.0)](../specs/2026-06-28-loope-iterative-loop-spec.md)

**Goal:** Turn the fixed pipeline into a real convergence loop, make its output
actionable, and gate on the Design Contract.

**Architecture:** Factor a single-step primitive (`execute_one`) out of the executor,
then drive it from an iteration loop (`execute_loop`) that runs design once, then repeats
implement/fix → review → verify with feedback until it converges or hits `--max-iters`.
A run-level cumulative diff plus a `loope apply` command land the result.

**Tech Stack:** Rust 2024, standard library only. No new crates.

---

## Tasks

### Task 1: Single-step primitive + iteration core (P0)

- [ ] Factor `execute_one(workspace, step, prompt, invoker, observer, before_content,
      verify_command)` from the current executor: prompt/home, observer, invoke or verify
      command, persist transcript/result/events, change detection, gate, outcome (with
      duration, changes, verdict).
- [ ] `StepOutcome` gains `iteration: usize`.
- [ ] `execute_loop(config, workspace, invoker, observer)`: design once (if set), then
      iterate implement → review(s) → verify; evaluate convergence; stop on converged /
      max-iters / hard failure. `LoopRun` gains `iterations` and `stop_reason`.
- [ ] `LoopOptions.max_iters` (default 3); CLI `--max-iters N`.
- [ ] Rework executor unit tests around the iteration model.

### Task 2: Feedback + convergence gating (P0)

- [ ] Verify gates on real success (a `--verify-cmd` must exit 0); with no command,
      verification is informational (passing).
- [ ] Compose a feedback block (review blockers + verify failure output) and append it to
      the next iteration's implementer prompt.
- [ ] Unit test: a stub that blocks iteration 1 and passes iteration 2 runs exactly two
      iterations and the fix prompt carries the fed-back text; a hard failure halts.

### Task 3: Iteration visibility (P0)

- [ ] Live UI prints an iteration header between iterations; finished steps show their
      iteration.
- [ ] `report.md` groups steps by iteration and states the stop reason + iteration count;
      `run.json` records `iterations`, `stop_reason`, `converged`.
- [ ] The summary box reads "converged in N" / "stopped after N (max-iters)" / "halted".

### Task 4: Actionable output + apply (P1)

- [ ] Compute a run-level cumulative diff (original source snapshot vs final workspace);
      write `<run>/changes.diff`; summary shows changed files + stats.
- [ ] `loope run --show-diff` prints the cumulative diff after the run.
- [ ] `loope apply <run-id> [--workdir DIR]`: copy changed/added files from the run
      workspace into the working directory; list what was applied; never delete.
- [ ] Tests: cumulative diff over a copied tree; `apply` lands a dry-run change.

### Task 5: Contract-aware gating (P2)

- [ ] Include the Design Contract in the verifier prompt; instruct reviewers to `BLOCK`
      on unmet acceptance criteria.
- [ ] Unit/integration: a design run feeds the contract to the verifier prompt.

### Task 6: Verify + docs + real run

- [ ] `cargo test` green, no binaries/network; `cargo clippy` clean; no new crates.
- [ ] Real run that needs two iterations; `loope apply` lands the result.
- [ ] Update `docs/guide/usage.md` (iterations, `--max-iters`, `--show-diff`, `apply`,
      stop reasons) and the README. Commit once.
