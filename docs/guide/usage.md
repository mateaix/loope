# Loope Usage Guide

Loope is a Loop Engineering orchestrator: it turns one requirement into a repeatable
multi-agent loop — **design → implement → review → revise → verify** — driving real
coding-agent CLIs (Claude, Codex, OpenCode) with clear roles, artifacts, and gates.

This is the complete usage reference. For the design rationale, see the
[SDD documents](#sdd--design-documents).

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [Core concepts](#core-concepts)
- [Commands](#commands)
- [Adapters & providers](#adapters--providers)
- [Presets](#presets)
- [The run directory](#the-run-directory)
- [Terminal UI](#terminal-ui)
- [Diffs & change tracking](#diffs--change-tracking)
- [Design Contracts](#design-contracts)
- [Safety & isolation](#safety--isolation)
- [Exit codes](#exit-codes)
- [Environment variables](#environment-variables)
- [Non-interactive / CI use](#non-interactive--ci-use)
- [Troubleshooting](#troubleshooting)
- [Testing & development](#testing--development)
- [SDD / design documents](#sdd--design-documents)

## Install

Loope is a Rust 2024 project with **no external dependencies** (standard library only).

```bash
# build
cargo build --release           # target/release/loope

# or install on PATH
cargo install --path .          # ~/.cargo/bin/loope

# run from source without installing
cargo run -- <command> [args]
```

The deterministic `--dry-run` path needs no agent binaries or network. Real runs need
the agent CLIs you choose (see [Adapters & providers](#adapters--providers)).

## Quick start

```bash
# 1) Print the loop plan for a requirement (no execution)
loope plan "Add a billing settings page"

# 2) Execute the loop deterministically (stub agents — no CLIs, no network)
loope run --dry-run "Add a billing settings page"

# 3) Execute for real (Claude implements, Codex reviews, cargo test verifies)
loope run --verify-cmd "cargo test" "Add a multiply(a, b) function with a test"

# 4) Inspect a past run
loope runs                      # list runs with their outcome
loope show run-0001             # print a run's report
loope show run-0001 --diff      # report + the unified diffs of what changed
```

By default Loope operates on a **copy** of your project inside the run directory, so a
real run never touches your real source unless you pass `--in-place`.

## Core concepts

**Loop.** A controlled sequence of agent turns. The default loop is:

```text
requirement → implement (Claude) → review (Codex) → revise (Claude) → verify
```

With `--design`, a design step is inserted first:

```text
requirement → design (Claude) → implement → review → revise → verify
```

**Roles.** `designer`, `implementer`, `reviewer`, `verifier`. Each role maps to an
adapter (overridable). Implementer and designer are write-capable; reviewer and verifier
run read-only where the CLI supports it.

**Adapters.** Data describing how to launch a coding-agent CLI: `claude`, `codex`,
`opencode`, `generic`. See [Adapters & providers](#adapters--providers).

**Gates.** After every step Loope checks a gate. A step passes only if it succeeded and
produced its expected artifact. Reviewers emit a structured verdict (`VERDICT: PASS` /
`VERDICT: BLOCK`); if a review found blockers and the revise turn changes nothing, the
loop **blocks**. A blocking gate halts the loop and the report says which gate failed.

**Workspace & artifacts.** Each run gets its own directory under `.loope/runs/<run-id>/`
with a copied working tree and a numbered directory per step recording the exact prompt,
transcript, normalized events, result, and any diff. See
[The run directory](#the-run-directory).

## Commands

### `loope plan <requirement>`

Print the loop plan (roles, adapters, gates, and the prompt for each step). Does not run
anything.

- `--design` — include the design-contract step.

### `loope design [flags] <requirement>`

Run a **design-only** loop: one designer step that produces a Design Contract, prints it,
and writes `design-contract.md`. See [Design Contracts](#design-contracts).

Flags: `--designer A`, `--workdir DIR`, `--in-place`, `--dry-run`, `--timeout SECS`,
`--isolate-home`, `--opencode-model M`, `--color WHEN`, `--no-progress`.

### `loope run [flags] <requirement>`

Execute the full loop end to end and write a run directory.

| Flag | Meaning |
| --- | --- |
| `--dry-run` | Execute with deterministic stub agents (no CLIs, no network) |
| `--design` | Insert a design-contract step before implementation |
| `--workdir DIR` | Source directory to run against (default: current directory) |
| `--in-place` | Edit the working directory directly instead of a copied tree |
| `--approve auto\|manual` | `manual` confirms before launching any agent (default `auto`) |
| `--preset NAME` | A named adapter combination (see [Presets](#presets)) |
| `--implementer A` | Override the implementer adapter (default `claude`) |
| `--reviewer A` | Override the reviewer adapter (single, default `codex`) |
| `--reviewers A,B` | Run several reviewers in parallel and aggregate their verdicts |
| `--designer A` | Override the designer adapter (with `--design`, default `claude`) |
| `--verify-cmd C` | Run shell command `C` as the verifier; gate passes iff it exits 0 |
| `--opencode-model M` | `provider/model` for OpenCode (or `LOOPE_OPENCODE_MODEL`) |
| `--timeout SECS` | Per-step timeout (default `600`; `0` disables; or `LOOPE_TIMEOUT`) |
| `--isolate-home` | Give each agent a private CLI config dir (default: reuse your login) |
| `--quiet` | Suppress the live activity feed; keep step results and the summary |
| `--no-progress` | Disable the animated status line (keep committed step lines) |
| `--color WHEN` | `auto` (default), `always`, or `never` |

### `loope runs`

List past runs in `.loope/runs/`, each with its outcome and step count.

- `--color WHEN`.

### `loope show <run-id>`

Print a past run's report.

- `--diff` — also print the run's unified diffs (hunked, with a line-number gutter).
- `--color WHEN`.

### `loope adapters`

List the supported adapter ids.

## Adapters & providers

| Adapter | Default role | Binary | Override env |
| --- | --- | --- | --- |
| `claude` | implements, revises, designs | `claude` | `LOOPE_CLAUDE_BIN` |
| `codex` | reviews | `codex` | `LOOPE_CODEX_BIN` |
| `opencode` | any role | `opencode` | `LOOPE_OPENCODE_BIN` |
| `generic` | placeholder (no real CLI) | — | `LOOPE_GENERIC_BIN` |

**How each is driven (non-interactive, headless):**

- **Claude** — `claude -p --output-format stream-json --verbose`; prompt on stdin;
  write steps use `--permission-mode acceptEdits`, read-only steps use `plan`.
- **Codex** — `codex exec --json --skip-git-repo-check`; prompt on stdin; sandbox
  `read-only` for read-only steps, `workspace-write` otherwise; the final message is
  captured via `-o`.
- **OpenCode** — `opencode run --format json --dir <workspace>`; prompt as the message
  argument; needs a configured, licensed provider. Point it at a model with
  `--opencode-model provider/model` (or `LOOPE_OPENCODE_MODEL`). A provider/auth error
  surfaces as a failed step (the loop halts with the message), not a crash.

**Authentication.** By default Loope reuses each CLI's normal login, so it must be set up
and authenticated in your shell. `--isolate-home` gives each agent a fresh private config
dir instead (only useful if the CLI authenticates without the user's stored login).

## Presets

`--preset NAME` expands to a base adapter combination; explicit flags still override it.

| Preset | Implementer | Reviewers |
| --- | --- | --- |
| `claude-codex` | claude | codex |
| `codex-claude` | codex | claude |
| `claude-solo` | claude | claude |
| `dual-review` | claude | codex, claude (parallel) |
| `opencode-codex` | opencode | codex |

```bash
loope run --preset dual-review --verify-cmd "cargo test" "Add an endpoint"
```

## The run directory

Everything about a run lives under `.loope/runs/<run-id>/` (gitignored):

```text
.loope/runs/run-0001/
  plan.md                 the generated loop plan
  report.md               final loop report (per-step status, timing, outcome)
  run.json                machine-readable run record
  design-contract.md      the Design Contract (with --design or `loope design`)
  workspace/              the working tree the agents read and edit (a copy by default)
  agents/                 one numbered directory per step (the revise turn is its own)
    01-implementer-claude/
      home/               this step's private CLI config/session dir
      prompt.md           the exact prompt sent
      transcript.jsonl    the captured raw output stream
      events.jsonl        normalized events (actions, messages)
      result.md           the parsed result (message + changed files)
      changes.diff        the unified diff this step produced (write steps)
    02-reviewer-codex/...
    03-implementer-claude/...
    ...
```

Agent directories are numbered by step id, so the implement turn and the revise turn keep
separate, complete records.

## Terminal UI

On a TTY, `loope run` renders a small visual identity built from Loope's motifs — the `∞`
loop glyph and a `design → implement → review → verify` pipeline tinted by the logo
palette (Claude blue, Codex orange).

While a step runs, Loope streams the agent's actions as a **live activity feed** (file
reads/edits, commands, a short message), under an **animated status line** showing a
spinner, elapsed time, the last action, and `[n/m]` progress — updating ~10×/s even while
the agent is quiet:

```text
  ▸ 1 implementer · Claude
      ✎ edit   src/lib.rs
      ▸ run    cargo build
      › Added multiply(a, b) with a test.
  ⠹ implementer · Claude   0:42   ▸ run cargo build        [1/4]   ← live, animated
  ✓ 1 implementer · Claude   0:51   src/lib.rs +12 −0              ← committed on finish
```

The final summary lists every step with its gate result, verdict, change stats, and
duration, plus the run's total time.

- `--no-progress` keeps the committed lines but drops the animation.
- `--quiet` suppresses the per-event feed (keeps step results + summary).
- Color is automatic on a terminal and off when piped or `NO_COLOR` is set; override with
  `--color auto|always|never`. Brand colors downgrade to 256-color when the terminal
  lacks truecolor. Piped/CI output is plain.

## Diffs & change tracking

Because a CLI cannot reliably self-report what it changed, Loope detects a write step's
changes by diffing the workspace before and after. Each write step records `+added
−removed` line stats and a unified diff (`changes.diff`).

```bash
loope show run-0001 --diff
```

renders the diffs as `@@` hunks with a line-number gutter and `+`/`−` coloring; large
diffs collapse with a `… +N more lines` note.

## Design Contracts

`--design` (in `run`) and the standalone `loope design` command produce a **Design
Contract** — a markdown document covering user flows, UI states, component boundaries,
API/data contracts, and acceptance criteria.

```bash
loope design "Build a settings page for API keys"     # produce + print a contract
loope run --design --verify-cmd "cargo test" "..."    # design-aware loop
```

The contract is saved to `<run>/design-contract.md` and copied into the workspace as
`DESIGN_CONTRACT.md`, and is woven into the implementer prompt ("implement against the
contract") and the reviewer prompt ("check design consistency").

## Safety & isolation

- **Copy by default.** The working tree is a copy inside the run; your real source is
  untouched. `--in-place` opts into editing `--workdir` directly (commit first).
- **Confined writes.** Agents are pointed at the run `workspace/`; where the CLI supports
  an allow-list flag, that is the only writable directory.
- **Approval.** `--approve manual` prints the plan and waits for confirmation before
  launching any agent. `--approve auto` (default) runs unattended.
- **Per-step timeout.** Each agent step is time-bounded (`--timeout`, default 600s); on
  timeout the child is killed, the step fails with `timed out after Ns`, and the loop
  halts without hanging.
- **Skipped files.** Seeding the workspace skips `target/`, `.git/`, `.loope/`,
  `.claude/`, `node_modules/`, and `.DS_Store`.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success — the loop ran and all gates passed |
| `1` | The loop halted on a blocking gate, or a run/IO error occurred |
| `2` | Usage error (missing requirement, unknown flag value, bad workdir, etc.) |

This makes `loope run` usable as a gate in scripts and pipelines:

```bash
loope run --verify-cmd "cargo test" "..." && echo "passed" || echo "blocked"
```

## Environment variables

| Variable | Effect |
| --- | --- |
| `LOOPE_CLAUDE_BIN` | Override the `claude` binary path |
| `LOOPE_CODEX_BIN` | Override the `codex` binary path |
| `LOOPE_OPENCODE_BIN` | Override the `opencode` binary path |
| `LOOPE_GENERIC_BIN` | Set a binary for the `generic` adapter |
| `LOOPE_OPENCODE_MODEL` | Default `provider/model` for OpenCode (`--opencode-model` wins) |
| `LOOPE_TIMEOUT` | Default per-step timeout in seconds (`--timeout` wins) |
| `NO_COLOR` | Disable colored output |
| `FORCE_COLOR` | `3` forces truecolor, `1`/`2` forces 256-color |
| `COLORTERM` | `truecolor`/`24bit` enables truecolor brand colors |

## Non-interactive / CI use

- In a pipe / non-TTY / CI, output is plain line-streamed markdown (no cursor control,
  no spinner) — safe for logs.
- Use `--dry-run` for a hermetic loop with no agent binaries or network (e.g. to test
  Loope itself).
- Gate on the [exit code](#exit-codes).
- Running real agents in CI requires their auth in the environment (API keys via
  secrets) and consumes usage; agents edit a copy by default, so apply the resulting
  `changes.diff` deliberately or use `--in-place`.

## Troubleshooting

**Claude returns `403 Request not allowed` / auth failure.** The spawned `claude`
inherited your shell's Anthropic environment (e.g. a custom `ANTHROPIC_BASE_URL`). Verify
`printf hi | claude -p` works in the same shell; switch to a working environment or unset
the offending vars before running.

**OpenCode: `not licensed to use Copilot` / provider error.** OpenCode's default provider
isn't usable for your account. Point it at one with `--opencode-model provider/model` (or
`LOOPE_OPENCODE_MODEL`), or configure `opencode auth`. The step fails gracefully with the
message; the loop halts.

**A step hangs.** It will be killed at `--timeout` (default 600s) and reported as `timed
out after Ns`. Lower `--timeout` for fast feedback; `--timeout 0` disables the bound.

**`no program configured for adapter '…'`.** That adapter has no binary (e.g. `generic`)
or its override env points nowhere. Pick an installed adapter or set its `LOOPE_*_BIN`.

**The run copied a lot.** Large `--workdir` trees are copied (minus the skipped dirs).
Run from a small project dir, point `--workdir` at the right subtree, or use `--in-place`.

## Testing & development

```bash
cargo test            # full suite — no agent binaries or network required
cargo clippy --all-targets
cargo run -- run --dry-run "Add login"   # exercise the whole loop deterministically
```

All automated tests run the loop through the stub adapter (`--dry-run`), so CI is
hermetic. The real-CLI paths are verified manually.

## SDD / design documents

Loope is built spec-first. Each capability has a spec + plan under `docs/specs/` and
`docs/plans/`:

- MVP, Agent Integration, Review Orchestration, CLI UX, Live Execution Visibility,
  Live Terminal Rendering, OpenCode Adapter, Design Contract, and Robustness.

See the linked specs in the project [README](../../README.md#sdd-artifacts), or the
[docs index](../README.md).
