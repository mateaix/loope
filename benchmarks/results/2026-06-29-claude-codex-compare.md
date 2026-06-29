# Loop vs baseline — claude-codex

- baseline: `results/2026-06-29-claude-codex-singleshot.json` (mode=real, max_iters=1, samples=4)
- loop:     `results/2026-06-29-claude-codex.json` (mode=real, max_iters=3, samples=4)

| metric | baseline | loop | Δ |
| --- | --- | --- | --- |
| resolve rate | 100.0% | 100.0% | +0.0 pts |
| convergence rate | 75.0% | 100.0% | +25.0 pts |
| catch-and-fix rate | 0.0% | 0.0% | +0.0 pts |
| median iterations | 1 | 1 | +0 |
| tokens / resolved | 96486 | 88472 | -8014 |
| wasted-token ratio | 30.0% | 0.0% | -30.0 pts |
| median wall ms | 98036 | 84895 | -13141 |

## Attribution

- Loop resolved no case the baseline missed.

## Verdict

No resolve-rate difference in this run (premium 0.92×). Add samples/cases, or the tasks are too easy/hard to separate the loop from a single shot.
