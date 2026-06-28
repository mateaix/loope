# Loope Agent Integration Spec (v0.2)

## Requirement Background

Loope v0.1 turns a requirement into a deterministic loop *plan* and prints prompts
for each role. It does not run anything. v0.2 makes the loop **actually execute**:
it drives the **Claude** and **Codex** command-line tools as real subprocesses, gives
each agent a managed working area, captures what each one produced, passes artifacts
between steps, and checks the gate after every step.

The bet stays the same as v0.1: the useful unit is not one stronger agent, but
**role separation plus repeatable gates**. v0.2 adds the missing half — real
execution — while keeping the loop reproducible and safe to run locally.

## Product Positioning

Loope is a thin local **orchestration layer** over existing coding-agent CLIs. It does
not replace Claude or Codex; it runs them in a controlled loop, manages their
workspaces, and records every turn as an inspectable artifact.

> Loope runs your coding agents in a loop: one implements, another reviews, each in
> its own managed workspace, with gates between every step.

## Goals

1. Execute a loop end-to-end by invoking the `claude` and `codex` CLIs.
2. Give each agent an isolated, managed workspace and session/home directory.
3. Persist every step (prompt, transcript, produced artifact, gate result) under a
   per-run directory so the loop is auditable and resumable.
4. Stay reproducible and CI-friendly: a `--dry-run` mode runs the whole loop with
   deterministic stub output and **no external binaries or network**.

## Non-Goals (v0.2)

- Web, desktop, or mobile UI; cross-device session continuation.
- Remote hosts, cloud sandboxes, or managed-host provisioning.
- A coordination server or any always-on background daemon.
- Real visual-design tooling (Figma import/export, screenshot review).
- Git automation, branch management, or PR creation.
- Parallel multi-agent debate or N-way voting.
- OS-level sandboxing (bwrap/seatbelt). Isolation in v0.2 is directory- and
  permission-based, not kernel-enforced.

## Core Concepts

### Adapter

An **adapter** describes how to launch and talk to one coding-agent CLI. It is data,
not behavior, so adapters can be added without touching the loop engine.

Fields:

- `id` — `claude` | `codex` | `generic`.
- `program` — the binary to run (default `claude` / `codex`), overridable by env var
  (`LOOPE_CLAUDE_BIN`, `LOOPE_CODEX_BIN`). If the resolved program is missing, the
  step is reported as *unavailable* and the run halts at that gate (or, under
  `--dry-run`, falls back to stub output).
- `invocation` — how a single turn is run: a **headless / non-interactive** mode
  (e.g. Claude's print mode, Codex's exec mode). Loope never drives an interactive
  TUI; it always runs one prompt → one captured result.
- `prompt_delivery` — argument or stdin.
- `output_format` — structured stream (JSON lines, when the CLI supports it) or raw
  text. Loope parses out the final agent message and any reported file changes.
- `workspace_env` — how the CLI is told which directory to work in (working
  directory and/or an explicit "additional allowed dir" flag).
- `write_capable` — whether this adapter is allowed to modify the workspace. The
  implementer is write-capable; reviewer and verifier run read-only where the CLI
  supports it.

### Agent Invocation

One subprocess run: a prompt, a workspace, captured stdout/stderr, an exit status, and
a **parsed result** (final message text, list of changed files, success/failure).
Invocations are non-interactive and time-bounded.

### Workspace & Isolation

A **run** owns a directory tree under `.loope/runs/<run-id>/` (already gitignored):

```text
.loope/runs/run-0001/
  run.json                      run metadata + status
  plan.md                       the generated loop plan
  workspace/                    the code the agents read and edit (the working tree)
  agents/
    implementer-claude/
      home/                     this agent's private CLI home/session dir
      prompt.md                 exact prompt sent
      transcript.jsonl          captured raw output stream
      result.md                 parsed result (message + changed files)
    reviewer-codex/
      home/
      prompt.md
      transcript.jsonl
      result.md
    ...
  report.md                     final loop report
```

Isolation rules:

- All agents in a run share **one** `workspace/` working tree so the reviewer sees
  the implementer's changes. The working tree is seeded from `--workdir` (default:
  current directory). Default is to operate on a **copy** inside the run so the source
  is never touched; `--in-place` opts into editing the real `--workdir`.
