# Real SWE-bench batch — 5 instances, official Docker harness

Loope (claude-codex, loop `--max-iters 3`) on a 5-instance subset of SWE-bench Lite
(`pallets/flask` ×3, `pytest-dev/pytest`, `pylint-dev/pylint`), evaluated by the **official
`swebench` Docker harness**.

## Official verdict — 3/5 resolved (60%)

| instance | repo | official |
| --- | --- | --- |
| pallets__flask-4045 | flask | ✅ resolved |
| pallets__flask-5063 | flask | ✅ resolved |
| pylint-dev__pylint-5859 | pylint | ✅ resolved |
| pallets__flask-4992 | flask | ❌ unresolved |
| pytest-dev__pytest-7490 | pytest | ❌ unresolved |

```json
{ "total": 5, "resolved": 3, "unresolved": 2, "resolve_rate": 0.6 }
```

Evidence: the 5 model patches in
[`swebench-lite5.predictions.jsonl`](swebench-lite5.predictions.jsonl), summary in
[`swebench-lite5.report.json`](swebench-lite5.report.json). flask-4045 is the deeply-traced
catch-and-fix win ([writeup](2026-06-29-swebench-flask-4045.md)).

## Reading it honestly

- **Officially Docker-verified**, real GitHub issues, gold-test oracle — not a local-env
  artifact. **60% on 5 instances** lands in the ballpark of strong single coding agents on
  SWE-bench Lite, but **5 is a small sample**: the confidence interval is wide and this is
  not a published rate. It needs many more instances (the resumable [`batch.py`](../swebench/batch.py)
  exists for exactly that).
- The subset is **light repos** (flask/pytest/pylint) where loope's *local* `verify_cmd` can
  run, so the catch-and-fix loop actually functions. Heavy repos (django/sympy/sklearn) need
  loope to verify inside the instance's own Docker env — a future enhancement.
- Two new data points beyond flask: **pylint-5859 resolved** (the loop fixed a real pylint
  issue), **pytest-7490 unresolved** (a plausible fix the hidden tests rejected — the honest
  other side).

## Notes from the run

- The harness aborted its final summary on a transient TLS error during an image build, but
  all five per-instance `report.json`s were already written — this summary is aggregated from
  those (no evaluation was lost).
- Each loope loop took ~12–13 min (real Claude+Codex); background runs are capped at ~25–30
  min here, so the sample was grown across resume cycles — `batch.py` appends each prediction
  as it lands, so a kill never loses completed work.
