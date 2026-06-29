#!/usr/bin/env bash
#
# Run the OFFICIAL SWE-bench harness (Dockerized) on Loope-produced predictions — the
# canonical, reproducible oracle for publishable numbers.
#
#   eval.sh <predictions.jsonl> <run_id> [max_workers]
#
# predictions.jsonl: one JSON per line — {instance_id, model_name_or_path, model_patch},
# where model_patch is Loope's SOURCE diff vs base_commit (exclude test files; the harness
# applies the gold test_patch itself). Build one by running loope on a clone @ base_commit
# (with the instance's test_patch applied so loope can verify), then diffing the non-test
# files — see swebench/README.md.
#
# Requires: a running Docker daemon and `pip install swebench`.

set -eu
PRED="${1:?usage: eval.sh <predictions.jsonl> <run_id> [max_workers]}"
RID="${2:?run_id}"
WORKERS="${3:-1}"

command -v docker >/dev/null || { echo "docker not found"; exit 2; }
docker ps >/dev/null 2>&1 || { echo "docker daemon not running"; exit 2; }

python -m swebench.harness.run_evaluation \
  --dataset_name princeton-nlp/SWE-bench_Lite \
  --predictions_path "$PRED" \
  --run_id "$RID" \
  --max_workers "$WORKERS"

echo "--- report ---"
cat ./*."$RID".json 2>/dev/null || echo "(report json not found in CWD)"
