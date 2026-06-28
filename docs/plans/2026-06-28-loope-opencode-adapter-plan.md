# Loope OpenCode Adapter Implementation Plan (v0.7)

> Implement task-by-task with tests first. Keep the suite hermetic (stub unchanged); the
> real OpenCode path is manual-only. Commit once at the end.

**Spec:** [OpenCode Adapter Spec (v0.7)](../specs/2026-06-28-loope-opencode-adapter-spec.md)

**Goal:** Make `opencode` a real adapter Loope can drive in any loop role.

**Architecture:** Extend the existing subprocess invoker and event parsers; OpenCode's
prompt is delivered as a `run` message argument (not stdin), its `--format json` event
stream feeds the existing `LoopEvent` pipeline, and provider/auth errors flow through the
existing failed-step path. A model can be selected via flag/env.

**Tech Stack:** Rust 2024, standard library only. No new crates.

---

## Tasks

### Task 1: OpenCode invocation

- [ ] In `configure_command`, build OpenCode as `opencode run --format json --dir
      <workspace> [-m <model>]` and deliver the prompt as a message argument.
- [ ] Make prompt delivery per-adapter: stdin for Claude/Codex, argument for OpenCode
      (don't write the prompt to stdin for OpenCode).
- [ ] Resolve the model: `--opencode-model` flag (plumbed via the invocation) over env
      `LOOPE_OPENCODE_MODEL` over none; pass `-m` only when set.
- [ ] Derive the final message from OpenCode's stream (last assistant text); map a
      `type:"error"` stream to a failed `InvocationResult` with the error message.

### Task 2: OpenCode event parser

- [ ] `parse_opencode_event(line) -> Vec<LoopEvent>` for the `--format json` JSONL:
      assistant text → `Message`, tool/file events → `Action`, usage → `Usage`.
- [ ] Map OpenCode tool names to `ActionKind` (edit/write/read/command/search/other).
- [ ] Wire it into `parse_event` and the adapter-message extraction.
- [ ] Unit tests against captured samples, including the `type:"error"` line (yields no
      event).

### Task 3: CLI + preset

- [ ] Add `--opencode-model provider/model` to `run` (threaded into the invoker).
- [ ] Add the `opencode-codex` preset (OpenCode implements, Codex reviews).
- [ ] Update `print_help` and the README adapter table / presets.

### Task 4: Verify + docs + real run

- [ ] `cargo test` green, no binaries/network; `cargo clippy` clean; no new crates.
- [ ] Confirm the graceful failed-step path when OpenCode's provider is unlicensed
      (the run halts with OpenCode's error message; no panic).
- [ ] If a working provider is configured, a real `--implementer opencode` loop end to
      end; otherwise document the provider-config requirement.
- [ ] README: OpenCode in the adapter table, `--opencode-model`, the new preset, and the
      provider-configuration note.
