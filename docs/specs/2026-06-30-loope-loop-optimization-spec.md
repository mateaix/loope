# Loop Optimization Spec — verify-driven repair, stall-stop, adaptive review

## Background — what the benchmarks showed

Running loope against real tasks (the micro traps and a SWE-bench Lite subset, evaluated by
the official Docker harness) produced evidence that points at concrete algorithm
improvements to the loop:

1. **The verifier is the real bug-catcher; the reviewer often rubber-stamps.** On
   `flask-4045` the single shot produced a *plausible-but-incomplete* fix — Codex's review
   **passed it**, and only the **verify gate** (tests) caught the gap. `flask-4992` was
   resolved by neither: review passed a wrong fix that the tests rejected. So review, run
   every iteration, frequently fails to catch what the tests catch — yet it costs a full
   agent round-trip each time.

2. **Wasted iterations on no-progress.** `flask-4992` and `pytest-7490` burned the full
   3-iteration budget without resolving — the loop kept attempting similar fixes after the
   tests kept failing the same way, with no mechanism to notice "we're stuck."

3. **Repair quality tracks feedback quality.** The difference between a resolved and an
   unresolved instance was largely whether the next attempt got an *actionable* signal. The
   loop already feeds back the verifier's output tail, but a raw tail buries the few lines
   that matter (which tests failed, the assertion) in noise.

4. **On easy tasks the loop is pure overhead.** The micro traps were solved single-shot
   100%; the loop converged at iteration 1 but, with review-every-iteration, still spent a
   reviewer call it did not need.

These are not model problems — they are *harness* problems, exactly the layer loope owns.

## Goals

Three evidence-driven changes to `execute_loop`, all in the std-only engine:

1. **Verify-first, review-on-pass** — when a verify command is configured, run *verify
   before review*, and **only run reviewers when verify passes**. A failing verify is the
   actionable signal; skip the rubber-stamp round-trip and go straight to repair. Review is
   spent where it adds value — vetting a change the tests already accept, to catch what
   tests miss (design, edge cases). With no verify command, behavior is unchanged (review is
   the only gate).

2. **Stall detection + early stop** — detect when an iteration makes *no progress* and stop
   with a new `StopReason::Stalled` instead of burning the remaining budget. No-progress =
   the implementer produced no change, **or** the verify failure is identical to the prior
   iteration's. Saves tokens and ends honestly ("stuck", not a silent max-iters).

3. **Structured repair feedback** — parse the verifier output into a short, targeted
   "failing checks" block (the failed test ids + their assertion/error lines) placed at the
   top of the repair prompt, falling back to the bounded tail when no failures are
   recognized. Sharper signal → better repair.

All three preserve correctness, are unit-testable through the stub invoker, keep the `loope`
crate `deps = 1`, and leave the no-verify and design-contract paths working.

## Non-Goals (v1)

- Changing the convergence definition's meaning (`verify_pass && review_no_block`) — only
  *when* review runs.
- Verifying heavy repos inside their own container/sandbox (a separate infra enhancement).
- Switching models / multi-strategy escalation on stall (this version *stops* on stall; a
  later one may *escalate* — switch implementer, add a diagnostic reviewer).
- Token-budget accounting as a stop condition (stall-stop is the v1 lever).

## Design

### O1 — Verify-first, review-on-pass

Per iteration, when `config.verify_command` is set:

```
implement → verify
  ├─ verify fails → (skip review) → repair feedback → next iteration
  └─ verify passes → review(s)
        ├─ a reviewer blocks → feedback → next iteration
        └─ no blocker → converged
```

When `config.verify_command` is `None`, the order is unchanged (implement → review →
converge), since review is then the only gate.

Convergence is still `verify_pass && !has_blockers`; the change is that reviewers don't run
(and can't block) on an iteration whose verify already failed. The pipeline UI keeps showing
`implement → review → verify`; only the *execution order and gating* change.

### O2 — Stall detection

Track a per-iteration **progress key**:

- the implementer's change set was empty (`gate_notes == "no change made"`), or
- the current `verify_failure` equals the previous iteration's `verify_failure` (same
  failure, byte-for-byte after normalization).

If an iteration is non-converging **and** its progress key shows no progress versus the prior
iteration, stop with `StopReason::Stalled` (label: "stopped: no progress"). A single
no-progress iteration is enough to stop — repeating an identical failure is the signal.

### O3 — Structured repair feedback

Add `summarize_failures(output: &str) -> Option<String>` that extracts recognizable failures
from common runners:

- pytest: `FAILED <id>`, `E   assert …`, `E   <Error>: …`
- cargo test: `test <name> ... FAILED`, `panicked at …`, `assertion `…`

It returns a compact bullet list (capped). `compose_feedback` puts this **first** ("These
checks are failing — fix them:"), then the existing bounded tail and any reviewer blockers
as context. When nothing is recognized, it degrades to today's tail (no regression).

## Acceptance Criteria

- With a verify command, a failing-verify iteration runs **no reviewer step**; a passing
  verify is followed by review; convergence and the `caught & fixed` highlight still work.
- Without a verify command, the loop is byte-for-byte unchanged.
- A run that keeps failing the same check **stops with `StopReason::Stalled`** before
  `--max-iters`, persisted in `run.json` and shown in the CLI/TUI/report.
- The repair prompt for a failing run leads with a parsed "failing checks" block on
  recognized runner output; falls back to the tail otherwise.
- `cargo test` and `cargo clippy` green with and without `--features tui`; `deps = 1`;
  existing executor/TUI tests pass (updated where order is asserted).

## Testing Strategy

- **Step order** (stub invoker): a configured-but-failing verify yields outcomes with **no
  reviewer** that iteration; a passing verify yields a reviewer outcome; assert the
  `StopReason` and step sequence.
- **Stall** (stub): an implementer stub that makes no change (or a verify that fails
  identically) → `StopReason::Stalled` at iteration < max; a progressing stub still reaches
  `Converged`/`MaxIters`.
- **Feedback parser** (pure): pytest and cargo fixtures → expected bullet summaries; unknown
  output → `None` (tail fallback).
- **No-verify regression**: the existing convergence/feedback tests pass unchanged.

## Related

- [[2026-06-28-loope-iterative-loop-spec]] — the loop this optimizes.
- [[2026-06-28-loope-review-orchestration-spec]] — reviewers, now run conditionally.
- [[2026-06-29-loope-convergence-highlight-spec]] — the catch-and-fix highlight, preserved.
- Evidence: `benchmarks/results/2026-06-30-swebench-lite5.md` and the flask-4045 writeup.
