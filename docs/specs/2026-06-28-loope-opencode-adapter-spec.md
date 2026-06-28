# Loope OpenCode Adapter Spec (v0.7)

## Background

Loope already drives Claude and Codex as real adapters; OpenCode is listed but is a
stub (`opencode run` with no real invocation, no event parsing). Loope's positioning is
to orchestrate **Claude, Codex, and OpenCode**, so this phase makes OpenCode a
first-class adapter that can fill any loop role.

OpenCode (v1.3.0) runs non-interactively as:

```text
opencode run [message..] --format json --dir <workdir> [-m provider/model] [--agent name]
```

- The prompt is passed as the `message` positional (not stdin).
- `--format json` emits a JSONL **event stream** on stdout; each line is an object with
  a `type` and a `sessionID`.
- `--dir` sets the working directory.
- OpenCode delegates to a configured **provider/model**. Its default provider may be
  unavailable for a given account — a probe here returned
  `{"type":"error",...,"statusCode":403,...}` ("not licensed to use Copilot"). So
  OpenCode availability depends on the user's provider configuration, and auth/provider
  errors must be handled as a **failed step**, never a crash.

## Goals

1. Drive `opencode` as a real adapter in any role (implementer / reviewer / designer)
   via `opencode run --format json`, captured and streamed like Claude and Codex.
2. Parse OpenCode's JSON event stream into Loope's `LoopEvent` vocabulary (live feed)
   and extract the final message.
3. Degrade gracefully when OpenCode's provider/auth fails: the step fails with the
   error message and the loop halts at that gate, with nothing left half-rendered.
4. Let the user point OpenCode at a working model when its default isn't usable.
5. Keep tests hermetic (stub unchanged); no new crates.

## Non-Goals (v0.7)

- OpenCode's server / web / ACP / MCP / GitHub modes.
- Session continue / resume / fork.
- Enforcing read-only through a CLI flag — `opencode run` exposes none; read-only roles
  request "review only, do not edit" in the prompt (a soft, prompt-level constraint).
- Changing the stub or any other adapter's behavior.

## Core Concepts

### Invocation

For the OpenCode adapter, the subprocess invoker builds:

```text
opencode run --format json --dir <workspace> [-m <model>] "<prompt>"
```

- **Prompt delivery is by argument**, not stdin (OpenCode's `run` takes the message as a
  positional). This overrides the default stdin delivery used by Claude/Codex.
- `--dir` is the run `workspace/`.
- `-m <model>` is added only when a model is configured for Loope (see below); otherwise
  OpenCode uses its own configured default.
- The working directory is also set as the process cwd, as for the other adapters.

### Model selection

OpenCode's default provider may be unlicensed for the account. Loope resolves an
optional model string and passes it as `-m`:

- env `LOOPE_OPENCODE_MODEL` (e.g. `anthropic/claude-...`), or
- a `--opencode-model provider/model` run flag (takes precedence).

When neither is set, Loope omits `-m` and relies on OpenCode's own configuration.

### Event parsing

`opencode run --format json` emits JSONL events keyed by `type`. The parser maps them to
`LoopEvent`s for the live feed:

- assistant text events → `Message`,
- tool / file-edit / command events → `Action { kind, target }` (edit/write/read/command
  mapped from the tool, as for the other adapters),
- token-usage events (when present) → `Usage`,
- `type:"error"` → not an event; it marks the invocation as failed and its message
  becomes the step's failure message.

The final message is the last assistant text event (with the raw JSONL kept as the
transcript). Exact field names are confirmed against a captured success-path sample
during implementation; unrecognized lines are ignored safely.

### Availability & failure

- A missing `opencode` binary is already handled (failed step, clear message).
- A provider/auth error (`type:"error"`, e.g. 403) makes the invocation fail with that
  message; the implementer/reviewer gate then blocks and the loop halts, exactly like
  any other failed step. No special-casing, no panic.
- Because the real path depends on a configured provider, it is verified manually, not
  in the automated suite.

### Roles

- **Implementer / designer**: write-capable; OpenCode edits the workspace; change
  detection (workspace before/after diff) already captures what it changed.
- **Reviewer / verifier**: read-only is requested in the prompt (no CLI enforcement);
  the structured-verdict contract (`VERDICT: PASS|BLOCK`) applies unchanged.

## CLI Surface (additions)

```bash
loope run --implementer opencode --reviewer codex "..."     # OpenCode implements
loope run --reviewers opencode,codex "..."                  # OpenCode reviews in parallel
loope run --opencode-model anthropic/claude-... "..."       # point OpenCode at a model
```

`opencode` is already a valid value everywhere adapters are parsed. A new `--preset`
value `opencode-codex` (OpenCode implements, Codex reviews) is added for convenience.

## Acceptance Criteria

- `loope run --implementer opencode ...` drives `opencode run --format json` in the run
  workspace; its actions stream to the live feed and its final message is captured in
  the report.
- A provider/auth error surfaces as a failed step carrying OpenCode's error message and
  halts the loop; Loope does not panic and the terminal is left clean.
- `--opencode-model` / `LOOPE_OPENCODE_MODEL` selects the model passed as `-m`.
- `loope run --dry-run` is unchanged; the whole test suite stays green with no binaries
  or network; `cargo clippy` clean; no new crates.
- The OpenCode event parser is unit-tested against captured JSON samples (including the
  `type:"error"` case).

## Testing Strategy

- Unit-test `parse_opencode_event` against captured JSONL samples: an assistant message,
  a tool/file action, and the `type:"error"` line (asserting it yields no event so the
  failure path is driven by the invocation result, not a spurious message).
- Unit-test model resolution (`--opencode-model` over `LOOPE_OPENCODE_MODEL` over none).
- Integration: dry-run and existing behavior unchanged.
- Manual: with a configured provider, a real `--implementer opencode` loop; and, with
  the default unlicensed provider, confirm the graceful failed-step behavior.

## Related

- [[2026-06-28-loope-agent-integration-spec]] — the adapter/execution model this extends.
- [[2026-06-28-loope-live-visibility-spec]] — the event vocabulary the parser feeds.
