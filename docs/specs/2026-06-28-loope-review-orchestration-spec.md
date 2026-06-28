# Loope Review Orchestration Spec (v0.4)

## Background

The loop drives real agents, but the review phase is thin: a single reviewer whose
gate only checks that *some* output was produced. This phase makes review a
first-class, structured, multi-agent step:

1. **Structured verdicts** — parse the reviewer's conclusion so the gate decides on
   "are there blockers?" instead of merely "did the process exit 0?".
2. **Parallel multi-reviewer** — run several reviewers (e.g. Codex + Claude) at once on
   the same change and aggregate their verdicts.
3. **Presets** — name common agent combinations (`--preset claude-codex`) so a run is
   one short command.

## Goals

- A reviewer emits a machine-readable verdict; Loope parses it and feeds a real
  blocker signal into the gates and the report.
- Multiple reviewers run concurrently and their verdicts are aggregated (any blocker
  ⇒ blockers present).
- Common loops are reachable via a single `--preset`.

## Non-Goals (v0.4)

- A consensus/debate protocol between reviewers (just aggregation, not negotiation).
- Weighting or ranking reviewers by trust.
- Changing the dry-run stub semantics beyond emitting a parseable verdict.

## 1. Structured review verdicts

### Verdict contract

Every reviewer prompt ends with an explicit instruction to finish with one line:

```text
VERDICT: PASS        # no blocking issues
VERDICT: BLOCK       # one or more blocking issues must be fixed
```

Optionally preceded by findings. Loope parses the **last** `VERDICT:` line in the
reviewer's final message into a `ReviewVerdict { has_blockers, summary }`. If no
verdict line is present, Loope falls back to a conservative heuristic (phrases like
"no blocking findings" ⇒ pass; "blocker"/"must fix" ⇒ block; otherwise pass).

### Reliable message extraction

For Codex, the final agent message is captured structurally rather than scraped from
mixed stdout:

- run `codex exec --json` (JSONL event stream) for the transcript, and
- `codex exec ... -o <file>` (`--output-last-message`) to capture the final message
  verbatim; Loope reads that file as the message it parses.

If the output file is absent, Loope extracts the last agent message from the `--json`
event stream as a fallback. Claude's headless message is used as today.

### Gate semantics

- The reviewer step **passes** whenever a review was produced (a review is a valid
  artifact regardless of verdict); the parsed `has_blockers` is recorded on the
  outcome and shown in the report.
- The aggregated blocker signal drives the **revise** step: if blockers were found and
  the revise turn makes no change, that turn **blocks** (the blockers were not
  addressed); if no blockers were found, the revise turn may legitimately be a no-op.

## 2. Parallel multi-reviewer

- `LoopOptions.reviewers: Vec<Adapter>` (default `[codex]`). `generate_plan` emits one
  reviewer step per reviewer, all positioned in the same review phase (each reads the
  same implementer result).
- The executor runs a maximal run of consecutive reviewer steps **concurrently**
  (`std::thread::scope`); each reviewer writes to its own `agents/<role>-<adapter>/`
  directory, so there is no contention. Reviewers are read-only, so they never mutate
  the shared workspace.
- Aggregation: `has_blockers = any(reviewer.has_blockers)`. The aggregated summary
  (each reviewer's verdict) becomes the review artifact passed to the revise step.
- CLI: `--reviewers codex,claude` (comma list) in addition to single `--reviewer`.

## 3. Presets

`--preset <name>` expands to a base set of options that explicit flags still override:

| Preset         | Implementer | Reviewers       |
| -------------- | ----------- | --------------- |
| `claude-codex` | claude      | codex           |
| `codex-claude` | codex       | claude          |
| `claude-solo`  | claude      | claude          |
| `dual-review`  | claude      | codex, claude   |

`--verify-cmd`, `--design`, `--workdir`, etc. compose with any preset.

## CLI Surface (additions)

```bash
loope run --preset dual-review --verify-cmd "cargo test" "Add an endpoint"
loope run --reviewers codex,claude "Add an endpoint"
```

## Acceptance Criteria

- A reviewer whose message ends `VERDICT: BLOCK` is recorded as having blockers; the
  report shows the verdict; if the revise turn then makes no change, the loop blocks.
- A reviewer ending `VERDICT: PASS` lets a no-op revise turn pass.
- `--reviewers codex,claude` produces two reviewer steps that run concurrently, each
  with its own artifacts, and an aggregated blocker signal.
- `--preset dual-review` is equivalent to `--reviewers codex,claude` with a Claude
  implementer; explicit flags override the preset.
- Dry-run remains hermetic: the stub reviewer emits a parseable `VERDICT: PASS`, and
  the full suite stays green with no network or binaries.
- Verdict parsing and aggregation are unit-tested; `cargo clippy` clean.

## Related

- [[2026-06-28-loope-agent-integration-spec]] — real-CLI execution this builds on.
