# Loope Review Orchestration Implementation Plan (v0.4)

> Implement task-by-task with tests first. Keep automated tests hermetic (stub
> reviewer emits a parseable verdict; no binaries or network). Commit once at the end.

**Spec:** [Review Orchestration Spec (v0.4)](../specs/2026-06-28-loope-review-orchestration-spec.md)

**Goal:** Make review a structured, parallel, presettable phase of the loop.

---

## Tasks

### Task 1: Structured review verdicts

- [ ] Add `ReviewVerdict { has_blockers, summary }` and `parse_review_verdict(message)`
      (last `VERDICT: PASS|BLOCK` line, else conservative heuristic).
- [ ] Update the reviewer prompt to require a final `VERDICT:` line.
- [ ] Stub reviewer emits `VERDICT: PASS`.
- [ ] Capture Codex's final message structurally: `codex exec --json` + `-o <file>`;
      read the output file as the message (fallback: last agent message from JSONL).
- [ ] Record `has_blockers` on the reviewer outcome; show the verdict in the report.
- [ ] Gate: revise turn blocks if blockers were found and it made no change; passes as
      a no-op when no blockers.
- [ ] Unit tests for the parser and the revise-gate behavior under block/pass.

### Task 2: Parallel multi-reviewer

- [ ] `LoopOptions.reviewers: Vec<Adapter>` (default `[codex]`); `generate_plan` emits
      one reviewer step per reviewer in the review phase.
- [ ] Executor runs a maximal run of consecutive reviewer steps concurrently with
      `std::thread::scope`; each writes its own artifacts; aggregate `has_blockers`.
- [ ] Aggregated review summary is what the revise step receives.
- [ ] Keep the observer single-threaded (group rendering, no cross-thread calls).
- [ ] Unit tests: two stub reviewers run, both produce artifacts, aggregation correct.

### Task 3: Presets and CLI

- [ ] `--reviewers a,b` (comma list) plus existing single `--reviewer`.
- [ ] `--preset claude-codex|codex-claude|claude-solo|dual-review`, expanded to base
      options that explicit flags override.
- [ ] Update `print_help` and the README with `--preset` / `--reviewers`.
- [ ] Integration test: `run --dry-run --reviewers codex,claude` yields two reviewer
      steps in the run directory and report.

### Task 4: Verify + docs

- [ ] `cargo test` green with no binaries/network; `cargo clippy` clean.
- [ ] Real run: `loope run --preset dual-review --verify-cmd "cargo test" "..."`,
      confirm Codex + Claude both review with verdicts and the loop completes.
- [ ] README: document structured verdicts, parallel reviewers, presets.
