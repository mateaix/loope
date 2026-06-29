#!/usr/bin/env bash
#
# Delivery-tier benchmark runner. For each case it runs Loope with a real preset, then
# re-runs the case's verify_cmd on the final workspace as an independent oracle, parses
# run.json / events.jsonl into metrics, and writes a dated results snapshot.
#
#   benchmarks/deliver.sh [--preset NAME] [--samples K] [--max-iters N]
#                         [--single-shot] [--cases "id1 id2"] [--dry-run]
#
# --single-shot  forces --max-iters 1 (one pass, no repair) — the ablation baseline.
# --dry-run      uses stub agents (no real CLIs) — exercises the whole pipeline hermetically.
#
# Real runs need the agent CLIs installed and authenticated.

set -u
DIR="$(cd "$(dirname "$0")" && pwd)"; cd "$DIR"
command -v loope >/dev/null || { echo "loope not on PATH"; exit 2; }

preset="claude-codex"; samples=1; iters=3; dry=""; only=""; label=""
while [ $# -gt 0 ]; do
  case "$1" in
    --preset) preset="$2"; shift 2;;
    --samples) samples="$2"; shift 2;;
    --max-iters) iters="$2"; shift 2;;
    --single-shot) iters=1; label="-singleshot"; shift;;
    --cases) only="$2"; shift 2;;
    --dry-run) dry="--dry-run"; shift;;
    *) echo "unknown arg: $1"; exit 2;;
  esac
done

stamp="$(date +%Y-%m-%d)"
mode="real"; [ -n "$dry" ] && mode="dry-run"
mkdir -p results
records="$(mktemp)"; trap 'rm -f "$records"' EXIT

jget(){ python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
now(){ python3 -c 'import time;print(time.time())'; }

echo "delivery benchmark · preset=$preset iters=$iters samples=$samples mode=$mode"
for cdir in cases/*/; do
  [ -f "$cdir/case.json" ] || continue
  id="$(basename "$cdir")"
  if [ -n "$only" ] && ! echo " $only " | grep -q " $id "; then continue; fi
  requirement="$(jget "$cdir/case.json" requirement)"
  verify="$(jget "$cdir/case.json" verify_cmd)"
  for s in $(seq 1 "$samples"); do
    w="$(mktemp -d)"; cp -R "$cdir/seed/." "$w/"
    t0="$(now)"
    ( cd "$w" && loope run $dry --quiet --in-place --preset "$preset" \
        --verify-cmd "$verify" --max-iters "$iters" "$requirement" >/dev/null 2>&1 )
    t1="$(now)"
    wall="$(python3 -c "print(int(($t1-$t0)*1000))")"
    # independent oracle: does the verify_cmd pass on the final workspace?
    resolved=0; ( cd "$w" && eval "$verify" >/dev/null 2>&1 ) && resolved=1
    rdir="$(dirname "$(find "$w/.loope/runs" -name run.json 2>/dev/null | head -1)")"
    if [ -n "$rdir" ] && [ -f "$rdir/run.json" ]; then
      python3 _metrics.py record "$rdir" "$resolved" "$wall" "$id" >> "$records"
      echo "  $id #$s  resolved=$resolved  $(grep -o '"stop_reason":"[a-z_]*"' "$rdir/run.json")"
    else
      echo "  $id #$s  (no run.json — run failed?)"
    fi
    rm -rf "$w"
  done
done

meta="$(python3 -c 'import json,sys;print(json.dumps({"preset":sys.argv[1],"max_iters":int(sys.argv[2]),"date":sys.argv[3],"mode":sys.argv[4]}))' "$preset" "$iters" "$stamp" "$mode")"
out="results/${stamp}-${preset}${label}$([ -n "$dry" ] && echo -dryrun).json"
python3 _metrics.py aggregate "$records" "$meta" > "$out"
echo "wrote $out"
python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));print("  resolve_rate=%s  catch_and_fix_rate=%s  median_iters=%s  tokens_per_resolved=%s  wasted_token_ratio=%s"%(d["resolve_rate"],d["catch_and_fix_rate"],d["median_iterations"],d["tokens_per_resolved"],d["wasted_token_ratio"]))' "$out"
