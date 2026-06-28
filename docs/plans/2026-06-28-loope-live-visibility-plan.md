# Loope Live Execution Visibility Implementation Plan (v0.5)

> Implement task-by-task with tests first. Keep automated tests hermetic (stub emits
> synthetic events; line-diff and parsers tested on fixtures). Commit once at the end.

**Spec:** [Live Execution Visibility Spec (v0.5)](../specs/2026-06-28-loope-live-visibility-spec.md)

**Goal:** Make a run legible — stream each agent's actions live and surface the real
file changes it produced.

**Architecture:** Add a normalized `LoopEvent` vocabulary and an event sink. The
`Invoker` trait gains a streaming entry point with a default (no-op) implementation, so
only `SubprocessInvoker` changes. A small std-only line-diff turns workspace snapshots
into per-file stats and unified diffs. The UI renders events live and the report/`show`
gain change stats and `--diff`.

**Tech Stack:** Rust 2024 edition, standard library only. No new crates.

---

## Tasks

### Task 1: LoopEvent vocabulary + streaming trait entry point

- [ ] Add `LoopEvent` (`Model`, `Action { kind, target }`, `Message`, `Usage`) and an
      `ActionKind` enum in the library.
- [ ] Add `Invoker::invoke_streaming(&self, inv, sink: &mut dyn FnMut(LoopEvent)) ->
      InvocationResult` with a default impl that calls `invoke` and emits nothing.
- [ ] Executor calls `invoke_streaming`, forwarding events to the observer; existing
      invokers and tests keep compiling unchanged.

### Task 2: Adapter event parsers

- [ ] `parse_claude_event(line) -> Vec<LoopEvent>` for `--output-format stream-json`
      (`assistant` text + `tool_use`, `result` usage).
- [ ] `parse_codex_event(line) -> Vec<LoopEvent>` for `--json` (`command_execution`,
      `agent_message`, `turn.completed` usage).
- [ ] Map tool names to `ActionKind` (Write/Edit→edit, Read→read, Bash→command,
      Grep/Glob→search, else other).
- [ ] Unit tests against captured JSONL fixtures from both CLIs.

### Task 3: Streaming subprocess invoker

- [ ] `SubprocessInvoker::invoke_streaming` reads stdout line by line, parses events,
      pushes them to the sink, and accumulates the raw transcript.
- [ ] Add `--output-format stream-json --verbose` for Claude; keep `--json` for Codex.
- [ ] Still derive the final message (Codex `-o` file / last `agent_message`; Claude
      `result`/last text) and write `events.jsonl` alongside `transcript.jsonl`.
- [ ] Keep stderr capture and the missing-binary / nonzero-exit failure path.

### Task 4: Line diff + change stats

- [ ] Extend the snapshot to capture per-file content (or enough to diff): a
      `workspace::diff_file(before, after) -> FileDiff { added, removed, unified }`
      using a small LCS line-diff.
- [ ] After a write step, compute changed files with `+/−` stats and a combined unified
      diff; persist `changes.diff`; include per-file stats on the `StepOutcome`.
- [ ] Report shows `path +A −R` per write step; `run.json` includes the stats.
- [ ] Unit tests for the diff (modify, add, delete; counts + unified output).

### Task 5: Live UI + report + show --diff

- [ ] `PrettyObserver` renders the live feed: indented action/message lines under the
      step header while it runs, then the result line with change stats.
- [ ] `--quiet` suppresses the feed (keep step results + summary).
- [ ] `ui::print_report` shows change stats per step; `loope show --diff <id>` prints
      the persisted diffs (plain + colored).
- [ ] Non-TTY/plain output stays byte-compatible with today for existing assertions.

### Task 6: Verify + docs + real run

- [ ] `cargo test` green with no binaries/network; `cargo clippy` clean; no new crates.
- [ ] `run --dry-run` shows the feed shape and writes `events.jsonl`; `show --diff`
      prints the stub change.
- [ ] Real run: confirm the live feed shows Claude's edits + Codex's review actions and
      that `--diff` shows the real change.
- [ ] README: document the live feed, change stats, `--diff`, and `--quiet`.
