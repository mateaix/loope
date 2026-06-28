# Loope Iterative Loop Spec (v1.0)

## Background

Loope calls itself "Loop Engineering", but it does not loop: a run executes
implementer → reviewer → implementer → verifier **once each** and stops. There is no
convergence — if the verifier's tests fail or the reviewer raises blockers, the loop
does not iterate to fix them; it just halts. That is a fixed four-step pipeline, not a
loop, which is why it feels toy-like and why there is no "iteration count" to show.

Three changes make it a real tool:

- **P0 — a real iterative loop.** Repeat *implement/fix → review → verify* until the
  tests pass and the review has no blockers, or a max-iteration cap is reached, feeding
  each iteration's failures back into the next fix.
- **P1 — actionable output.** Surface the cumulative change inline and let the user
  **apply** a run's changes to the real repository, instead of stranding them in a
  sandbox copy.
- **P2 — contract-aware gating.** When a Design Contract exists, "done" means the
  contract's acceptance criteria are met, not merely that a command exited zero.

## Goals

1. A run **iterates** implement/fix → review → verify and **converges**: it stops when
   verification passes and no reviewer reports blockers, or after `--max-iters` (default
   3). Each iteration's failures (failing test output, review blockers) are fed into the
   next fix.
2. The number of iterations and the **stop reason** (converged / max-iters reached /
   step failed) are visible live, in the report, and in `run.json`.
3. The run surfaces the **cumulative diff** of what it changed, and `loope apply
   <run-id>` lands those changes in the real working directory.
4. With `--design`, the contract is given to the verifier and the reviewer is told to
   block if the acceptance criteria are not met.
5. No regressions: dry-run stays hermetic; plain/CI output stays plain; tests green;
   `cargo clippy` clean; no new crates.

## Non-Goals (v1.0)

- Branch/PR automation in `apply` (a simple copy-back to the working tree; `git` is the
  user's to run). A `--git` mode is future work.
- Per-tool approval, or cancelling mid-iteration.
- Machine-checking acceptance criteria beyond feeding them to the agents as gate prompts.

## Core Concepts

### Iterations

A run is: an optional **design** step (once), then a sequence of **iterations**. Each
iteration is:

```text
implement/fix  →  review (1..N reviewers)  →  verify
```

- Iteration 1's implementer does the initial implementation.
- Iteration *k>1*'s implementer is a **fix** turn: its prompt carries the previous
  iteration's review blockers and the verifier's failure output, and is asked to resolve
  them.
- After each iteration Loope evaluates **convergence**.

### Convergence and stop reasons

After an iteration:

- `verify_pass` is true if the verify step succeeded (a `--verify-cmd` exits zero; with
  no command, verification is informational and treated as passing).
- `has_blockers` is true if any reviewer's verdict is `BLOCK`.
- **Converged** when `verify_pass && !has_blockers` → stop, success.
- Otherwise, if iterations `< --max-iters`, loop again with feedback.
- Otherwise stop with **max-iters reached** → the run did not converge (failure).

A **hard step failure** (an agent invocation that fails or times out) stops the loop
immediately with **step failed** — retrying a crashed agent is not useful.

`run.json` records `iterations`, `stop_reason`, and `converged`. The report and the live
UI show iteration boundaries and the final "converged in N iterations" / "stopped after
N (max-iters)" / "halted: <role> failed".

### Feedback

Between iterations Loope composes a feedback block from:

- each `BLOCK` reviewer's message ("address these blocking findings: …"), and
- the verifier's output when it failed ("the verifier failed: …").

That block is appended to the next iteration's implementer prompt.

### Per-step records

Step ids continue to span the whole run (`agents/<NN>-<role>-<adapter>/`), so every
iteration's implement, review, and verify steps keep separate artifacts. Each
`StepOutcome` carries its `iteration` number.

### Actionable output (P1)

- A run-level **cumulative diff** (original source vs final workspace) is computed at the
  end and written to `<run>/changes.diff`; the summary shows the changed files with
  `+/−` stats. `loope run --show-diff` (and `loope show --diff`) print it.
- `loope apply <run-id> [--workdir DIR]` copies the run's changed/added files from its
  workspace back into the working directory (default: the current directory), so a
  converged run can be landed. It lists what it applied; it never deletes.

### Contract-aware gating (P2)

When a Design Contract is present, it is included in the **verifier** prompt as well as
the implementer/reviewer prompts, and the reviewer instruction becomes "return `BLOCK`
if the change does not meet the contract's acceptance criteria." Convergence therefore
requires the reviewers to judge the contract satisfied.

## CLI Surface (additions)

```bash
loope run --max-iters 5 --verify-cmd "cargo test" "..."   # iterate up to 5 times
loope run --show-diff --verify-cmd "cargo test" "..."     # print the cumulative diff
loope apply run-0001                                       # land the run's changes
loope apply run-0001 --workdir ./app                       # land into a specific tree
```

`--max-iters N` (default 3; `1` reproduces the old single-pass behavior). `--show-diff`
prints the cumulative diff after the run.

## Acceptance Criteria

- A run repeats implement/fix → review → verify and stops on convergence or
  `--max-iters`; the iteration count and stop reason appear live, in `report.md`, and in
  `run.json`. `--max-iters 1` is a single pass.
- When verification fails or a reviewer blocks, the next iteration's implementer prompt
  contains that failure/blocker text.
- A hard agent failure/timeout halts immediately with a "step failed" stop reason.
- `loope run --show-diff` and `loope show --diff` print the cumulative diff; `loope apply
  <run-id>` copies the changed files into the working directory and reports them.
- With `--design`, the verifier prompt includes the contract and the reviewer is asked to
  block on unmet acceptance criteria.
- Dry-run stays hermetic (the stub converges in one iteration); the suite stays green
  with no binaries or network; `cargo clippy` clean; no new crates.

## Testing Strategy

- Unit: convergence decision (verify_pass × has_blockers × iter vs max); feedback
  composition; the iteration loop via a stub that blocks on iteration 1 and passes on
  iteration 2 (asserting two iterations and the fed-back text); a hard-failure halt.
- Unit: cumulative diff over a copied tree; `apply` copies changed files and reports
  them.
- Integration: `run --dry-run` converges in one iteration and writes `iterations`/`stop
  reason`; `run --dry-run --max-iters 1` is a single pass; `apply` lands a dry-run's
  change into a target dir.
- Manual: a real run that needs two iterations (e.g. a failing test the implementer must
  fix), and `loope apply`.

## Related

- [[2026-06-28-loope-mvp-spec]] — the loop concept this finally realizes.
- [[2026-06-28-loope-robustness-spec]] — per-step dirs/timeout the iterations rely on.
