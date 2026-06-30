# Result Landing Plan

Implementation plan for [[2026-07-01-loope-result-landing-spec]]. Sequenced so each task is
independently verifiable; the `loope` crate stays `deps = 1` (git is driven by shelling out,
like the agent CLIs).

## T1 — Don't pollute VCS: auto-gitignore `.loope/`

- A small `git` module (`engine::git`) with `ensure_loope_ignored(source)`: when `source` is
  a git repo and `.loope/.gitignore` is absent, write it containing `*`.
- Call it at run start (and run-dir creation) so artifacts never show as unversioned.
- Tests: idempotent; writes `*`; no-op outside a git repo.
- **Verify:** core green; `git status` clean of `.loope` in a temp repo.

## T2 — Git detection + worktree workspace

- `engine::git`: `repo_root(source)`, `is_dirty(source)`, `sanitize_ref(run_id) -> branch`,
  `worktree_add(source, branch, path, base)`, `worktree_remove`, `commit_all`,
  `merge_base`/`base_ref` (the HEAD sha at creation, recorded for the diff command).
- `RunWorkspace::prepare` gains a mode: **Worktree** (git repo, no `--in-place`/`--copy`),
  **Copy** (today's behavior), **InPlace**. Worktree mode runs `worktree_add -b loope/<id>`
  instead of copying; `workspace_dir` is the worktree path.
- Fall back to Copy when not a git repo or git is unavailable.
- Tests: ref sanitization; mode selection; integration (`git init` temp → worktree created,
  workspace_dir is the worktree).
- **Verify:** existing workspace tests pass; new ones pass.

## T3 — Commit results + dirty-tree warning

- In `finalize` (converged or stopped), when in Worktree mode and not `--no-commit`:
  `commit_all(worktree, "loope: <requirement> (<id>)")`. Record the base sha + branch on the
  run record (`run.json`).
- At run start, if the source tree is dirty, surface the HEAD-based warning through the
  observer / printed output.
- Tests: commit message format; the run record carries branch + base; dirty check.
- **Verify:** integration — after a stub run, `git diff base..loope/<id>` shows the change.

## T4 — End-of-run guidance + flags + apply

- Build the end-of-run summary: worktree → branch + review/merge/discard commands; copy →
  existing diff summary + `loope apply <id>` hint. Wire into the print feed, the TUI, and
  `loope show`.
- CLI: add `--copy`, `--branch NAME`, `--no-commit`; keep `--in-place`. Document them.
- `loope apply` unchanged for copy/non-git; note that worktree runs land via `git merge`.
- Tests: summary builder for each mode; flag parsing.
- **Verify:** core green both configs.

## T5 — Verify, docs, real run

- Full verification: `cargo test` / `cargo clippy` both feature configs; `deps = 1`.
- Docs: `docs/guide/usage.md` (run modes, the run directory, the new flags, the landing
  story), README, and the SDD links; note the worktree default + `.loope/` gitignore.
- Real run: re-run the matecloud RFC review; confirm the edit lands on `loope/<id>`,
  reviewable via `git diff`, main tree clean.

## Related

- Spec: [[2026-07-01-loope-result-landing-spec]]
- [[2026-06-28-loope-iterative-loop-spec]]
