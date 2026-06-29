# SWE-bench → Loope

Run the external, industry-standard benchmark
[SWE-bench](https://github.com/SWE-bench/SWE-bench) (Lite) through Loope's delivery runner by
importing its instances into the case format.

## What it does

`import.py` turns each SWE-bench instance into `../cases/swe-<id>/`:

- clones the instance's `repo` at its `base_commit` into `seed/`,
- applies the instance's **test_patch** (the hidden tests) onto the seed,
- writes `case.json` whose `verify_cmd` runs the `FAIL_TO_PASS` + `PASS_TO_PASS` tests,
- and strips the cloned `.git` so no upstream history is nested in this repo.

The **gold patch is never written** — only the bug and the hidden tests. An instance is
**resolved** iff, on the final workspace, all `FAIL_TO_PASS` pass and all `PASS_TO_PASS` stay
green — exactly what [`../deliver.sh`](../deliver.sh) checks when it re-runs `verify_cmd` as
an independent oracle. So the same loop / single-shot ablation and the catch-and-fix metric
apply unchanged to real GitHub issues.

## Use

```bash
# 1. get the dataset (e.g. from Hugging Face princeton-nlp/SWE-bench_Lite) as JSONL —
#    one instance per line with: instance_id, repo, base_commit, problem_statement,
#    test_patch, FAIL_TO_PASS, PASS_TO_PASS
python3 import.py --instances lite.jsonl --limit 20      # → ../cases/swe-*/

# 2. run them through the delivery tier like any other case
../deliver.sh --preset claude-codex --cases "$(ls -d ../cases/swe-* | xargs -n1 basename | tr '\n' ' ')"
../deliver.sh --preset claude-codex --single-shot   # ablation baseline
```

## Official Docker evaluation (the source-of-record verdict)

For publishable, reproducible numbers, evaluate Loope's fixes with the **official `swebench`
harness in Docker** rather than a local pinned env. The flow:

1. run loope on a clone @ base_commit (with the test_patch applied so loope can verify),
2. extract the **source-only** diff vs base_commit as `model_patch` (the harness applies the
   gold test_patch itself, so exclude test files),
3. write `predictions.jsonl` lines `{instance_id, model_name_or_path, model_patch}`,
4. evaluate: [`eval.sh predictions.jsonl <run_id>`](eval.sh) → builds the per-instance image,
   applies the patch + gold tests, and reports `resolved_ids`.

**Confirmed result:** loope's loop-produced fix for `pallets__flask-4045` was evaluated by
the official harness and **officially RESOLVED** (`resolved_instances: 1`). See
[`../results/2026-06-29-swebench-flask-4045.md`](../results/2026-06-29-swebench-flask-4045.md)
and the patch [`../results/swebench-flask-4045.patch`](../results/swebench-flask-4045.patch).

## Caveat — environment is the source of record

Faithful evaluation needs each repo's **environment** (its Python dependencies, sometimes a
specific interpreter). The official **Dockerized SWE-bench harness** builds a per-instance
image and is the source of record for published numbers. This importer is for **local
experimentation** on instances whose dependencies you can install (run `pip install -e .` /
the repo's test deps in the seed before benchmarking, or wrap `verify_cmd` accordingly).

## Self-test

`selftest.sh` proves the importer's mechanics with a synthetic instance backed by a local
git repo — clone @ base_commit, apply the hidden test_patch, emit a case + `verify_cmd`, and
confirm the oracle fails on the still-buggy seed. No network, no real SWE-bench repo:

```bash
bash selftest.sh        # → selftest: OK
```
