# Rich Execution Rendering Spec — deeper stream parsing

## Background

loope captures the Claude / Codex / OpenCode streams into a **thin** vocabulary: each
`tool_use` becomes a `LoopEvent::Action { kind, target }` (6 `ActionKind`s: read / edit /
write / run / search / other) plus assistant `Message`, `Model`, and `Usage`. The parsers
live in `src/adapter/subprocess.rs`.

This loses a lot of what the agents actually do. A real run against `OpenWiki` shows the gap:
its `events.jsonl` carried `thinking_tokens` the whole time, yet **reasoning is never
rendered**; a step runs `cargo test` but **its output is never shown**; a `TodoWrite` plan is
flattened to a generic "do"; `WebFetch` / `Task` (sub-agent) / MCP calls collapse into
`search` / `other`. So the live activity feed (TUI) and the cell transcript (GUI) read as a
*list of what was done*, not *what was done, its result, the plan, and the reasoning*.

A surveyed editor that "renders Claude/Codex" turned out to be a **contribution timeline**
(it POSTs writes/patches and shows commit metadata) — it parses *less* of the stream than
loope, so it is not the reference for richer-stream rendering. The enrichment must come from
parsing the **real Claude/Codex stream schemas** more deeply (the `codex-rs` history-cell
model is the closer guide). This spec does exactly that, and renders the result in **both**
the TUI activity feed and the GUI cell transcript.

## Goals

1. **Extend the normalized vocabulary** (`adapter::event::LoopEvent`) to capture what the
   streams already emit:
   - **Reasoning** — thinking / reasoning snippets.
   - **Output** — a tool's result / a command's stdout tail (the thing that was missing for
     `cargo test`).
   - **Plan** — a TodoWrite / plan checklist.
   - finer **Action** sub-kinds — `Fetch` (web) and `Task` (sub-agent) split out from the
     catch-all, so URLs / sub-tasks aren't lost.
   Each new event serializes to `events.jsonl` and round-trips (replay-identical).
2. **Extend the adapter parsers** (Claude, Codex, OpenCode) to emit the new events from the
   real streams — reasoning blocks, tool-result/exec-output content, `TodoWrite` input,
   `WebFetch`/`WebSearch`/`Task` tools.
3. **Render the richer vocabulary in both surfaces:**
   - **TUI** — the live activity feed (`view/preview.rs`) and the per-step replay show
     reasoning (dim), command/tool output (indented under the action), and the plan
     checklist.
   - **GUI** — the `adapter::cell` projection maps the new events onto cells
     (`Reasoning`, `Exec` output, a `Plan` cell), so the desktop transcript shows them too.
4. **No regressions** — the existing thin events still render; `deps = 1`; the print feed,
   the TUI, and the persisted-run readers keep working.

## Non-Goals (v1)

- Presentational polish borrowed from the surveyed editor (agent brand icons + colors,
  collapsible/lazy diffs, per-step summary bullets, affected-files panel, burst grouping) —
  a separate follow-up once the data is captured.
- Full structured tool-result objects (we capture a bounded text summary, not every field).
- Streaming partial deltas char-by-char (we commit per event, as today).
- The GUI cannot be compiled in this environment (Tauri toolchain) — its cell mapping and
  front-end rendering are written and reviewed but verified on the user's machine.

## Design

### New `LoopEvent` variants

```text
LoopEvent::Reasoning { text }     // thinking / reasoning (bounded)
LoopEvent::Output    { text }     // a tool result / command stdout tail (bounded)
LoopEvent::Plan      { text }     // a formatted TodoWrite / plan checklist
```

`ActionKind` gains `Fetch` (web) and `Task` (sub-agent). All persist via the existing
hand-rolled JSONL (`to_json_line` / `parse_event_line`) and round-trip.

### Parser additions (`subprocess.rs`)

- **Claude:** emit `Reasoning` from `thinking` blocks; `Output` from `tool_result` content
  (bounded tail); `Plan` from `TodoWrite` input (render the todos as a checklist); map
  `WebFetch`→`Fetch`, `WebSearch`→`Search`, `Task`→`Task`.
- **Codex:** emit `Reasoning` from reasoning events; `Output` from
  `exec_command_output_delta` / command output; keep `apply_patch` → an edit action.
- **OpenCode:** emit `Reasoning` from its reasoning step; `Output` from tool results.

Each is best-effort: unrecognized shapes fall through (no crash, no regression).

### Cell mapping (`adapter::cell`)

`Cell::from_event` projects the new events: `Reasoning → Cell::Reasoning`,
`Output → Cell::Exec`'s output (appended to the preceding exec) or a dim output cell,
`Plan → a new Cell::Plan { text }`. The GUI renders `Plan` as a checklist and `Output` as a
scrim block under the command.

### TUI rendering (`view/preview.rs`)

`activity_line` renders the new events: reasoning dim/italic, `Output` as an indented dim
block under its action, `Plan` as a checklist. The live "running" pane and per-step replay
both use it, so a long step shows its reasoning + output as it happens (not just a spinner).

## Acceptance Criteria

- `events.jsonl` carries and round-trips `Reasoning` / `Output` / `Plan` and the new
  `ActionKind`s; replaying a recorded run reproduces them byte-for-byte.
- Adapter fixtures (Claude/Codex/OpenCode JSONL) parse into the new events (golden tests).
- The TUI activity feed shows reasoning, command output, and the plan; the print feed and
  existing snapshots still pass.
- The GUI cell projection maps every new event to a cell (unit-tested in the std-only core).
- `cargo test` / `cargo clippy` green with and without `--features tui`; `deps = 1`; no
  external project named anywhere in the tree.

## Testing Strategy

- **Vocabulary round-trip** (pure): the new events + action kinds serialize/parse identically.
- **Parser goldens** (pure): real-shaped Claude/Codex/OpenCode lines → expected event lists,
  including reasoning, tool-result output, and a TodoWrite plan.
- **Cell projection** (pure): each new event → the expected cell.
- **TUI render** (feature build): `activity_line` for the new events produces the expected
  styled lines; existing render snapshots unchanged.

## Related

- [[2026-06-28-loope-live-visibility-spec]] — the normalized `LoopEvent` vocabulary this
  extends.
- [[2026-06-29-loope-desktop-hub-spec]] — the `adapter::cell` model the GUI renders.
- [[2026-06-28-loope-live-rendering-spec]] — the TUI activity feed this enriches.
