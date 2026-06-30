# Loope Benchmarks

Benchmarks live here (not under `scripts/`) because they are **product evidence and
evaluation tooling**, not maintenance commands. They answer one question:

> Does wrapping a coding agent in a **convergent review loop** (implement → review → verify,
> repeated with feedback) deliver more reliable software than a single shot — and at what
> cost?

To answer it credibly we separate two things that are usually tangled together:

- the **harness** — Loope's own orchestration (state, gating, recovery, evidence), and
- the **model** — the agent CLI doing the actual coding.

So the suite has two tiers: a **hermetic tier** that measures the harness with zero model
cost (deterministic, runs in CI), and a **delivery tier** that measures end-to-end outcomes
with real agents on real tasks (SWE-bench-style).

---

## Metrics

### Harness metrics (hermetic — model excluded)

| Metric | Definition | Why it matters |
| --- | --- | --- |
| **Determinism** | identical `run.json` + artifacts across repeated `--dry-run`s | the harness must be a reproducible substrate; non-determinism here would poison every delivery measurement |
| **Harness overhead** | wall-clock of a full dry-run loop (stub agents) | the orchestration must be negligible next to the model |
| **Gating correctness** | converges *iff* verify passes and no reviewer blocks; otherwise stops honestly (`max_iters` or `stalled`) | no false "converged"; honest failure |
| **Artifact fidelity** | every step persists prompt / events / transcript / diff / result, and `events.jsonl` round-trips to cells | the run is auditable evidence, not a black box |

### Delivery metrics (real agents — the outcomes)

| Metric | Definition |
| --- | --- |
| **Resolve rate** (`pass@1`, `pass@k`) | task solved iff its `--verify-cmd` passes on the final workspace (the SWE-bench oracle) |
| **Iterations-to-converge** | implement→review→verify cycles to pass (median, p90) |
| **Convergence-within-budget** | % of tasks that converge within `--max-iters` |
| **Review catch-and-fix rate** | % of resolved runs with a recorded `highlight` — a reviewer blocked a real defect the implementer missed *and* a later iteration fixed it. **Loope's signature metric**: it quantifies the value the review loop adds |
| **Token economy** | total tokens, **tokens-per-resolved-task**, and **wasted-token ratio** = tokens on runs that never converged ÷ total |
| **Wall-clock** | per task and per iteration |
| **Over-edit / regression** | did a later iteration break a previously-passing check (verify oscillation) |

Tokens, iterations, changed files, verdicts, and the highlight are all read straight from
the persisted `run.json` and per-step `events.jsonl` — no extra instrumentation.

---

## The core experiment (the ablation that justifies the loop)

Run the **same task set** under two configurations and compare:

- **Loop** — `loope run` with review + verify, `--max-iters 3` (e.g. `--preset claude-codex`).
- **Single-shot baseline** — `loope run --max-iters 1` (implement only, no review/repair).

**Hypotheses**

1. *Reliability:* the loop's resolve rate ≥ single-shot's; the **gap is explained by the
   catch-and-fix rate** (defects the reviewer caught and the loop repaired).
2. *Cost:* the loop spends more tokens, but **tokens-per-resolved-task** stays bounded
   (wasted tokens concentrate in the tasks that never converge — capped by `--max-iters`).
3. *Independent review beats self-review:* `claude→codex` and `dual-review` catch more than
   `claude-solo` (an agent reviewing itself). Compare catch rate + resolve rate per preset.

Each hypothesis is decided by a specific metric above, so the result is falsifiable.

---

## Task cases

A case is self-contained: `cases/<id>/` with a `case.json`
(`{ id, requirement, verify_cmd, lang, trap }`) and a `seed/` working tree. The `verify_cmd`
(e.g. `cargo test`, `pytest`) is the oracle. Cases deliberately include a **trap** — a
subtle requirement (overflow, empty input, off-by-one) that a naive first pass fails — so the
catch-and-fix metric is meaningful. See [`cases/checked-multiply/`](cases/checked-multiply/)
for the format.

Two pools:

- **Micro** — small Rust/Python tasks with a hermetic `verify_cmd` (fast, runnable anywhere).
  Bundled traps:
  - [`checked-multiply`](cases/checked-multiply/) (Rust) — i64 overflow must saturate.
  - [`empty-average`](cases/empty-average/) (Rust) — divide-by-zero on an empty slice.
  - [`inclusive-sum`](cases/inclusive-sum/) (Rust) — off-by-one exclusive range.
  - [`safe-div`](cases/safe-div/) (Python/pytest) — unguarded `ZeroDivisionError`.

  Each ships a hidden test that a plausible naive first pass fails, so the catch-and-fix
  metric is meaningful. Add a case by dropping a `cases/<id>/` with `case.json` + `seed/`.
