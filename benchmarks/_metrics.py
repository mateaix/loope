#!/usr/bin/env python3
"""Parse a Loope run into delivery metrics, and aggregate a set of runs.

Usage:
  _metrics.py record <run_dir> <resolved 0|1> <wall_ms> <case_id>   # one JSON record
  _metrics.py aggregate <records.jsonl> <meta_json>                 # summary JSON

`run.json` and `events.jsonl` are valid JSON, so we read them directly — no extra
instrumentation. `resolved` is supplied by the runner, which re-runs the case's verify_cmd
on the final workspace as an independent oracle (SWE-bench style).
"""
import glob
import json
import statistics
import sys


def load_run(run_dir):
    with open(run_dir + "/run.json") as f:
        return json.load(f)


def sum_tokens(run_dir):
    """Sum token usage across every step's events.jsonl."""
    ti = to = 0
    for ev in glob.glob(run_dir + "/agents/*/events.jsonl"):
        with open(ev) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    o = json.loads(line)
                except ValueError:
                    continue
                if o.get("type") == "usage":
                    ti += int(o.get("input_tokens", 0))
                    to += int(o.get("output_tokens", 0))
    return ti, to


def cmd_record(run_dir, resolved, wall_ms, case_id):
    d = load_run(run_dir)
    ti, to = sum_tokens(run_dir)
    print(json.dumps({
        "case": case_id,
        "resolved": bool(int(resolved)),
        "converged": bool(d.get("converged", False)),
        "iterations": int(d.get("iterations", 0)),
        "stop_reason": d.get("stop_reason", ""),
        "catch_and_fix": bool(d.get("highlight", False)),
        "tokens_in": ti,
        "tokens_out": to,
        "wall_ms": int(wall_ms),
    }))


def med(xs):
    return statistics.median(xs) if xs else 0


def cmd_aggregate(jsonl, meta_json):
    recs = [json.loads(line) for line in open(jsonl) if line.strip()]
    meta = json.loads(meta_json)
    n = len(recs) or 1
    resolved = [r for r in recs if r["resolved"]]
    total = sum(r["tokens_in"] + r["tokens_out"] for r in recs)
    wasted = sum(r["tokens_in"] + r["tokens_out"] for r in recs if not r["converged"])
    out = dict(meta)
    out.update({
        "samples": len(recs),
        "resolve_rate": round(len(resolved) / n, 3),
        "convergence_rate": round(sum(1 for r in recs if r["converged"]) / n, 3),
        "catch_and_fix_rate": round(sum(1 for r in recs if r["catch_and_fix"]) / n, 3),
        "median_iterations": med([r["iterations"] for r in recs]),
        "median_iterations_resolved": med([r["iterations"] for r in resolved]),
        "total_tokens": total,
        "tokens_per_resolved": (round(total / len(resolved)) if resolved else None),
        "wasted_token_ratio": (round(wasted / total, 3) if total else 0),
        "median_wall_ms": med([r["wall_ms"] for r in recs]),
        "records": recs,
    })
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "record":
        cmd_record(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5])
    elif cmd == "aggregate":
        cmd_aggregate(sys.argv[2], sys.argv[3])
    else:
        sys.exit("usage: _metrics.py record|aggregate ...")
