#!/usr/bin/env bash
#
# Hermetic harness benchmark — measures Loope's orchestration with ZERO model cost
# (stub agents via --dry-run). No network. Deterministic. CI-friendly.
#
# Asserts: determinism, success-gating, failure-gating, artifact fidelity.
# Reports: harness wall-clock overhead.
# Exits non-zero if any assertion fails.

set -u
command -v loope >/dev/null || { echo "loope not on PATH (build/install it first)"; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
PASS=0; FAIL=0
ok(){ echo "  PASS  $1"; PASS=$((PASS+1)); }
no(){ echo "  FAIL  $1"; FAIL=$((FAIL+1)); }

seed(){ # dir
  rm -rf "$1"; mkdir -p "$1/src"
  printf '[package]\nname="t"\nversion="0.1.0"\nedition="2021"\n[workspace]\n' > "$1/Cargo.toml"
  printf 'pub fn add(a:i64,b:i64)->i64{a+b}\n#[test]\nfn t(){assert_eq!(add(2,2),4);}\n' > "$1/src/lib.rs"
}
rjson(){ find "$1/.loope/runs" -name run.json 2>/dev/null | head -1; }
field(){ python3 -c '
import sys,re
m=re.search(r"\"%s\":(\"?)([a-z_0-9]+)\1"%sys.argv[2], open(sys.argv[1]).read())
print(m.group(2) if m else "")' "$1" "$2"; }
now(){ python3 -c 'import time;print(time.time())'; }

echo "Loope hermetic harness benchmark"
echo

# --- determinism + latency (6 runs) ---
echo "[determinism + latency]"
SUM=0; FIRST=""
for i in 1 2 3 4 5 6; do
  seed "r$i"
  t0=$(now); ( cd "r$i" && loope run --dry-run --quiet "add a multiply function" >/dev/null 2>&1 ); t1=$(now)
  ms=$(python3 -c "print(int(($t1-$t0)*1000))"); SUM=$((SUM+ms))
  norm=$(sed -E 's/[0-9]{4}-[a-z-]+/RUNID/g' "$(rjson "r$i")")
  [ -z "$FIRST" ] && FIRST="$norm"
  [ "$norm" = "$FIRST" ] || no "run $i diverged from run 1"
done
[ "$FAIL" -eq 0 ] && ok "run.json identical across 6 runs (deterministic)"
echo "  INFO  mean harness overhead: $((SUM/6)) ms/run (model cost excluded)"

# --- success gating ---
echo "[gating: success]"
seed s; ( cd s && loope run --dry-run --quiet --verify-cmd "cargo test" "add a multiply function" >/dev/null 2>&1 )
[ "$(field "$(rjson s)" converged)" = "true" ] && [ "$(field "$(rjson s)" iterations)" = "1" ] \
  && ok "converges in 1 iteration when review + verify pass" \
  || no "expected converged=true iterations=1"

# --- failure gating (no false convergence) ---
echo "[gating: failure]"
seed f; ( cd f && loope run --dry-run --quiet --verify-cmd "false" --max-iters 3 "add a multiply function" >/dev/null 2>&1 )
sr="$(field "$(rjson f)" stop_reason)"
{ [ "$(field "$(rjson f)" converged)" = "false" ] && { [ "$sr" = "max_iters" ] || [ "$sr" = "stalled" ]; }; } \
  && ok "does not false-converge (stop_reason=$sr) when verify keeps failing" \
  || no "expected converged=false + max_iters/stalled, got '$sr'"

# --- artifact fidelity ---
echo "[artifacts]"
adir="$(dirname "$(rjson s)")/agents"
miss=""
for f in prompt.md events.jsonl transcript.jsonl result.md; do
  find "$adir" -name "$f" | grep -q . || miss="$miss $f"
done
[ -z "$miss" ] && ok "each step persists prompt/events/transcript/result" || no "missing artifacts:$miss"

echo
echo "Result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
