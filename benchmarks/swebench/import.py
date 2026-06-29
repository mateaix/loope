#!/usr/bin/env python3
"""Import SWE-bench (Lite) instances into Loope benchmark cases.

Each instance becomes `cases/swe-<id>/`:
  seed/       the repo cloned at base_commit, with the instance's test_patch applied
  case.json   requirement = problem_statement; verify_cmd runs FAIL_TO_PASS + PASS_TO_PASS

Resolution follows SWE-bench exactly: an instance is *resolved* iff, on the final
workspace, all FAIL_TO_PASS tests pass AND all PASS_TO_PASS tests still pass. That is
precisely what `deliver.sh` checks when it re-runs `verify_cmd` as an independent oracle.
The gold patch is never written into the seed — only the bug and the hidden tests.

Usage:
  import.py --instances lite.jsonl [--out ../cases] [--limit N] [--runner pytest]
  import.py --instance one.json   [...]

A SWE-bench instance is a JSON object with at least:
  instance_id, repo ("owner/name" or a URL/path), base_commit, problem_statement,
  test_patch, FAIL_TO_PASS, PASS_TO_PASS

CAVEAT: faithful evaluation needs each repo's environment (its dependencies). The official
Dockerized SWE-bench harness is the source of record; this importer is for local
experimentation on instances whose dependencies you can install.
"""
import argparse
import json
import os
import re
import shlex
import shutil
import subprocess
import sys


def sh(args, cwd=None):
    subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True)


def repo_url(repo):
    if "://" in repo or repo.startswith("/") or repo.startswith("file:"):
        return repo
    return "https://github.com/%s.git" % repo


def slug(s):
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", s).strip("-")


def as_list(v):
    if isinstance(v, list):
        return v
    if isinstance(v, str):
        try:
            parsed = json.loads(v)
            return parsed if isinstance(parsed, list) else [v]
        except ValueError:
            return [v]
    return []


def runner_cmd(runner, tests):
    if not tests:
        return runner
    quoted = " ".join(shlex.quote(t) for t in tests)
    if runner == "pytest":
        return "python -m pytest -q %s" % quoted
    return "%s %s" % (runner, quoted)


def materialize(inst, out_dir, runner):
    iid = inst["instance_id"]
    case_id = "swe-" + slug(iid)
    case_dir = os.path.join(out_dir, case_id)
    seed = os.path.join(case_dir, "seed")
    os.makedirs(case_dir, exist_ok=True)
    if os.path.exists(seed):
        shutil.rmtree(seed)

    sh(["git", "clone", "--quiet", repo_url(inst["repo"]), seed])
    sh(["git", "-C", seed, "checkout", "--quiet", inst["base_commit"]])

    test_patch = inst.get("test_patch", "") or ""
    if test_patch.strip():
        patch = os.path.join(case_dir, "test_patch.diff")
        with open(patch, "w") as f:
            f.write(test_patch if test_patch.endswith("\n") else test_patch + "\n")
        sh(["git", "-C", seed, "apply", os.path.abspath(patch)])

    f2p = as_list(inst.get("FAIL_TO_PASS"))
    p2p = as_list(inst.get("PASS_TO_PASS"))
    case = {
        "id": case_id,
        "lang": "python",
        "requirement": (inst.get("problem_statement", "") or "").strip(),
        "verify_cmd": runner_cmd(runner, f2p + p2p),
        "trap": "SWE-bench instance %s (%s@%s). Resolution = all FAIL_TO_PASS pass and "
                "PASS_TO_PASS stay green." % (iid, inst["repo"], inst["base_commit"][:8]),
        "swebench": {
            "instance_id": iid,
            "repo": inst["repo"],
            "base_commit": inst["base_commit"],
            "fail_to_pass": f2p,
            "pass_to_pass": p2p,
        },
    }
    with open(os.path.join(case_dir, "case.json"), "w") as f:
        json.dump(case, f, indent=2)

    # Drop the cloned repo's history so we don't nest a huge .git in the loope repo.
    git_dir = os.path.join(seed, ".git")
    if os.path.isdir(git_dir):
        shutil.rmtree(git_dir)
    return case_dir


def load_instances(args):
    if args.instance:
        with open(args.instance) as f:
            return [json.load(f)]
    out = []
    with open(args.instances) as f:
        for line in f:
            line = line.strip()
            if line:
                out.append(json.loads(line))
    return out


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--instances", help="JSONL of SWE-bench instances")
    src.add_argument("--instance", help="a single instance JSON file")
    ap.add_argument("--out", default=os.path.join(here, "..", "cases"))
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--runner", default="pytest")
    args = ap.parse_args()

    insts = load_instances(args)
    if args.limit:
        insts = insts[: args.limit]
    out_dir = os.path.abspath(args.out)
    for inst in insts:
        try:
            d = materialize(inst, out_dir, args.runner)
            print("imported %s" % os.path.basename(d))
        except subprocess.CalledProcessError as e:
            print("FAILED %s: %s" % (inst.get("instance_id", "?"), e.stderr.strip().splitlines()[-1:]),
                  file=sys.stderr)


if __name__ == "__main__":
    main()
