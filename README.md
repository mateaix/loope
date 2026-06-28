<div align="center">

<img src="assets/loope-logo.png" alt="Loope" width="460">

**Loop Engineering orchestrator for collaborative coding agents.**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg?logo=rust)](https://www.rust-lang.org)
[![Status](https://img.shields.io/badge/status-MVP-yellow.svg)](docs/specs/2026-06-28-loope-mvp-spec.md)

</div>

---

Loope is a Rust CLI prototype for a new development pattern: **don't ask one agent to do everything.** Put agents into a repeatable loop with clear roles, artifacts, and gates.

## How it works

**The loop actually loops.** A run is an optional design step, then iterations of
implement → review → verify, repeated with feedback until it converges (verification
passes and no reviewer blocks) or hits `--max-iters` (default 3).

**Default loop**

```text
requirement
  -> Claude implements
  -> Codex reviews          ┐
  -> verifier checks        │ repeat (fix → review → verify) with feedback
  -> not converged? Claude fixes the blockers and failures …
  -> converged ✓
```

**Design-aware loop**

```text
requirement
  -> Design Contract (once)
  -> Claude implements against the contract
  -> Codex reviews code + design consistency (blocks on unmet acceptance criteria)
  -> verifier checks against the contract
  -> repeat with feedback until converged ✓
```

## Documentation

The complete usage reference — every command, flag, preset, adapter, the run directory,
the terminal UI, exit codes, environment variables, and troubleshooting — is in
**[docs/guide/usage.md](docs/guide/usage.md)**. See the [docs index](docs/README.md) for
the full documentation layout.

## Quick start

```bash
./install.sh                       # build + install loope to ~/.cargo/bin
loope run --dry-run "Add login"    # run the whole loop with stub agents (no CLIs)

# or, from source without installing:
cargo test
cargo run -- plan "Add login"
cargo run -- run --dry-run "Add login"
cargo run -- adapters
```

## Commands

```bash
loope plan "Add billing settings"            # print a loop plan
loope plan --design "Build settings page"    # print a design-aware plan
loope design "Build a settings page"         # produce a Design Contract artifact
loope run --dry-run "Add login"              # execute the loop with stub agents
loope run "Add login"                        # execute by driving the real CLIs
loope run --show-diff "Add login"            # …and print the cumulative diff after
loope runs                                   # list past runs
loope show run-0001                          # print a past run's report
loope apply run-0001                         # land a run's changes into your tree
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
.loope/runs/run-0001/
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
  last-message output). The blocker signal drives convergence: any `BLOCK` keeps the
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
the `∞` loop glyph and a `design → implement → review → verify` pipeline, tinted by the
logo's palette (Claude blue, Codex orange).

While a step runs, Loope streams the agent's actions as a **live activity feed** —
parsed from each CLI's event stream — under an animated status line that shows a
spinner, the elapsed time, the last action, and overall progress, updating ~10×/s even
while the agent is quiet:

```text
  ▸ 1 implementer · Claude
      ✎ edit   src/lib.rs
      ▸ run    cargo build
      › Added multiply(a, b) with a test.
  ⠹ implementer · Claude   0:42   ▸ run cargo build        [1/4]   ← live, animated
  ✓ 1 implementer · Claude   0:51   src/lib.rs +12 −0              ← committed on finish
```

The final summary and `show` carry each step's duration and the run's total time.
`--no-progress` keeps the committed lines but drops the animation; colors downgrade to
256-color when the terminal lacks truecolor.

After each write step, the **changed files and `+/−` line stats** are shown live and in
the report; a unified diff is persisted to `agents/<step>/changes.diff`. View it with:

```bash
loope show run-0001 --diff      # colored summary + the run's diffs
loope run --quiet "..."         # suppress the live feed; keep step results
```

Color is automatic on a terminal and off when piped or when `NO_COLOR` is set; override
with `--color auto|always|never`. Piped/CI output stays plain markdown.

## Supported adapters

| Adapter     | Role                                | Binary (override env)        |
| ----------- | ----------------------------------- | ---------------------------- |
| `claude`    | Implements and revises              | `claude` (`LOOPE_CLAUDE_BIN`) |
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

   Claude implements and revises, Codex reviews, and `cargo test` runs in the copied
   workspace (the verify gate passes only if it exits 0). Use `--reviewer claude` for
   a Claude-only loop when Codex is unavailable, and `--approve manual` to confirm
   before any agent launches.
3. Inspect `.loope/runs/<run-id>/` — each agent's `prompt.md`, `transcript.jsonl`, and
   `result.md`, plus the final `report.md`.

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
- [Product Prototype](docs/prototype/2026-06-28-loope-product-prototype.md)
- [MVP Plan](docs/plans/2026-06-28-loope-mvp-plan.md)
- [Agent Integration Plan](docs/plans/2026-06-28-loope-agent-integration-plan.md)
- [Iterative Loop Plan (v1.0)](docs/plans/2026-06-28-loope-iterative-loop-plan.md)

## License

[MIT](LICENSE)
