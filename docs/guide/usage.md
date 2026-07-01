# Loope Usage Guide

Loope is a Loop Engineering orchestrator: it turns one requirement into a repeatable
multi-agent loop — an optional **design** step, then **implement → review → verify**
repeated until it converges — driving real coding-agent CLIs (Claude, Codex, OpenCode)
with clear roles, artifacts, and gates.

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
- [Interactive TUI](#interactive-tui)
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
It needs Rust/Cargo ([rustup.rs](https://rustup.rs)).

```bash
# one-step install to ~/.cargo/bin (builds release, installs, runs a smoke test)
./install.sh

# or, equivalently
cargo install --path .          # ~/.cargo/bin/loope

# build without installing
cargo build --release           # target/release/loope

# run from source without installing
cargo run -- <command> [args]
```

After installing, make sure `~/.cargo/bin` is on your `PATH`.

The deterministic `--dry-run` path needs no agent binaries or network. Real runs need
the agent CLIs you choose (see [Adapters & providers](#adapters--providers)).

The default build is **std only** (no external crates). The optional interactive
[TUI](#interactive-tui) is the one feature with dependencies — build it with
`--features tui` when you want it.

> **Run inside a project directory**, not a shared/system path like `/tmp` — Loope seeds
> the workspace by copying the current directory, and copying a directory full of other
> users' files fails with a permission error. Point `--workdir` at your project if you
> run from elsewhere.

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
loope show 0001             # print a run's report
loope show 0001 --diff      # report + the unified diffs of what changed
```

By default, in a **git repo**, Loope runs on a **worktree** checked out on a new branch
`loope/<run-id>`, so the results land as a real branch you review and merge — your working
tree is never touched. Outside a git repo (or with `--copy`) it runs on a **copy** instead;
`--in-place` edits your working tree directly. Either way `.loope/` is auto-git-ignored, so
run artifacts never show up as unversioned files.

## Core concepts

**Loop & iterations.** A run is an optional **design** step (once), then a sequence of
**iterations**. Each iteration is:

```text
implement / fix → verify → review (1..N reviewers, only when verify passes)
```

When a verify command is configured, Loope runs **verify first** and runs the reviewers
**only if verify passes** — a failing verify is the actionable signal, so the loop goes
straight to repair instead of spending a reviewer round-trip on a change the tests already
reject (review then vets a passing change for what tests can't catch). With **no** verify
command, review runs every iteration (it is the only gate).

Iteration 1's implementer does the initial implementation; each later iteration is a
**fix** turn whose prompt **leads with the specific failing checks** parsed from the verifier
output (failed test ids + assertion/error lines), followed by any review blockers and a
bounded output tail, asked to resolve them.

```text
requirement → [implement → review → verify] → [fix → review → verify] → … → converged
```

**Convergence & stop reasons.** After each iteration Loope decides whether it is done:

- **Converged** — verification passed and no reviewer reported a blocker. Stops, success.
- **Stopped at the iteration cap** — ran `--max-iters` iterations (default 3) without
  converging. Stops, failure.
- **Stopped: no progress** (`stalled`) — an iteration made no progress (the implementer
  changed nothing, or the verify failure was identical to the previous iteration). Loope
  stops early rather than burn the remaining budget re-attempting a stuck fix.
- **Halted: a step failed** — an agent invocation failed or timed out; retrying a crashed
  agent is not useful, so the loop stops immediately.

The iteration count and stop reason are shown live, in `report.md`, and in `run.json`
(`iterations`, `stop_reason`, `converged`). `--max-iters 1` reproduces a single pass.

**Roles.** `designer`, `implementer`, `reviewer`, `verifier`. Each role maps to an
adapter (overridable). Implementer and designer are write-capable; reviewer and verifier
run read-only where the CLI supports it.

**Adapters.** Data describing how to launch a coding-agent CLI: `claude`, `codex`,
`opencode`, `generic`. See [Adapters & providers](#adapters--providers).

**Verification.** With `--verify-cmd C`, Loope runs `C` in the workspace each iteration
and verification passes iff it exits 0. On failure, a **trimmed tail of the command's
output** (where test failures land) is fed to the next iteration's implementer so it can
fix the actual cause — bounded to keep the prompt small. With no command, verification is
informational (treated as passing) and convergence rests on the reviewers' verdicts.
Reviewers emit a structured verdict (`VERDICT: PASS` / `VERDICT: BLOCK`); any `BLOCK`
keeps the loop iterating with that feedback. Reviewers are told to **block only on
objective defects** — wrong code, compile/test failures, regressions, or an unmet
requirement — and to mark subjective/stylistic improvements as non-blocking `SUGGEST:`
notes, so the loop converges on correctness rather than taste.

**Workspace & artifacts.** Each run gets its own directory under `.loope/runs/<run-id>/`
with a copied working tree and a numbered directory per step recording the exact prompt,
transcript, normalized events, result, and any diff. See
[The run directory](#the-run-directory).

## Commands

### `loope` (no arguments)

Open the interactive [home prompt](#interactive-tui) (requires a `--features tui` build on
a terminal). Type a requirement and press Enter to run the loop; `/` runs a command (see
below). Without the feature or a TTY, prints help.

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
| `--max-iters N` | Cap the implement → review → verify iterations (default `3`; `1` = single pass) |
| `--show-diff` | After the run, print the cumulative diff of everything that changed |
| `--tui` | Watch the run in a full-screen dashboard (needs a `--features tui` build) |
| `--workdir DIR` | Source directory to run against (default: current directory) |
| `--in-place` | Edit the working directory directly instead of a worktree/copy |
| `--copy` | Force a copied workspace instead of a git worktree branch |
| `--branch NAME` | Name the result branch (default `loope/<run-id>`; git repos only) |
| `--no-commit` | Leave the worktree's changes uncommitted on the branch |
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

Print a past run's report. The `<run-id>` may be the full directory name
(`0007-add-jwt-auth`) or any **unique prefix** (`0007`), like a short git hash.

- `--diff` — also print the run's unified diffs (hunked, with a line-number gutter).
- `--color WHEN`.

### `loope apply <run-id>`

Copy a run's changed/added files from its `workspace/` back into the working directory,
so a converged run can be landed in your real tree. It lists what it applied and **never
deletes** anything.

```bash
loope apply 0001                 # apply into the current directory
loope apply 0001 --workdir ./app # apply into a specific tree
```

The applied set is the run's cumulative changed files (`changed-files.txt`). Review the
diff first with `loope show <run-id> --diff` or `loope run --show-diff`.

### `loope doctor`

Self-check the local agent CLIs: prints each of `claude`, `codex`, `opencode` as `ok`
(found on `PATH` or via its `LOOPE_*_BIN` override) or `missing`, with the resolved
program. The home TUI runs the same check on entry.

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

Each run's id is a zero-padded sequence number plus a slug of the requirement —
`0007-add-jwt-auth` — so runs stay ordered and self-describing; commands accept any unique
prefix (`0007`). Everything about a run lives under `.loope/runs/<run-id>/` (gitignored):

```text
.loope/runs/0001-add-login/
  plan.md                 the generated loop plan
  report.md               final loop report (steps grouped by iteration, outcome)
  run.json                machine-readable run record (iterations, stop_reason, branch, base)
  changes.diff            the run's cumulative diff (original source → final workspace)
  changed-files.txt       the cumulative changed-file listing (used by `loope apply`)
  highlight               the "caught & fixed" card (only when a review caught a blocker)
  design-contract.md      the Design Contract (with --design or `loope design`)
  workspace/              the working tree the agents read and edit (a git worktree by
                          default, else a copy)
  agents/                 one numbered directory per step, across all iterations
    01-implementer-claude/
      home/               this step's private CLI config/session dir
      prompt.md           the exact prompt sent
      transcript.jsonl    the captured raw output stream
      events.jsonl        normalized events (actions, messages, reasoning, command
                          output, plans/todos, model, tokens)
      result.md           the parsed result (message + changed files)
      changes.diff        the unified diff this step produced (write steps)
    02-reviewer-codex/...
    03-implementer-claude/   the next iteration's fix turn (its own directory)
    ...
```

### Taking the results

Every run ends by telling you where its output is and how to take it:

- **Worktree run** (default in a git repo): the changes are committed on branch
  `loope/<run-id>`. Review, merge, or discard them with plain git:

  ```bash
  git diff <base>..loope/<run-id>     # review what the loop produced
  git merge loope/<run-id>            # land it (or open a PR from the branch)
  git worktree remove .loope/runs/<run-id>/workspace && git branch -D loope/<run-id>
  ```

  `--no-commit` leaves the changes uncommitted in the worktree; `--branch NAME` overrides the
  branch name. A dirty source tree warns that the branch is cut from `HEAD` (commit/stash
  first, or use `--in-place`).
- **Copy run** (non-git, or `--copy`): the changes sit in the run's `workspace/`. Land them
  with `loope apply <run-id>`.
- **`--in-place`**: the changes are already in your working tree.

`loope show <run-id>` reprints the same guidance for a past run.

Agent directories are numbered by step id and span every iteration, so each iteration's
implement, review, and verify turns keep separate, complete records.

## Terminal UI

On a TTY, `loope run` renders a small visual identity built from Loope's motifs — the `∞`
loop glyph and a `design → implement → review → verify` pipeline tinted by the logo
palette (Claude blue, Codex orange).

While a step runs, Loope streams the agent's activity as a **live activity feed** — file
reads/edits, commands **and their output**, web fetches, sub-agent tasks, the agent's
**reasoning**, **plan/todo checklists**, and short messages — under an **animated status
line** showing a spinner, elapsed time, the last action, and `[n/m]` progress — updating ~10×/s even while
the agent is quiet:

```text
  ▸ 1 implementer · Claude
      ✎ edit   src/lib.rs
      ▸ run    cargo build
      › Added multiply(a, b) with a test.
  ⠹ implementer · Claude   0:42   ▸ run cargo build        [1/4]   ← live, animated
  ✓ 1 implementer · Claude   0:51   src/lib.rs +12 −0              ← committed on finish
```

Between iterations Loope commits an `∞ iteration k/N` header, so the live feed and the
final report both show which iteration each step belongs to. The final summary lists every
step with its gate result, verdict, change stats, and duration, grouped by iteration, plus
the run's total time and stop reason.

- `--no-progress` keeps the committed lines but drops the animation.
- `--quiet` suppresses the per-event feed (keeps step results + summary).
- Color is automatic on a terminal and off when piped or `NO_COLOR` is set; override with
  `--color auto|always|never`. Brand colors downgrade to 256-color when the terminal
  lacks truecolor. Piped/CI output is plain.

## Interactive TUI

Loope ships a full-screen **interactive terminal UI** — the front door when you just run
`loope`, like `claude` or `codex`. It is built on [ratatui](https://ratatui.rs) and gated
behind a `tui` cargo feature; `./install.sh` enables it by default, while the **default
`cargo build` and the `loope` library stay dependency-free** (std only):

```bash
./install.sh                  # installs with the TUI
./install.sh --no-tui         # minimal std-only build (no TUI)
cargo run --features tui      # from source
```

Every screen shows a **workspace line** — `📁 <project path>  ⎇ <git branch>` (and the
worktree name when you're in a linked git worktree) — so you always know which checkout a
run belongs to.

Three entry points:

```bash
loope                      # the home prompt: type a requirement, run it, browse, repeat
loope tui                  # just browse .loope/runs interactively
loope run --tui "..."      # run one specific requirement full-screen
```

- **Home** (`loope`) — a prompt: type what you want built and press **Enter**. Loope runs
  the loop (Claude implements, Codex reviews, up to 3 iterations) live, then shows the
  result; type the next requirement to go again. `Tab` browses past runs. On entry it
  **self-checks the agent CLIs** (Claude / Codex / OpenCode) and shows each as `✓`
  installed or `✗` missing; `/doctor` re-checks. (`loope doctor` does the same from the
  shell.)
- **Browser** (`loope tui`) — a slim run list on the left; the selected run's steps grouped
  by iteration on the right (sized to their content), and a **preview pane** that takes the
  rest of the height to show the focused step's result, its diff, or its transcript as fully
  as possible. When launched from `loope`, a **persistent prompt** sits at the bottom: press
  `i` to type a new requirement (or `/command`) and run it without returning home — type,
  watch, browse, type again. A long requirement **wraps** and the box **grows** as you type
  (nothing scrolls off the side).
- **Live** (`loope run --tui`) — the same layout with full `run` flags, updating as the
  loop runs: the active step streams the agent's **data flow** — each file read/edit, each
  command, the model in use, assistant messages, and a running token count — so you can
  see exactly what Claude and Codex are doing. Press **Esc** to **stop** the run — it
  halts at the next step boundary (the in-flight agent call finishes first, then the loop
  stops with "stopped by user"). When the run converges it settles into the browser, where
  `a` replays any finished step's recorded stream.

On the **home** screen: type to edit the requirement, **Enter** to run it, **Tab** to
browse past runs, **Esc** to quit. **Ctrl+V** pastes an image from the clipboard (macOS) —
it is copied into the run's workspace and referenced in the prompt so Claude/Codex open it
with their file tools (or pastes clipboard text when there's no image). In the
**browser / live** views:

| Key | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | move selection |
| `→`/`l`, `Enter` | open / focus the detail pane |
| `←`/`h`, `Esc` | back / focus the run list |
| `Tab` | switch pane |
| `a` | toggle the agent activity stream (actions, output, reasoning, plans, messages, model, tokens) |
| `d` / `t` | toggle the diff / transcript preview |
| `g` / `G` | top / bottom |
| `PgUp` / `PgDn` | scroll the preview |
| `r` | refresh the run list |
| `?` | help overlay |
| `q` / `Ctrl-C` | quit |

### Slash commands (home prompt)

In the home prompt, start a line with `/` to run a command instead of a requirement. A
palette of matching commands appears (↑/↓ select, `Tab` completes, `Enter` runs, `Esc`
leaves command mode). The status line above the prompt always shows the current run
configuration.

| Command | Effect |
| --- | --- |
| `/iters N` | set the iteration cap (alias `/max-iters`) |
| `/preset NAME` | set adapters from a preset (`claude-codex`, `dual-review`, …) |
| `/implementer A` | set the implementer adapter |
| `/reviewers A[,B]` | set the reviewer adapter(s) |
| `/verify CMD` | set the verifier command (no argument clears it) |
| `/design` | toggle the design-contract step |
| `/dry` | toggle stub agents (no real CLIs) |
| `/apply` | copy the selected run's changes into the working tree |
| `/browse` | open the run browser |
| `/doctor` | re-check the local agent CLIs |
| `/help` | keys & command overlay |
| `/quit` | quit |

Settings commands change the **next** run you launch; e.g. `/iters 5` then typing a
requirement runs up to five iterations. The configuration mirrors the `loope run` flags.

Both TUI commands require an interactive terminal. On a build **without** the `tui`
feature they print a hint and exit `2`; `loope run` without `--tui` is unchanged (the
streaming print feed).

## Loope Desktop (graphical app)

Loope also has a graphical desktop app in the **Liquid Glass** style — a multi-agent hub
that presents the loop's plan and the agents' execution content visually. It is the TUI's
capabilities re-expressed as glass, over the same `loope::hub` core.

What it does:

- **Multi-agent switcher** — the agent CLIs (Claude / Codex / OpenCode) shown with a brand
  icon, availability ✓/✗, version, and an install hint when missing.
- **Live runs** — type a requirement in the command bar and press **Enter**; the loop's plan
  (an implement → review → verify pipeline) sits over a single scrollable transcript that
  streams typed **cells** live — shell commands and their output, file diffs, assistant
  markdown, reasoning, plan/todo checklists, and notices. **Esc** stops the run (cooperatively, at the next step
  boundary). A "caught & fixed" hero card appears when a reviewer's block is fixed later.
- **Projects & sessions** — runs grouped by project; double-click a run to rename it, the
  **+** on PROJECTS registers a directory, and **Shift+Enter** runs a full-text search across
  past runs.
- **Run settings & presets** — the **⚙** popover edits the run options (implementer,
  reviewers, iterations, verify command, design step, dry-run) and saves named presets.
- **Dark / light** — the **☾/☀** toggle switches themes (remembered across launches).

It lives in [`src-tauri/`](../../src-tauri/) as a **separate, independently packaged** Tauri
app: it has its own dependency tree and is excluded from the `loope` workspace, so the
std-only core keeps `deps = 1` and the TUI and desktop app are built/deployed separately.
Build and run:

```bash
cargo install tauri-cli --version '^2'
cd src-tauri && cargo tauri dev
```

Full build/run notes are in [`src-tauri/README.md`](../../src-tauri/README.md); the design is
specified in [the desktop hub spec](../specs/2026-06-29-loope-desktop-hub-spec.md) and
[the Liquid Glass design spec](../specs/2026-06-29-loope-liquid-glass-design-spec.md).

## Convergence highlight

When a reviewer catches a real blocker that a later iteration fixes — the loop's whole
point — `loope run` and `loope show` lead with a **highlight card**, and the TUI shows it
atop the run detail:

```text
✦ caught & fixed
✗ Codex flagged · iter 1   token comparison is not constant-time (timing attack)
✎ Claude fixed · iter 2    src/auth.rs +12 −3
✓ converged · blocker found → fixed
```

It appears only when the review *earned* it (no card for a first-try convergence). Suppress
with `--no-highlight`.

## Diffs & change tracking

Because a CLI cannot reliably self-report what it changed, Loope detects changes by
diffing the workspace before and after. Each write step records `+added −removed` line
stats and a per-step unified diff; the run as a whole records a **cumulative diff**
(original source → final workspace) in `<run>/changes.diff`, summarized in the report as
`Changed: N file(s) +X −Y`.

```bash
loope run --show-diff "..."     # print the cumulative diff right after the run
loope show 0001 --diff      # report + the run's cumulative diff
loope apply 0001            # land those changes into your working tree
```

Diffs render as `@@` hunks with a line-number gutter and `+`/`−` coloring; large diffs
collapse with a `… +N more lines` note.

## Design Contracts

`--design` (in `run`) and the standalone `loope design` command produce a **Design
Contract** — a markdown document covering user flows, UI states, component boundaries,
API/data contracts, and acceptance criteria.

```bash
loope design "Build a settings page for API keys"     # produce + print a contract
loope run --design --verify-cmd "cargo test" "..."    # design-aware loop
```

The contract is saved to `<run>/design-contract.md` and copied into the workspace as
`DESIGN_CONTRACT.md`, and is woven into the implementer, reviewer, and verifier prompts.
When a contract is present, reviewers are told to return `VERDICT: BLOCK` if the change
does not meet the contract's acceptance criteria — so convergence requires the reviewers
to judge the contract satisfied, not merely that a command exited zero.

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
| `0` | Success — the loop **converged** (or a design-only run produced its contract) |
| `1` | Did not converge (hit `--max-iters`), a step failed/timed out, or a run/IO error |
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
