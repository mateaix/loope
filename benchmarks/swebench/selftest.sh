#!/usr/bin/env bash
#
# Self-test for import.py using a SYNTHETIC SWE-bench instance backed by a local git repo.
# Proves the importer's mechanics (clone @ base_commit, apply the hidden test_patch, emit a
# loope case + verify_cmd) without needing a real SWE-bench repo or its environment.

set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

# --- build a tiny repo: a buggy module + one already-passing test (PASS_TO_PASS) ---
REPO="$TMP/repo"; mkdir -p "$REPO"; cd "$REPO"
git init -q; git config user.email a@b.c; git config user.name t
printf 'def safe_div(a, b):\n    return a / b\n' > safe_div.py
printf 'from safe_div import safe_div\n\n\ndef test_basic():\n    assert safe_div(10, 2) == 5\n' > test_basic.py
git add -A; git commit -qm base
BASE="$(git rev-parse HEAD)"

# --- the hidden test (FAIL_TO_PASS) as a test_patch, then revert so base lacks it ---
printf 'from safe_div import safe_div\n\n\ndef test_zero():\n    assert safe_div(10, 0) == 0\n' > test_zero.py
git add -N test_zero.py
git diff > "$TMP/test_patch.diff"
rm -f test_zero.py

# --- craft the instance JSON ---
python3 - "$TMP/test_patch.diff" "$BASE" "$REPO" "$TMP/inst.json" <<'PY'
import json, sys
patch, base, repo, out = sys.argv[1:5]
inst = {
    "instance_id": "demo__safe-div-1",
    "repo": "file://" + repo,
    "base_commit": base,
    "problem_statement": "safe_div must return 0 when the divisor is 0 instead of raising.",
    "test_patch": open(patch).read(),
    "FAIL_TO_PASS": ["test_zero.py::test_zero"],
    "PASS_TO_PASS": ["test_basic.py::test_basic"],
}
json.dump(inst, open(out, "w"))
PY

# --- import it ---
OUT="$TMP/cases"
python3 "$HERE/import.py" --instance "$TMP/inst.json" --out "$OUT"

CASE="$OUT/swe-demo__safe-div-1"
echo "--- assertions ---"
fail=0
[ -f "$CASE/case.json" ] && echo "  PASS case.json written" || { echo "  FAIL no case.json"; fail=1; }
[ -f "$CASE/seed/test_zero.py" ] && echo "  PASS hidden test applied (test_zero.py present)" || { echo "  FAIL test_patch not applied"; fail=1; }
[ ! -d "$CASE/seed/.git" ] && echo "  PASS cloned .git stripped" || { echo "  FAIL .git left behind"; fail=1; }
VC="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["verify_cmd"])' "$CASE/case.json")"
echo "$VC" | grep -q "test_zero::test_zero\|test_zero.py::test_zero" && echo "  PASS verify_cmd targets FAIL_TO_PASS" || { echo "  FAIL verify_cmd missing FAIL_TO_PASS"; fail=1; }
echo "$VC" | grep -q "test_basic.py::test_basic" && echo "  PASS verify_cmd keeps PASS_TO_PASS" || { echo "  FAIL verify_cmd missing PASS_TO_PASS"; fail=1; }

# --- the oracle must currently FAIL on the buggy seed (a real trap) ---
if ( cd "$CASE/seed" && eval "$VC" >/dev/null 2>&1 ); then
  echo "  FAIL oracle passes on the buggy seed (not a trap)"; fail=1
else
  echo "  PASS oracle fails on the buggy seed (the bug is unsolved, as expected)"
fi

echo
[ "$fail" -eq 0 ] && echo "selftest: OK" || { echo "selftest: FAILED"; exit 1; }
