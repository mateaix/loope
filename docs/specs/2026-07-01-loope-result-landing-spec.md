# Result Landing Spec — git worktree branches

## Background

`loope run` (default mode) copies the working tree into `.loope/runs/<id>/workspace/`, the
agents edit the **copy**, and the cumulative diff is captured to `changes.diff` /
`changed-files.txt`. The user's actual files are never touched unless they separately run
`loope apply <id>`.

A real run exposed the gap. In `matecloud`, the requirement was *"review
docs/rfcs/076-hive-trace-to-approval-bridge.md and see if there are issues."* The loop
converged and the implementer **did edit** the RFC (`changes.diff`, 139 lines, 1 file) — but
that edit lived only in `.loope/runs/0001-…/workspace/docs/rfcs/076-…md`. The real
`docs/rfcs/076-…md` was unchanged, so it looked like nothing happened. Worse, `.loope/` is
not git-ignored, so the 3,608-file workspace **copy** showed up as unversioned files
polluting the repo.

Two problems: **(1) results don't land** in the user's project (and `loope apply` is
undiscoverable), and **(2) the run copy pollutes VCS**.

## Goals

1. **Results are first-class git objects.** In a git repo, a run's output lands on a real
   branch `loope/<run-id>` that the user can `git diff`, merge, or open a PR from — the main
   working tree is never silently mutated. This is the worktree-isolation pattern modern
   agent harnesses use.
2. **Never pollute VCS.** `.loope/` is git-ignored automatically, so neither the run
   artifacts nor the workspace ever appear as unversioned files.
3. **Tell the user where the results are.** Every run ends with an explicit, copy-pasteable
   summary: the branch, how to review it, how to merge it, how to discard it.
4. **No regressions, `deps = 1`.** Non-git directories and `--in-place` keep working; the
   `loope` crate stays std-only (git is driven by shelling out, like the agent CLIs).

## Non-Goals (v1)

- A built-in merge/PR command (we print the `git` commands; a future `loope land` could
  automate merge + worktree cleanup).
- Carrying the source's **uncommitted** working-tree changes into the worktree — v1 branches
  from `HEAD` and **warns** when the tree is dirty (see Design). Including uncommitted/
  untracked work is a follow-up.
- Per-iteration commit history — v1 makes a single result commit per run.

## Design

### Workspace becomes a git worktree

When `loope run` starts in a git repo (detected via `git -C <source> rev-parse
--show-toplevel`) and neither `--in-place` nor `--copy` is set:

1. Create the run dir `.loope/runs/<id>/` as today.
2. `git -C <source> worktree add -b loope/<id> .loope/runs/<id>/workspace HEAD` — a fresh
   branch `loope/<id>` checked out into the workspace path. **The worktree IS the workspace**
   (no file copy). Branch/worktree names use the run-id slug, sanitized to a valid git ref.
3. Run the loop exactly as today (the executor already operates on `workspace_dir`).
4. On finalize (converged **or** stopped — partial work is still worth keeping), commit the
   result on the branch unless `--no-commit`:
   `git -C <worktree> add -A && git -C <worktree> commit -m "loope: <requirement> (<id>)"`.
   The branch now carries the work; `git diff <base>..loope/<id>` shows it.

If the source is **not** a git repo, or `git` is unavailable, fall back to today's **copy**
workspace (and auto-apply is offered per the end message). `--copy` forces this path.

### Dirty working tree

The worktree branches from `HEAD`, so the source's **uncommitted** changes are not included.
If the source working tree is dirty at run start, loope prints a warning: results are based
on the committed state (`HEAD`); commit/stash first, or use `--in-place` / `--copy` to
include uncommitted work. (Including dirty state in the worktree is a v2 enhancement.)

### Never pollute VCS

On run start in a git repo, ensure `.loope/` is ignored: if `.loope/.gitignore` is absent,
write it containing `*` (ignore the whole directory's contents, including itself). This
leaves the user's root `.gitignore` untouched and keeps the registered worktree + all run
artifacts out of the main repo's status.

### End-of-run guidance

Every run prints where the results are and how to get them. Worktree mode:

```
✓ converged · results on branch loope/0001-review-rfc
  review   git diff <base>..loope/0001-review-rfc
  merge    git merge loope/0001-review-rfc      (or open a PR)
  discard  git worktree remove .loope/runs/0001-review-rfc/workspace && \
           git branch -D loope/0001-review-rfc
```

Copy/non-git mode keeps the existing diff summary and points at `loope apply <id>`.

### Flags

| Flag | Meaning |
| --- | --- |
| *(default in a git repo)* | worktree mode: results on branch `loope/<id>` |
| `--copy` | force the copy workspace (the pre-worktree behavior) |
| `--in-place` | edit the source working tree directly (unchanged) |
| `--branch NAME` | override the result branch name (default `loope/<id>`) |
| `--no-commit` | leave the worktree's changes uncommitted on the branch |

`loope apply <id>` continues to copy a run's changed files into the working tree (the
fallback for copy/non-git runs); worktree runs are landed with `git merge`.

## Acceptance Criteria

- In a git repo, a default `loope run` produces a branch `loope/<id>` whose diff against the
  base equals the run's changes; the main working tree is unmodified; `.loope/.gitignore`
  exists so `git status` is clean of run artifacts.
- A non-git directory (or `--copy`) still runs via the copy workspace; `--in-place` still
  edits the source.
- The end-of-run summary prints the branch + review/merge/discard commands (worktree) or the
  `loope apply` hint (copy).
- A dirty source tree triggers the HEAD-based warning.
- `cargo test` / `cargo clippy` green with and without `--features tui`; `deps = 1`; no
  external project named in the tree.

## Testing Strategy

- **Git plumbing** (pure where possible): branch-name sanitization (`run-id → loope/<ref>`),
  the gitignore writer (idempotent, writes `*`), the dirty-tree check, the end-message
  builder — all unit-tested without a real repo.
- **Integration** (a temp `git init` repo): worktree add → edit → commit → `git diff
  base..branch` matches; `git status` in the main tree is clean; non-git fallback path.
- **Real run**: re-run the matecloud RFC review and confirm the edited RFC lands on
  `loope/<id>`, reviewable with `git diff`, with the main tree clean.

## Related

- [[2026-06-28-loope-iterative-loop-spec]] — the run/workspace/apply model this extends.
- [[2026-06-30-loope-loop-optimization-spec]] — convergence/stop reasons that gate the
  result commit.
