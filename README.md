<div align="center">

<img src="assets/loope-logo.png" alt="Loope" width="460">

**Two AIs. One writes, one reviews. They argue until the code is right — you just hit enter.**

<sub>Loop Engineering orchestrator for collaborative coding agents.</sub>

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg?logo=rust)](https://www.rust-lang.org)
[![Status](https://img.shields.io/badge/status-v1.0%20%C2%B7%20iterative%20loop-brightgreen.svg)](docs/specs/2026-06-28-loope-iterative-loop-spec.md)
[![Std only](https://img.shields.io/badge/deps-std%20only-blue.svg)](Cargo.toml)

</div>

---

Loope is a Rust tool for a new development pattern: **don't ask one agent to do everything.** Put agents into a repeatable loop with clear roles, artifacts, and gates. Run `loope` for an interactive prompt (like `claude` / `codex`), or use it as a scriptable CLI.

## How it works

**The loop actually loops.** A run is an optional design step, then iterations of
implement → review → verify, repeated with feedback until it **converges** (verification
passes and no reviewer blocks) or hits `--max-iters` (default 3).

**Default loop**

```text
requirement
   │
   ▼
 ┌───────────────────────────────────────────────┐
 │  Claude implements / fixes                     │
 │  Codex reviews        →  verifier checks        │ ← one iteration
 └───────────────────────────────────────────────┘
   │
   ├─ converged (tests pass, no blockers)? ── yes ─▶ done ✓
   └─ no ─▶ feed blockers + failures back, iterate again  (≤ --max-iters)
```

**Design-aware loop** (`--design`) adds a one-time contract the whole loop is judged
against:

```text
requirement ─▶ Design Contract (once) ─▶ [ implement → review → verify ] ↻ ─▶ converged ✓
                                          reviewers BLOCK on unmet acceptance criteria
```

## Documentation

The complete usage reference — every command, flag, preset, adapter, the run directory,
the terminal UI, exit codes, environment variables, and troubleshooting — is in
**[docs/guide/usage.md](docs/guide/usage.md)**. See the [docs index](docs/README.md) for
the full documentation layout.

## Quick start

```bash
./install.sh                       # build + install loope (with the interactive TUI)
loope                              # open the prompt: type a requirement, watch it run

# scriptable CLI (no TUI needed):
loope run --dry-run "Add login"    # run the whole loop with stub agents (no CLIs)
loope runs                         # list past runs

# from source without installing:
cargo run --features tui           # the interactive prompt
cargo run -- run --dry-run "Add login"
```

## Commands

```bash
loope plan "Add billing settings"            # print a loop plan
loope plan --design "Build settings page"    # print a design-aware plan
loope design "Build a settings page"         # produce a Design Contract artifact
loope run --dry-run "Add login"              # execute the loop with stub agents
loope run "Add login"                        # execute by driving the real CLIs
loope run --show-diff "Add login"            # …and print the cumulative diff after
loope                                        # open the interactive prompt (needs --features tui)
loope runs                                   # list past runs (e.g. 0007-add-jwt-auth)
loope show 0007                              # print a past run's report (id or unique prefix)
loope apply 0007                             # land a run's changes into your tree
loope tui                                    # interactive run browser (needs --features tui)
loope doctor                                 # self-check the local agent CLIs
loope adapters                               # list supported adapters
```

### `run` flags

| Flag                     | Meaning                                                           |
| ------------------------ | ---------------------------------------------------------------- |
| `--dry-run`              | Execute with deterministic stub agents (no external CLIs/network) |
| `--design`               | Insert a design-contract step before implementation              |
| `--max-iters N`          | Cap the implement → review → verify iterations (default `3`; `1` = single pass) |
| `--show-diff`            | Print the cumulative diff of everything the run changed          |
| `--workdir DIR`          | Source directory to run against (default: current directory)     |
| `--in-place`             | Edit the working directory directly instead of a copied tree     |
| `--approve auto\|manual` | `manual` confirms before launching any agent (default `auto`)    |
| `--preset NAME`          | `claude-codex` \| `codex-claude` \| `claude-solo` \| `dual-review` \| `opencode-codex` |
| `--implementer A`        | Override the implementer adapter (default `claude`)              |
| `--reviewer A`           | Override the reviewer adapter (single, default `codex`)         |
| `--reviewers A,B`        | Run several reviewers in parallel and aggregate their verdicts  |
| `--designer A`           | Override the designer adapter (with `--design`)                 |
| `--verify-cmd C`         | Run shell command `C` as the verifier; gate passes iff it exits 0 |
| `--opencode-model M`     | `provider/model` for OpenCode (or `LOOPE_OPENCODE_MODEL`)        |
| `--timeout SECS`         | Per-step timeout (default 600; `0` disables; or `LOOPE_TIMEOUT`) |
| `--isolate-home`         | Give each agent a private CLI config dir (default: reuse your login) |
| `--color WHEN`           | `auto` (default), `always`, or `never`                          |

OpenCode runs via `opencode run --format json`; it needs a configured, licensed
provider. If its default provider isn't usable for your account, point it at one with
`--opencode-model provider/model`. A provider/auth error surfaces as a failed step (the
loop halts with OpenCode's message) rather than a crash.

## How a run executes

`loope run` turns the plan into a real, inspectable run:

1. A run directory is created under `.loope/runs/<run-id>/`.
2. The working tree is seeded into `workspace/` (a copy by default; `--in-place`
   edits the source directly).
3. An optional design step runs once, then the loop **iterates** implement → review →
   verify. Each step runs through its adapter — each agent in its own private `home/`,
   the reviewer and verifier read-only — and its prompt, transcript, and result are saved.
4. After each iteration Loope checks **convergence** (verification passed and no reviewer
   blocked). If not converged, the next iteration's implementer is fed the blockers and
   verifier failures to fix; the loop repeats until converged or `--max-iters`.
5. A final `report.md`, `run.json`, and a cumulative `changes.diff` are written; `loope
   apply <run-id>` lands the changes in your tree.

### Run directory layout

```text
.loope/runs/0001-add-login/
  plan.md              the generated loop plan
  report.md            final loop report (steps grouped by iteration + outcome)
  run.json             machine-readable run record (iterations, stop_reason, converged)
  changes.diff         the run's cumulative diff (original source → final workspace)
  changed-files.txt    cumulative changed-file listing (used by `loope apply`)
  design-contract.md   the Design Contract (with --design or `loope design`)
  workspace/           the working tree the agents read and edit
  agents/              one numbered directory per step, across all iterations
    01-implementer-claude/{home/, prompt.md, transcript.jsonl, events.jsonl, result.md}
    02-reviewer-codex/{home/, prompt.md, transcript.jsonl, result.md}
    03-implementer-claude/...   the next iteration's fix turn
    ...
```

`.loope/` is gitignored, so runs never pollute version control.

## Review orchestration

The review phase is structured and can fan out across agents:

- **Structured verdicts** — each reviewer ends with `VERDICT: PASS` or
  `VERDICT: BLOCK`, which Loope parses (for Codex, from its `--json` event stream /
  last-message output). Reviewers block **only on objective defects** (wrong code,
  compile/test failures, regressions, unmet requirement) and mark taste/style as
  non-blocking `SUGGEST:` notes — so the loop converges on correctness, not opinion.
  The blocker signal drives convergence: any `BLOCK` keeps the
  loop iterating, feeding that reviewer's findings into the next fix turn.
- **Parallel reviewers** — `--reviewers codex,claude` runs both reviewers
  concurrently on the same change, each in its own workspace directory, and
  aggregates their verdicts (any blocker ⇒ blockers present).
- **Presets** — name a common combination instead of spelling out adapters:

  ```bash
  loope run --preset dual-review --verify-cmd "cargo test" "Add an endpoint"
  ```

  | Preset         | Implementer | Reviewers     |
  | -------------- | ----------- | ------------- |
  | `claude-codex` | claude      | codex         |
  | `codex-claude` | codex       | claude        |
  | `claude-solo`  | claude      | claude        |
  | `dual-review`  | claude      | codex, claude |

## Terminal UI

On a TTY, `loope run` renders a small visual identity built from Loope's own motifs —
the `∞` loop glyph and an `implement → review → verify ↻` pipeline (the `↻` marks that it
repeats to convergence), tinted by the logo's palette (Claude blue, Codex orange).

Each iteration is announced with an `∞ iteration k/N` header. While a step runs, Loope
streams the agent's actions as a **live activity feed** — parsed from each CLI's event
stream — under an animated status line that shows a spinner, the elapsed time, the last
action, and overall progress, updating ~10×/s even while the agent is quiet:

```text
  ∞ iteration 1/3
  ▸ 1 implementer · Claude
      ✎ edit   src/lib.rs
      ▸ run    cargo build
      › Added multiply(a, b) with a test.
  ⠹ implementer · Claude   0:42   ▸ run cargo build        [1/9]   ← live, animated
  ✓ 1 implementer · Claude   0:51   src/lib.rs +12 −0              ← committed on finish
```

The final summary groups steps by iteration and states the stop reason; it and `show`
carry each step's duration and the run's total time.
`--no-progress` keeps the committed lines but drops the animation; colors downgrade to
256-color when the terminal lacks truecolor.

After each write step, the **changed files and `+/−` line stats** are shown live and in
the report; a unified diff is persisted to `agents/<step>/changes.diff`. View it with:

```bash
loope show 0001 --diff      # colored summary + the run's diffs
loope run --quiet "..."         # suppress the live feed; keep step results
```

Color is automatic on a terminal and off when piped or when `NO_COLOR` is set; override
with `--color auto|always|never`. Piped/CI output stays plain markdown.

## Interactive TUI (optional)

`./install.sh` installs Loope with a full-screen **interactive TUI** (built on
[ratatui](https://ratatui.rs)), so plain `loope` drops you into a prompt — like `claude`
or `codex`:

```bash
loope                     # type a requirement → watch it run → browse the result → repeat
loope tui                 # just browse .loope/runs: list ↔ steps ↔ diff/transcript
loope run --tui "..."     # run a specific requirement full-screen
```

Keyboard-first: type your requirement and press **Enter** to launch it; `Tab` browses past
runs, `j/k`/arrows move, `→`/`Enter` drills in, `d`/`t` toggle diff/transcript, `?` for
help, `q`/`Esc` to quit.

On entry the home screen **self-checks the agent CLIs** — Claude / Codex / OpenCode show
as `✓` installed or `✗` missing (also `loope doctor` from the shell).

Type **`/`** at the prompt for commands (à la `claude` / `codex`) — `/iters 5`,
`/preset dual-review`, `/reviewers codex,claude`, `/verify cargo test`, `/design`, `/dry`,
`/apply`, `/browse`, `/doctor`. A palette autocompletes them and a status line shows the
current run configuration.

The TUI lives behind a `tui` cargo feature, so the **default `cargo build` and the `loope`
library stay dependency-free** (std only). `./install.sh --no-tui` builds the minimal CLI;
there, `loope run` works as always and the TUI commands print a hint. See the
[usage guide](docs/guide/usage.md#interactive-tui).

## Desktop app (optional)

Loope also has a graphical desktop app in the **Liquid Glass** style — a multi-agent hub
that presents the loop's plan and the agents' execution content visually:

- a **multi-agent switcher** (detected CLIs with version + install hints),
- **live runs** — type a requirement and watch the pipeline + a scrollable transcript of
  typed cells (exec / diff / markdown / reasoning) stream in; **Esc** stops,
- **projects & sessions** — runs grouped by project, rename, register, full-text search,
- **run settings & presets**, and a **dark / light** toggle.

It lives in [`src-tauri/`](src-tauri/) as a **separate, independently packaged** Tauri app:
its own dependency tree, excluded from the `loope` workspace, so the std-only core keeps
`deps = 1` and the TUI and desktop app are built/deployed separately. Its backend is a thin
layer over the same `loope::hub` core. Build and run:

```bash
cargo install tauri-cli --version '^2'
cd src-tauri && cargo tauri dev
```

See [`src-tauri/README.md`](src-tauri/README.md) for details, the
[desktop hub spec](docs/specs/2026-06-29-loope-desktop-hub-spec.md), and the
[Liquid Glass design spec](docs/specs/2026-06-29-loope-liquid-glass-design-spec.md).

## Supported adapters

| Adapter     | Role                                | Binary (override env)        |
| ----------- | ----------------------------------- | ---------------------------- |
| `claude`    | Implements and fixes across iterations | `claude` (`LOOPE_CLAUDE_BIN`) |
| `codex`     | Reviews code and design consistency | `codex` (`LOOPE_CODEX_BIN`)   |
| `opencode`  | Any role via `opencode run` (needs a provider) | `opencode` (`LOOPE_OPENCODE_BIN`) |
| `generic`   | Fallback for any custom agent       | — (`LOOPE_GENERIC_BIN`)       |

## Verifying the real-CLI path

Automated tests cover the loop through `--dry-run` (no binaries or network). To
exercise the real agents manually:

1. Install and authenticate the agent CLIs you want to use (or point
   `LOOPE_CLAUDE_BIN` / `LOOPE_CODEX_BIN` at compatible binaries). By default Loope
   reuses your normal CLI login; pass `--isolate-home` to give each agent a fresh
   config dir instead.
2. Run the default **Claude + Codex** loop with a real verifier — one command drives
   both agents collaboratively:

   ```bash
   loope run --verify-cmd "cargo test" "Add an add(a, b) function with a test"
   ```

   Claude implements, Codex reviews, and `cargo test` runs in the copied workspace; if
   the tests fail or Codex blocks, Claude gets that feedback and the loop iterates again
   (up to `--max-iters`). Use `--reviewer claude` for a Claude-only loop when Codex is
   unavailable (e.g. quota), and `--approve manual` to confirm before any agent launches.
3. Inspect `.loope/runs/<run-id>/` — each agent's `prompt.md`, `transcript.jsonl`, and
   `result.md`, plus the final `report.md`.

## Architecture

The source is grouped by role into four domains, each layer depending only on the ones
below it:

```text
src/
  model.rs     the loop vocabulary: roles, adapters, the plan and its prompts (pure data)
  adapter.rs   how an agent is invoked + what it emits
    adapter/{event, stub, subprocess}.rs
  engine.rs    running the loop to convergence
    engine/{executor, workspace, review}.rs
  cli.rs       binary-only terminal presentation
    cli/{ui, theme}.rs
```

- **model** — pure vocabulary; no I/O. Everything depends on it.
- **adapter** — the `Invoker` trait and its `stub` (hermetic) and `subprocess` (real CLI)
  implementations, plus the normalized `event` stream they produce.
- **engine** — the `executor` (iterations + convergence), the run `workspace` (copy,
  snapshots, diffs), and `review` verdict parsing.
- **cli** — the `loope` binary's rendering; not part of the library API.

See the [Source Layout Spec](docs/specs/2026-06-29-loope-source-layout-spec.md) for the
rationale.

## SDD artifacts

- [MVP Spec](docs/specs/2026-06-28-loope-mvp-spec.md)
- [Agent Integration Spec](docs/specs/2026-06-28-loope-agent-integration-spec.md)
- [Review Orchestration Spec](docs/specs/2026-06-28-loope-review-orchestration-spec.md)
- [CLI UX Spec](docs/specs/2026-06-28-loope-cli-ux-spec.md)
- [Live Execution Visibility Spec](docs/specs/2026-06-28-loope-live-visibility-spec.md)
- [Live Terminal Rendering Spec](docs/specs/2026-06-28-loope-live-rendering-spec.md)
- [OpenCode Adapter Spec](docs/specs/2026-06-28-loope-opencode-adapter-spec.md)
- [Design Contract Spec](docs/specs/2026-06-28-loope-design-contract-spec.md)
- [Robustness Spec](docs/specs/2026-06-28-loope-robustness-spec.md)
- [Iterative Loop Spec (v1.0)](docs/specs/2026-06-28-loope-iterative-loop-spec.md)
- [Source Layout Spec](docs/specs/2026-06-29-loope-source-layout-spec.md)
- [TUI Spec](docs/specs/2026-06-29-loope-tui-spec.md)
- [TUI Slash Commands Spec](docs/specs/2026-06-29-loope-tui-commands-spec.md)
- [Convergence Highlight Spec](docs/specs/2026-06-29-loope-convergence-highlight-spec.md)
- [Desktop Hub Spec](docs/specs/2026-06-29-loope-desktop-hub-spec.md)
- [Liquid Glass Design Spec](docs/specs/2026-06-29-loope-liquid-glass-design-spec.md)
- [Product Prototype](docs/prototype/2026-06-28-loope-product-prototype.md)
- [MVP Plan](docs/plans/2026-06-28-loope-mvp-plan.md)
- [Agent Integration Plan](docs/plans/2026-06-28-loope-agent-integration-plan.md)
- [Iterative Loop Plan (v1.0)](docs/plans/2026-06-28-loope-iterative-loop-plan.md)
- [Source Layout Plan](docs/plans/2026-06-29-loope-source-layout-plan.md)
- [TUI Plan](docs/plans/2026-06-29-loope-tui-plan.md)
- [TUI Slash Commands Plan](docs/plans/2026-06-29-loope-tui-commands-plan.md)
- [Convergence Highlight Plan](docs/plans/2026-06-29-loope-convergence-highlight-plan.md)
- [Desktop Hub Plan](docs/plans/2026-06-29-loope-desktop-hub-plan.md)

## License

[MIT](LICENSE)
