# Loope Live Execution Visibility Spec (v0.5)

## Background

Real runs work, but they are opaque. While a step runs you only see `running…`; when
it finishes you get a one-line gate note. You cannot see what the agent did, which
files it touched, or what changed. For a tool whose whole value is orchestrating real
agents, the execution must be **legible**.

The agent CLIs already stream rich event data that Loope currently throws away:

- **Claude** (`--output-format stream-json --verbose`) emits JSONL events: `assistant`
  messages with content blocks (`text`, and `tool_use` with a tool name + input such as
  `Write {file_path}` / `Edit` / `Bash`), `user` messages with `tool_result`, and a
  final `result`.
- **Codex** (`--json`) emits `item.started` / `item.completed` for `command_execution`
  and `agent_message`, plus `turn.completed` with token usage.

This phase parses those streams into a normalized activity feed, renders it live, and
surfaces the concrete file changes a step produced.

## Goals

1. **Live activity feed** — as each agent works, show its actions in real time: file
   reads/edits/writes, commands run, and a short assistant message.
2. **Change visibility** — after a write step, show the changed files with `+added
   −removed` line stats, persist a unified diff, and let `loope show --diff` print it.
3. **Richer step results** — the live recap and report show what each step actually
   did (message summary + changed files), not just the gate label.
4. **Safe + std-only** — plain/piped/CI output stays exactly as today; no new crates.

## Non-Goals (v0.5)

- A full-screen TUI, pager, or syntax-highlighted diffs.
- Per-tool interactive approval (the `--approve` gate stays run-level).
- A server, web feed, or cross-device streaming.
- Token-accurate cost accounting (token counts are shown when the CLI reports them,
  not computed).

## Core Concepts

### Normalized agent events

A single `LoopEvent` vocabulary that both adapters map onto, so the UI and persistence
never depend on a specific CLI's schema:

- `Model { name }` — the model a step is using.
- `Action { kind, target }` — `kind` ∈ `read | edit | write | command | search |
  other`; `target` is the file path or command (e.g. `edit src/lib.rs`,
  `command cargo test`).
- `Message { text }` — assistant text (kept short for display; full text stays in the
  transcript).
- `Usage { input_tokens, output_tokens }` — when reported.

Adapter-specific parsers convert each JSONL line into zero or more `LoopEvent`s:

- Claude: `assistant.content[].tool_use` → `Action` (map `Write`/`Edit`→edit,
  `Read`→read, `Bash`→command, `Grep`/`Glob`→search); `assistant.content[].text` →
  `Message`; `result` usage → `Usage`.
- Codex: `item.*` `command_execution` → `Action(command)`; `agent_message` → `Message`;
  `turn.completed.usage` → `Usage`.

### Streaming invocation

`SubprocessInvoker` reads the child's stdout **line by line** instead of buffering to
the end, parses each line into `LoopEvent`s, and pushes them to an event sink as they
arrive. It still produces the final `InvocationResult` (message, changed files, raw
transcript) and still writes `transcript.jsonl`; it additionally writes a normalized
`events.jsonl` per step.

To avoid disturbing existing invokers, the `Invoker` trait gains a streaming entry
point with a **default implementation** that calls the existing `invoke` and emits no
events. Only `SubprocessInvoker` overrides it; `StubInvoker` emits a couple of
synthetic events so `--dry-run` shows the feed shape.

### Change visibility

The workspace snapshot is extended so a write step's changes can be turned into a real
diff:

- For each changed file, compute `+added −removed` line counts and a unified diff using
  a small standard-library line-diff (LCS-based). Binary/oversized files are reported
  as "changed" without a diff.
- Per write step, persist `changes.diff` under the step's agent directory; record the
  per-file stats in `run.json`.
- Added files show as all-added; deleted files as all-removed.

### Live rendering

On a TTY, each step renders an indented, live activity feed under its header, then a
one-line result with change stats:

```text
  ▸ 1 implementer · Claude
      ✎ edit  src/lib.rs
      ▸ run   cargo build
      › Added multiply(a, b) with a test.
  ✓ 1 implementer · Claude   src/lib.rs +12 −0
```

Plain/piped output prints nothing extra during the run (unchanged), but the final
`report.md` gains the changed-file stats for every write step.

## CLI Surface (additions)

```bash
loope show --diff run-0001     # print the persisted diffs for a run
loope run --quiet "..."        # suppress the live feed; show only step results
```

`run`, `runs`, `show`, `plan`, `adapters`, and all existing flags are unchanged.

## Run Directory (additions)

```text
.loope/runs/run-0001/
  agents/implementer-claude/
    events.jsonl     normalized LoopEvents for the step
    changes.diff     unified diff of what the step changed (write steps)
    ... (prompt.md, transcript.jsonl, result.md as before)
```

## Acceptance Criteria

- During a real `loope run` on a TTY, each step shows a live feed of the agent's
  actions (edits, commands) and a short message as it works — not just `running…`.
- After a write step, the changed files and `+/−` line stats appear live and in
  `report.md`; a unified diff is persisted; `loope show --diff <id>` prints it.
- `--quiet` suppresses the live feed but keeps step results and the summary.
- Dry-run stays hermetic; the stub emits a few events so the feed shape is visible;
  plain/piped output and every existing test remain unchanged.
- Event parsers and the line-diff are unit-tested against captured samples; the full
  suite stays green with no binaries or network; `cargo clippy` clean; no new crates.

## Testing Strategy

- Unit-test the Claude and Codex event parsers against captured JSONL fixtures →
  expected `LoopEvent`s.
- Unit-test the line-diff (added/removed counts and unified output) on small inputs,
  including added and deleted files.
- Integration: `run --dry-run` writes `events.jsonl`; the report shows change stats for
  the stub implementer; `show --diff` prints the persisted diff.
- Manual: a real run demonstrates the live feed and real diffs end to end.

## Related

- [[2026-06-28-loope-agent-integration-spec]] — real-CLI execution this builds on.
- [[2026-06-28-loope-cli-ux-spec]] — the visual identity this extends.
