# Loope Robustness Implementation Plan (v0.9)

> Implement task-by-task with tests first. Keep the suite hermetic. Commit once at the
> end.

**Spec:** [Robustness Spec (v0.9)](../specs/2026-06-28-loope-robustness-spec.md)

**Goal:** Time-bound every agent subprocess, and give every step its own artifact
directory so the implement and revise turns no longer overwrite each other.

**Tech Stack:** Rust 2024, standard library only. No new crates.

---

## Tasks

### Task 1: Per-step artifact directories

- [ ] `RunWorkspace::agent_dir(step_id, role, adapter)` and
      `agent_home(step_id, role, adapter)` → `agents/<NN>-<role>-<adapter>/` (zero-padded
      id, sanitized role/adapter).
- [ ] Update every caller in the executor (single-step path, reviewer-group prompt loop,
      `run_reviewer`) to pass the step id.
- [ ] Update unit tests and integration tests for the numbered directory names.
- [ ] Add an integration assertion that the two implementer turns keep separate
      `result.md` files.

### Task 2: Subprocess timeout

- [ ] Add `timeout: Duration` to `SubprocessInvoker` (0 / `None` semantics = disabled).
- [ ] Reread stdout on a dedicated thread that sends lines over a channel; the main
      thread consumes with `recv_timeout`, parses + sinks events, and on deadline kills
      the child and returns a `timed out after Ns` failure.
- [ ] Keep stderr draining and the missing-binary / nonzero-exit paths intact.
- [ ] CLI: `--timeout SECS` over `LOOPE_TIMEOUT` over default 600s; `0` disables. Applies
      to `run` and `design`.
- [ ] Unit test the timeout resolution; document the kill path as manual.

### Task 3: Verify + docs

- [ ] `cargo test` green, no binaries/network; `cargo clippy` clean; no new crates.
- [ ] Manual: a sleeping stub binary is killed at `--timeout`; the step fails and the
      loop halts.
- [ ] README: numbered run-directory layout, `--timeout` / `LOOPE_TIMEOUT`, and the
      manual timeout-check note. Commit once.