- Each agent gets its **own** `home/` (private CLI home / session directory). Agent
  home directories are kept separate so two agents never clobber each other's session
  state. Directory names are derived from the run id + role + adapter; any id used in
  a path is hashed/sanitized so it can never escape the run root.
- All run state files are written **atomically** (write to a temp file in the same
  directory, then rename) so a crash never leaves half-written JSON.
- The working directory passed to each CLI is the run `workspace/`, and — where the
  CLI supports an allow-list flag — that is the only writable directory granted.

### Session & Resume

`run.json` records the run id, requirement, options, per-step status, each agent's
session id, and the workspace path. A run can be re-opened by id to inspect or
continue. Cwd/workspace mismatches on resume are detected and reported rather than
silently editing the wrong tree.

### Gates

Gates are unchanged in concept from v0.1 but now evaluated against **real** outputs:
a step passes only if its invocation succeeded, produced the expected artifact, and
the gate predicate holds (e.g. implementer produced a non-empty diff; reviewer
emitted a review with blockers-first; verifier's checks exited zero). A blocking gate
halts the loop and the report says exactly which gate failed and why.

### Safety / Approval

- `--approve auto` (default): agents run unattended but confined to the run workspace.
- `--approve manual`: Loope prints the next agent's prompt and target workspace and
  waits for explicit confirmation before launching it.
- Reviewer and verifier run read-only when the adapter is not `write_capable`.
- Loope never grants an agent write access outside the run workspace.

## Execution Flow

```text
loope run [--design] "<requirement>"
  1. generate the loop plan (v0.1 engine)
  2. create .loope/runs/<run-id>/ and seed workspace/
  3. for each step in the plan:
       a. build the prompt (role + requirement + upstream artifacts)
       b. launch the step's adapter as a subprocess in workspace/
          (its own home/, read-only unless write_capable)
       c. capture + parse output, write prompt.md / transcript.jsonl / result.md
       d. evaluate the gate; on a blocking failure, stop the loop
  4. write report.md and finalize run.json
```

Each step's prompt includes the relevant **upstream artifacts**: the reviewer is given
the implementer's `result.md` (and changed-file list); the second implementer turn is
given the review; the verifier is given the final state.

## CLI Surface

```bash
loope run "Add login"                       # execute the default loop
loope run --design "Build dashboard"        # execute the design-aware loop
loope run --dry-run "Add login"             # deterministic stub run, no binaries
loope run --workdir ./app --in-place "..."  # edit a specific tree in place
loope run --approve manual "..."            # confirm before each agent
loope runs                                  # list past runs
loope show run-0001                         # print a past run's report
```

`plan`, `plan --design`, and `adapters` from v0.1 are retained unchanged.

## Acceptance Criteria

- `loope run --dry-run "Add login"` executes the full loop deterministically, creates
  `.loope/runs/<run-id>/` with `plan.md`, per-agent prompt/transcript/result files, and
  `report.md`, evaluates every gate, and exits 0.
- With `claude` and `codex` installed, `loope run "<requirement>"` drives each tool in
  non-interactive mode inside its managed workspace and produces a report; this path is
  covered by a documented manual-verification procedure (it needs the real CLIs).
- Each agent uses a separate `home/`; the reviewer step receives the implementer's
  result; no agent writes outside the run workspace.
- A blocking gate halts the loop and the report names the failing gate.
- `cargo test` passes with **no** external binaries or network (all automated tests use
  `--dry-run` / stubbed adapters).
- `README.md` documents `run`, `runs`, `show`, `--dry-run`, and the workspace layout.

## Testing Strategy

- **Hermetic by default:** all automated tests run the loop through a stub adapter
  (equivalent to `--dry-run`) so CI needs neither `claude`/`codex` nor a network.
- Unit tests cover prompt assembly, artifact passing, gate evaluation, run-directory
  layout, and atomic writes.
- Integration tests run `loope run --dry-run` end-to-end and assert the run directory
  structure, report contents, and exit codes.
- The real-CLI path is exercised by a manual checklist documented in the README, since
  it depends on external tools and credentials.

## Related Knowledge

- [[2026-06-28-loope-mvp-spec]] — v0.1 plan-generation spec this builds on.
