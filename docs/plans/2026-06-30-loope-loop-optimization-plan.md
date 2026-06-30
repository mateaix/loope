# Loop Optimization Plan

Implementation plan for [[2026-06-30-loope-loop-optimization-spec]]. All changes are in the
std-only engine (`src/engine/executor.rs`), driven by the benchmark evidence. Sequenced so
each task is independently verifiable; the `loope` crate stays `deps = 1`.

## T1 — Structured repair feedback (lowest risk, immediate value)

- Add `summarize_failures(output: &str) -> Option<String>` (pure): extract recognized
  failures from pytest / cargo output into a compact, capped bullet list.
- Have `compose_feedback` lead with the parsed "failing checks" block when present, then the
  existing bounded tail + reviewer blockers; fall back to today's behavior otherwise.
- Tests: pytest + cargo fixtures → expected summaries; unknown output → `None`.
- **Verify:** core green; no behavior change when output is unrecognized.

## T2 — Stall detection + `StopReason::Stalled`

- Add `StopReason::Stalled` (label "stopped: no progress", id "stalled"); `all_passed()`
  stays false for it; CLI/TUI/report render it.
- Track the previous iteration's progress key (implementer "no change made", or the
  normalized `verify_failure`). After a non-converging iteration whose key shows no progress
  vs. the prior one, set `StopReason::Stalled` and break.
- Tests: a no-change / identical-failure stub stalls before `--max-iters`; a progressing
  stub still converges or hits max.
- **Verify:** core + TUI green (handle the new variant in any exhaustive matches).

## T3 — Verify-first, review-on-pass

- Restructure the iteration body so that, when `config.verify_command.is_some()`:
  verify runs right after implement; reviewers run only when verify passed. When it is
  `None`, keep implement → review (unchanged).
- Keep convergence `verify_pass && !has_blockers`; ensure `has_blockers` is only set from
  reviewers that actually ran. Preserve the `caught & fixed` highlight detection.
- Tests: failing verify ⇒ no reviewer outcome that iteration; passing verify ⇒ reviewer
  runs; no-verify path byte-for-byte unchanged; highlight still fires on the
  block-then-fix sequence.
- **Verify:** core + TUI green, both feature configs; `deps = 1`.

## T4 — Verify, docs, real check

- Full verification: `cargo test` / `cargo clippy` with and without `--features tui`;
  `cargo tree` deps = 1; no forbidden names.
- Update `docs/guide/usage.md` (new `Stalled` stop reason + the conditional review note) and
  the README; a short note in `benchmarks/README.md` linking the optimization to the
  evidence that motivated it.
- Optional real re-run of one previously-unresolved instance (e.g. flask-4992) to see
  whether sharper feedback + stall-stop changes the outcome or the token spend; record it.

## Related

- Spec: [[2026-06-30-loope-loop-optimization-spec]]
- [[2026-06-28-loope-iterative-loop-spec]] · [[2026-06-28-loope-review-orchestration-spec]] ·
  [[2026-06-29-loope-convergence-highlight-spec]]
