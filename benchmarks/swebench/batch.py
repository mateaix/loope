#!/usr/bin/env python3
"""Resumable SWE-bench Lite batch for Loope.

Samples instances, runs the loope loop on each (clone @ base_commit + the hidden test_patch
so loope can verify), and appends a SOURCE-ONLY prediction to the output JSONL **as each
instance finishes** — so a kill loses at most the in-flight instance. Re-run with the same
args to resume (instances already in the output are skipped). Evaluate the result with the
official harness via `eval.sh`.

  batch.py --out preds.jsonl --sample 12 --seed 7 [--repos a,b] [--timeout 1500]

By default it samples from repos whose environment installs reasonably ad-hoc, so loope's
local `verify_cmd` actually runs — a *fair* measure of the review loop. Heavy repos
(django/sympy/sklearn/…) need the official Dockerized env for loope to verify; they are out
of scope for this local harness.
"""
import argparse, json, os, random, shlex, subprocess, sys, time

# repos that install cleanly enough for a local pytest verify
DEFAULT_REPOS = ["psf/requests", "pallets/flask", "pylint-dev/pylint",
                 "pytest-dev/pytest", "marshmallow-code/marshmallow"]
PINS = ["pytest<8", "werkzeug<3", "markupsafe<2.2"]  # common-era pins; harmless if unused


def sh(args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)


def done_set(path):
    s = set()
    if os.path.exists(path):
        for line in open(path):
            line = line.strip()
            if line:
                s.add(json.loads(line)["instance_id"])
    return s


def as_list(v):
    return json.loads(v) if isinstance(v, str) else v


def test_files(inst):
    out = set()
    for line in inst["test_patch"].splitlines():
        if line.startswith(("+++ b/", "--- a/")):
            out.add(line[6:])
    return out


def is_test_path(p):
    b = os.path.basename(p)
    return ("/tests/" in p or p.startswith("tests/") or b.startswith("test_")
            or b == "conftest.py")


def is_excluded(p):
    # loope's own run artifacts + build/cache junk — never part of the fix
    return (p.startswith(".loope/") or "__pycache__" in p or p.endswith((".pyc", ".pyo"))
            or ".egg-info" in p or ".pytest_cache" in p
            or p.startswith(("build/", "dist/", ".tox/", ".eggs/")))


def extract_source_patch(work, base, inst):
    # revert the gold test files, then diff everything except test / junk / artifact files
    tf = test_files(inst)
    for f in tf:
        sh(["git", "-C", work, "checkout", base, "--", f])
    sh(["git", "-C", work, "add", "-A"])
    staged = sh(["git", "-C", work, "diff", "--cached", "--name-only"]).stdout.split()
    for f in staged:
        if is_test_path(f) or is_excluded(f) or f in tf:
            sh(["git", "-C", work, "restore", "--staged", "--", f])
    return sh(["git", "-C", work, "diff", "--cached"]).stdout


def run_instance(inst, root, timeout):
    iid = inst["instance_id"]
    work = os.path.join(root, iid)
    sh(["rm", "-rf", work])
    sh(["git", "clone", "--quiet", "https://github.com/%s" % inst["repo"], work])
    sh(["git", "-C", work, "checkout", "--quiet", inst["base_commit"]])
    tp = os.path.join(root, iid + ".test.diff")
    open(tp, "w").write(inst["test_patch"] + ("" if inst["test_patch"].endswith("\n") else "\n"))
    sh(["git", "-C", work, "apply", tp])

    venv = work + "_venv"
    sh([sys.executable, "-m", "venv", venv])
    sh([os.path.join(venv, "bin", "pip"), "install", "-q", "-e", work, "pytest"] + PINS)

    verify = "python -m pytest -q " + " ".join(
        shlex.quote(t) for t in (as_list(inst["FAIL_TO_PASS"]) + as_list(inst["PASS_TO_PASS"])))
    env = dict(os.environ)
    env["PATH"] = os.path.join(venv, "bin") + ":" + env["PATH"]
    try:
        sh(["loope", "run", "--quiet", "--in-place", "--preset", "claude-codex",
            "--verify-cmd", verify, "--max-iters", "3", inst["problem_statement"]],
           cwd=work, env=env, timeout=timeout)
    except subprocess.TimeoutExpired:
        pass

    patch = extract_source_patch(work, inst["base_commit"], inst)
    sh(["rm", "-rf", venv])  # reclaim disk
    return {"instance_id": iid, "model_name_or_path": "loope-claude-codex", "model_patch": patch}


def main():
    from datasets import load_dataset
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--sample", type=int, default=12)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--repos", default=",".join(DEFAULT_REPOS))
    ap.add_argument("--timeout", type=int, default=1500)
    ap.add_argument("--workroot", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".swebatch"))
    a = ap.parse_args()

    repos = set(a.repos.split(","))
    ds = [r for r in load_dataset("princeton-nlp/SWE-bench_Lite", split="test") if r["repo"] in repos]
    random.Random(a.seed).shuffle(ds)
    sample = ds[: a.sample]
    root = os.path.abspath(a.workroot)
    os.makedirs(root, exist_ok=True)

    done = done_set(a.out)
    print("sample=%d already_done=%d repos=%s" % (len(sample), len(done), sorted(repos)), flush=True)
    for i, inst in enumerate(sample, 1):
        iid = inst["instance_id"]
        if iid in done:
            print("[%d/%d] skip %s (done)" % (i, len(sample), iid), flush=True)
            continue
        t0 = time.time()
        print("[%d/%d] %s ..." % (i, len(sample), iid), flush=True)
        try:
            rec = run_instance(inst, root, a.timeout)
        except Exception as e:
            rec = {"instance_id": iid, "model_name_or_path": "loope-claude-codex", "model_patch": ""}
            print("  ERROR %s" % e, flush=True)
        with open(a.out, "a") as f:           # append → crash-safe
            f.write(json.dumps(rec) + "\n")
        print("  banked %s (%db, %ds)" % (iid, len(rec["model_patch"]), int(time.time() - t0)), flush=True)
    print("batch complete: %d predictions in %s" % (len(done_set(a.out)), a.out), flush=True)


if __name__ == "__main__":
    main()
