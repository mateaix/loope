# Real SWE-bench result — `pallets/flask` #4045

A real run of the convergent review loop vs a single shot on an actual SWE-bench Lite
instance (the external, industry-standard benchmark), with `claude-codex` and the real
`FAIL_TO_PASS` / `PASS_TO_PASS` test oracle.

**Instance:** `pallets__flask-4045` — *"Raise error when blueprint name contains a dot."*
Materialized with `swebench/import.py` (clone @ base_commit + the hidden test_patch). The
environment needed the predicted per-instance pinning (`werkzeug==2.2.3`, `markupsafe==2.1.1`,
`pytest==7.1.2`) before flask would even import — the reason the official harness is Dockerized.

## Result

| config | resolved | iterations | catch-and-fix | tokens (in/out) | stop_reason |
| --- | --- | --- | --- | --- | --- |
| **single-shot** (`--max-iters 1`) | ❌ **no** | 1 | no | 385k / 8.9k | max_iters |
| **loop** (`--max-iters 3`) | ✅ **yes** | 2 | ✅ **yes** | 766k / 13.9k | converged |

## What happened

- **Single shot:** Claude patched `blueprints.py`, Codex's review passed it — but the
  verifier **blocked**: of the two `FAIL_TO_PASS` tests, one passed and
  `test_route_decorator_custom_endpoint_with_dots` **still failed**. The one-shot fix was
  plausible-but-incomplete, and review did not catch it. Result: **unresolved.**
- **Loop:** the same incomplete fix landed in iteration 1; the **verify gate caught** the
  remaining test failure, that feedback went back to Claude, and **iteration 2 completed the
  fix** → both `FAIL_TO_PASS` pass, `PASS_TO_PASS` stay green → **converged, resolved.** The
  run recorded a `highlight` (`catch_and_fix=true`) — the "caught & fixed" moment.

## Conclusion

On a real GitHub issue, **the review loop resolved what a single shot could not** — at ~2×
tokens and one extra iteration — and the win is **directly attributable to the verify→repair
cycle** (`catch_and_fix=true`). This is the regime the micro traps were too easy to reach
(Claude solved those single-shot 100%): the loop's value shows up exactly when the first
attempt is plausible-but-wrong and a gate catches it.

> One instance is an anecdote, not a rate — a published number needs many instances in the
> official Dockerized environment. But end-to-end, on real data, the thesis holds: **the
> harness's catch-and-fix loop turns an unresolved single shot into a resolved fix.**
