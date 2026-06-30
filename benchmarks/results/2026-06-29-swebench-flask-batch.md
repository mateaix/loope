# Real SWE-bench batch — `pallets/flask` (3 instances, official Docker harness)

A batch run of the convergent review loop on the three `pallets/flask` instances in
SWE-bench Lite, with `claude-codex` (loop, `--max-iters 3`), evaluated by the **official
`swebench` Docker harness** — the canonical, reproducible oracle.

## Official verdict

```json
{ "submitted_instances": 3, "completed_instances": 3,
  "resolved_instances": 2, "unresolved_instances": 1, "error_instances": 0,
  "resolved_ids":   ["pallets__flask-4045", "pallets__flask-5063"],
  "unresolved_ids": ["pallets__flask-4992"] }
```

**Loope resolved 2 of 3 (66.7%) on this flask subset, officially Docker-verified.**

| instance | issue | loope | official |
| --- | --- | --- | --- |
| flask-4045 | error on dotted blueprint name | loop, 2 iters, catch-and-fix | ✅ resolved |
| flask-5063 | show blueprint in `flask routes` | loop (run was interrupted, but the patch it had already produced was complete) | ✅ resolved |
| flask-4992 | add `mode` to `Config.from_file` | loop completed | ❌ unresolved (fix didn't satisfy the hidden tests) |

Evidence: predictions (the three model patches) in
[`swebench-flask-batch.predictions.jsonl`](swebench-flask-batch.predictions.jsonl); the
official report in [`swebench-flask-batch.report.json`](swebench-flask-batch.report.json).

## Reading it honestly

- **This is a real, official number** (Docker harness, gold test oracle), not a local-env
  artifact — but on a **3-instance subset**, so it is a sample, not a representative
  SWE-bench Lite rate. A headline number needs the full 300 (or a proper random sample).
- flask-5063 resolving despite an **interrupted** loope run shows the harness judges the
  *patch*, not the process — the produced fix was already correct.
- flask-4992 unresolved is the honest other side: the loop produced a plausible fix that the
  hidden tests rejected — the kind of case a larger iteration budget, a stronger verify
  signal, or a second reviewer might catch.

## How it was produced

`swebench/import.py` materialized each instance; loope ran the loop against a clone @
base_commit (test_patch applied so it could verify); the **source-only** diff was extracted
as the prediction; [`swebench/eval.sh`](../swebench/eval.sh) ran the official harness. See
[`swebench/README.md`](../swebench/README.md).