- **SWE-bench Lite subset** — real GitHub issues whose hidden test patch is the oracle, the
  industry-standard external benchmark ([SWE-bench](https://github.com/SWE-bench/SWE-bench)).
  [`swebench/import.py`](swebench/) materializes instances (clone @ base_commit + apply the
  hidden test_patch + a `FAIL_TO_PASS`/`PASS_TO_PASS` `verify_cmd`) into `cases/swe-*/` that
  flow through `deliver.sh` unchanged. See [`swebench/README.md`](swebench/README.md).

---

## How to run

**Hermetic tier** (no network, deterministic, CI-friendly):

```bash
benchmarks/hermetic.sh        # asserts determinism + gating; reports harness latency
```

**Delivery tier** (real agents — gated on installed, authenticated CLIs):

```bash
# run each case with a real preset; re-run its verify_cmd as an independent oracle; parse
# run.json/events.jsonl into metrics; write benchmarks/results/<date>-<preset>.json
benchmarks/deliver.sh --preset claude-codex --samples 3

# the ablation baseline (one pass, no repair loop):
benchmarks/deliver.sh --preset claude-codex --single-shot --samples 3

# exercise the whole pipeline hermetically (stub agents, no CLIs needed):
benchmarks/deliver.sh --dry-run --samples 2

# compare the two snapshots into a verdict table (Δresolve, token premium, and the
# attribution: of the cases the loop newly resolved, how many via a reviewer catch-and-fix)
benchmarks/compare.py results/<date>-<preset>-singleshot.json results/<date>-<preset>.json
```

`deliver.sh` drives the run, `_metrics.py` parses and aggregates, `compare.py` turns the
loop/baseline pair into the ablation verdict. Resolution is decided by
**re-running the case's `verify_cmd` on the final workspace** (an independent oracle, not the
harness's own verdict). Results snapshots are committed under `benchmarks/results/` as dated
JSON so trends are visible over time.

---

## Conclusions

### Hermetic tier — measured

Run on the bundled micro seed (`add`/`multiply`, `cargo test` oracle), `--dry-run` (stub
agents, so the numbers are pure harness):

| Check | Result |
| --- | --- |
| Determinism | `run.json` **byte-identical** across repeated runs (modulo the run-id) |
| Harness overhead | **~22–27 ms** wall-clock per full loop (engine self-timed at **1 ms**); the orchestration is negligible — essentially 100% of a real run's time/tokens is the agent, which is the intended design |
| Gating (success) | converges in **1 iteration** when review passes and `cargo test` is green |
| Gating (failure) | with a verifier that always fails, the loop **does not false-converge**: `converged=false, stop_reason=stalled` (it detects the repeated identical failure and stops early instead of burning the budget) |
| Artifacts | each step writes `prompt.md`, `events.jsonl`, `transcript.jsonl`, `changes.diff`, `result.md` — a complete, parseable audit trail |

**Takeaway:** the harness is a reproducible, near-zero-overhead substrate with honest

These runs also motivated a follow-up optimization (verify-first review, stall-stop, structured repair feedback) — see [`docs/specs/2026-06-30-loope-loop-optimization-spec.md`](../docs/specs/2026-06-30-loope-loop-optimization-spec.md).
convergence gating and full per-step evidence — the preconditions for trustworthy delivery
measurement.

### Delivery tier — runner validated, results pending

The runner (`deliver.sh` + `_metrics.py` + `compare.py`) is built and validated end-to-end
via `--dry-run`: on the trap cases the stub agent cannot fix the bug, and the pipeline
reports it honestly — `resolve_rate=0`, `stop_reason=max_iters`, `catch_and_fix_rate=0`, no
false success — and `compare.py` renders the loop/baseline table + attribution + verdict
(here a no-difference verdict, since stubs solve nothing). Real numbers require authenticated
agent CLIs and are run outside CI; the
metrics and the ablation above are defined and ready, and snapshots publish under
`benchmarks/results/`. The
single number that will make or break Loope's thesis is the **catch-and-fix rate**: if the
review loop rarely catches a real defect, the loop is not worth its tokens; if it catches
often, the loop is the product. The harness already records exactly that signal per run.
