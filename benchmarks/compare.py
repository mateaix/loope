#!/usr/bin/env python3
"""Compare two delivery snapshots (e.g. single-shot baseline vs full loop) into a table.

Usage: compare.py <baseline.json> <loop.json> [--out report.md]

Prints a metrics table with deltas and a per-case **attribution**: of the cases the loop
newly resolved, how many came from a reviewer catch-and-fix — the core test of whether the
review loop earns its tokens. Resolution per case is pass@k (resolved in >=1 sample).
"""
import argparse
import json
import sys

KEYS = [
    ("resolve_rate", "resolve rate", "pct"),
    ("convergence_rate", "convergence rate", "pct"),
    ("catch_and_fix_rate", "catch-and-fix rate", "pct"),
    ("median_iterations", "median iterations", "num"),
    ("tokens_per_resolved", "tokens / resolved", "num"),
    ("wasted_token_ratio", "wasted-token ratio", "pct"),
    ("median_wall_ms", "median wall ms", "num"),
]


def fmt(v, kind):
    if v is None:
        return "—"
    return ("%.1f%%" % (v * 100)) if kind == "pct" else ("%g" % v)


def delta(b, l, kind):
    if b is None or l is None:
        return "—"
    d = l - b
    return ("%+.1f pts" % (d * 100)) if kind == "pct" else ("%+g" % d)


def per_case(snap):
    """case -> resolved (pass@k), and case -> True if a resolved sample was a catch-and-fix."""
    resolved, caught = {}, {}
    for r in snap.get("records", []):
        c = r["case"]
        resolved[c] = resolved.get(c, False) or r["resolved"]
        if r["resolved"] and r.get("catch_and_fix"):
            caught[c] = True
    return resolved, caught


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("baseline")
    ap.add_argument("loop")
    ap.add_argument("--out")
    a = ap.parse_args()
    B = json.load(open(a.baseline))
    L = json.load(open(a.loop))

    out = []
    p = out.append
    p("# Loop vs baseline — %s" % L.get("preset", "?"))
    p("")
    p("- baseline: `%s` (mode=%s, max_iters=%s, samples=%s)"
      % (a.baseline, B.get("mode"), B.get("max_iters"), B.get("samples")))
    p("- loop:     `%s` (mode=%s, max_iters=%s, samples=%s)"
      % (a.loop, L.get("mode"), L.get("max_iters"), L.get("samples")))
    p("")
    p("| metric | baseline | loop | Δ |")
    p("| --- | --- | --- | --- |")
    for k, label, kind in KEYS:
        p("| %s | %s | %s | %s |" % (label, fmt(B.get(k), kind), fmt(L.get(k), kind),
                                     delta(B.get(k), L.get(k), kind)))
    p("")

    bres, _ = per_case(B)
    lres, lcf = per_case(L)
    cases = sorted(set(bres) | set(lres))
    lift = [c for c in cases if lres.get(c) and not bres.get(c)]
    regress = [c for c in cases if bres.get(c) and not lres.get(c)]
    via_cf = [c for c in lift if lcf.get(c)]

    p("## Attribution")
    p("")
    if lift:
        p("- Loop newly resolved **%d** case(s) the baseline missed: %s." % (len(lift), ", ".join(lift)))
        p("  Of these, **%d** (%.0f%%) via a reviewer **catch-and-fix**."
          % (len(via_cf), len(via_cf) / len(lift) * 100))
    else:
        p("- Loop resolved no case the baseline missed.")
    if regress:
        p("- ⚠ Loop lost **%d** case(s) the baseline solved: %s (over-edit / instability)."
          % (len(regress), ", ".join(regress)))
    p("")

    dr = (L.get("resolve_rate", 0) or 0) - (B.get("resolve_rate", 0) or 0)
    tb, tl = B.get("tokens_per_resolved"), L.get("tokens_per_resolved")
    prem = ("%.2f×" % (tl / tb)) if tb and tl else "n/a"
    p("## Verdict")
    p("")
    if dr > 0:
        p("The review loop **lifts resolve rate by %+.1f points** at a tokens-per-resolved "
          "premium of **%s**; %d/%d of the newly-resolved cases are attributable to a reviewer "
          "catch-and-fix." % (dr * 100, prem, len(via_cf), len(lift)))
    elif dr == 0:
        p("No resolve-rate difference in this run (premium %s). Add samples/cases, or the tasks "
          "are too easy/hard to separate the loop from a single shot." % prem)
    else:
        p("The loop **underperformed** the baseline by %.1f points here — check the regression "
          "list for over-editing." % (-dr * 100))

    text = "\n".join(out)
    print(text)
    if a.out:
        with open(a.out, "w") as f:
            f.write(text + "\n")
        print("\nwrote %s" % a.out, file=sys.stderr)


if __name__ == "__main__":
    main()
