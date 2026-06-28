# Loope Robustness Spec (v0.9)

## Background

Two reliability gaps remain in the real-run path:

1. **No subprocess timeout.** An agent subprocess that hangs (no output, never
   exits) hangs the entire run — Loope reads its stdout to end-of-stream and then
   waits for it to exit, with no bound. The v0.2 spec promised "time-bounded"
   invocations; that bound was dropped to avoid a pipe deadlock and never restored.
2. **Per-step artifacts overwrite each other.** A step's files live in
   `agents/<role>-<adapter>/`, keyed only by role and adapter. The implementer runs
   twice (implement, then revise), so the revise turn overwrites the implement turn's
   `prompt.md` / `transcript.jsonl` / `events.jsonl` / `result.md` / `changes.diff`.
   The first turn's record is lost.

This phase makes both right: every agent run is time-bounded, and every step (turn)
keeps its own artifacts.

## Goals

1. Each agent subprocess is **time-bounded**. On timeout it is killed and the step
   fails with a clear "timed out" message; the loop halts cleanly with no hang.
2. The timeout is configurable (`--timeout SECS`, `LOOPE_TIMEOUT`), with a sensible
   default; `0` disables it.
3. Each step persists to its **own** `agents/<NN>-<role>-<adapter>/` directory, so the
   implement and revise turns (and every step) keep separate, complete records.
4. No regressions: dry-run and plain/CI output behavior unchanged; tests stay green;
   `cargo clippy` clean; no new crates.

## Non-Goals (v0.9)

- Cancelling a single tool call mid-flight, or recovering partial output after a kill
  (the killed step is simply a failed step).
- Cross-platform process-group/child-tree killing beyond `std`'s `Child::kill`
  (grandchild processes a CLI spawns are out of scope).
- Retrying a timed-out step automatically.

## Core Concepts

### Per-step timeout

The subprocess invoker reads the child's stdout on a dedicated thread (sending lines
back over a channel) while the main thread waits with a deadline:

- lines arrive → parsed into events and accumulated as before;
- if the deadline passes with the child still running → the child is **killed**, the
  step result is a failure (`"timed out after Ns"`), and the loop halts at that gate;
- if stdout closes / the child exits first → normal completion.

This keeps the event sink on the main thread (no new `Send` requirements) and avoids
the pipe deadlock that motivated dropping the original timeout. The stderr drain thread
is unchanged.

The timeout is a `Duration` on the subprocess invoker. The CLI resolves it from
`--timeout SECS` (precedence) or `LOOPE_TIMEOUT` (seconds), defaulting to **600s**;
`--timeout 0` disables the bound. Dry-run uses the stub invoker, which never spawns a
process, so the timeout does not apply there.

### Per-step artifact directories

Agent directories become keyed by step id: `agents/<NN>-<role>-<adapter>/` (e.g.
`agents/1-implementer-claude/`, `agents/4-implementer-claude/`). The `NN` is the step's
1-based id, zero-padded for stable sorting; role and adapter remain sanitized. Each
agent's private `home/` lives under its step directory.

Effects:

- The implement turn and the revise turn no longer share a directory; both keep their
  full record (prompt, transcript, events, result, and any diff).
- `loope show --diff` reads the per-step `changes.diff` files in step order (the `NN`
  prefix sorts correctly), so the real implementation diff is always shown.
- The run directory layout in docs is updated to the numbered form.

## CLI Surface (additions)

```bash
loope run --timeout 120 "..."     # bound each agent step to 120s
loope run --timeout 0 "..."       # no timeout
LOOPE_TIMEOUT=300 loope run "..." # via env (seconds)
```

`--timeout` also applies to `loope design`.

## Acceptance Criteria

- A hanging agent is killed at the timeout; the step fails with `timed out after Ns`,
  the loop halts, and the process exits without hanging.
- `--timeout` / `LOOPE_TIMEOUT` set the bound; `0` disables it; default is 600s.
- A design-aware loop produces distinct numbered agent directories
  (`1-designer-*`, `2-implementer-*`, `3-reviewer-*`, `4-implementer-*`,
  `5-verifier-*`); the implement and revise turns keep separate artifacts.
- `loope show --diff` still shows the implementation diff.
- `cargo test` stays green with no binaries or network; `cargo clippy` clean; no new
  crates.

## Testing Strategy

- Unit: `agent_dir` includes the zero-padded step id and stays sanitized; timeout
  resolution (`--timeout` over `LOOPE_TIMEOUT` over default; `0` → disabled).
- Integration: a `run --dry-run --design` produces the numbered agent directories and
  both implementer turns' `result.md` exist separately.
- The kill-on-timeout path depends on a real long-running subprocess and is verified
  manually (e.g. a stub binary that sleeps), documented in the README.

## Related

- [[2026-06-28-loope-agent-integration-spec]] — the execution/workspace model this hardens.
