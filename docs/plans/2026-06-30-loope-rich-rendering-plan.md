# Rich Execution Rendering Plan

Implementation plan for [[2026-06-30-loope-rich-rendering-spec]]. Sequenced so each task is
independently verifiable; the `loope` crate stays `deps = 1`. Renders in both the TUI (Rust,
verifiable here) and the GUI cell model (written, compiled on the user's machine).

## T1 — Extend the normalized vocabulary (`adapter::event`)

- Add `LoopEvent::Reasoning { text }`, `Output { text }`, `Plan { text }`; add `ActionKind`
  `Fetch` and `Task` (with labels + ids + parse).
- Extend `to_json_line` / `parse_event_line` so all new kinds serialize and round-trip.
- Tests: JSONL round-trip for the new events + action kinds.
- **Verify:** core green both configs; `deps = 1`.

## T2 — Parser additions (`adapter::subprocess`)

- **Claude:** emit `Reasoning` from `thinking` blocks; `Output` (bounded) from `tool_result`
  content; `Plan` from `TodoWrite` input; map `WebFetch`→`Fetch`, `Task`→`Task`.
- **Codex:** `Reasoning` from reasoning; `Output` from exec output; keep apply_patch → edit.
- **OpenCode:** `Reasoning` + `Output` from its reasoning/tool-result steps.
- Best-effort: unrecognized shapes fall through unchanged.
- Tests: golden fixtures (real-shaped lines) → expected event lists incl. reasoning, output,
  a TodoWrite plan, web/task actions.
- **Verify:** existing parser tests pass; new goldens pass.

## T3 — Cell projection (`adapter::cell`)

- Add `Cell::Plan { text }`; project `Reasoning → Reasoning`, `Output → Exec`-output (or a
  dim output cell), `Plan → Plan` in `from_event` / the run reader.
- Round-trip the new cell; update `cells_from_events`.
- Tests: each new event → expected cell; JSONL round-trip.
- **Verify:** core green.

## T4 — Render in both surfaces

- **TUI** (`view/preview.rs` `activity_line` + per-step replay): reasoning dim/italic,
  `Output` as an indented dim block under its action, `Plan` as a checklist. Live "running"
  pane uses the same path.
- **GUI** (`src-tauri` backend DTO + `ui/app.js`): map the new cells to `CellDto` and render
  `Plan` (checklist) + `Output` (scrim block) + reasoning; agent stream events forward them.
- Tests: `activity_line` for the new events (feature build); existing snapshots unchanged.
- **Verify:** TUI green; GUI written (compiled by the user).

## T5 — Verify, docs, real check

- Full verification: `cargo test` / `cargo clippy` both feature configs; `deps = 1`; no
  external names.
- Docs: note the richer activity/cell content in `docs/guide/usage.md` (the run directory /
  TUI section) and the desktop `src-tauri/README.md`; README where relevant.
- Optional: re-run a real task and confirm reasoning + command output now show live.

## Related

- Spec: [[2026-06-30-loope-rich-rendering-spec]]
- [[2026-06-28-loope-live-visibility-spec]] · [[2026-06-29-loope-desktop-hub-spec]]
