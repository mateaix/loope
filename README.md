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

**Default loop**

```text
requirement
  -> Claude implements
  -> Codex reviews
  -> Claude revises
  -> verifier checks
```

**Design-aware loop**

```text
requirement
  -> Design Contract
  -> Claude implements against the contract
  -> Codex reviews code and design consistency
  -> Claude revises
  -> verifier checks against the contract
```

## Quick start

```bash
cargo test
cargo run -- plan "Add login"
cargo run -- run --dry-run "Add login"
cargo run -- adapters
```

## Commands

```bash
loope plan "Add billing settings"            # print a loop plan
loope plan --design "Build settings page"    # print a design-aware plan
loope run --dry-run "Add login"              # execute the loop with stub agents
loope run "Add login"                        # execute by driving the real CLIs
loope runs                                   # list past runs
loope show run-0001                          # print a past run's report
loope adapters                               # list supported adapters
```

### `run` flags

| Flag                     | Meaning                                                           |
| ------------------------ | ---------------------------------------------------------------- |
| `--dry-run`              | Execute with deterministic stub agents (no external CLIs/network) |
| `--design`               | Insert a design-contract step before implementation              |
| `--workdir DIR`          | Source directory to run against (default: current directory)     |
| `--in-place`             | Edit the working directory directly instead of a copied tree     |
| `--approve auto\|manual` | `manual` confirms before launching any agent (default `auto`)    |
| `--preset NAME`          | `claude-codex` \| `codex-claude` \| `claude-solo` \| `dual-review` |
| `--implementer A`        | Override the implementer adapter (default `claude`)              |
| `--reviewer A`           | Override the reviewer adapter (single, default `codex`)         |
| `--reviewers A,B`        | Run several reviewers in parallel and aggregate their verdicts  |
| `--designer A`           | Override the designer adapter (with `--design`)                 |
| `--verify-cmd C`         | Run shell command `C` as the verifier; gate passes iff it exits 0 |
| `--isolate-home`         | Give each agent a private CLI config dir (default: reuse your login) |
| `--color WHEN`           | `auto` (default), `always`, or `never`                          |

## How a run executes

`loope run` turns the plan into a real, inspectable run:

1. A run directory is created under `.loope/runs/<run-id>/`.
2. The working tree is seeded into `workspace/` (a copy by default; `--in-place`
   edits the source directly).
3. Each step runs through its adapter — each agent in its own private `home/`, the
   reviewer and verifier read-only — and its prompt, transcript, and result are saved.
4. The reviewer is given the implementer's result; the gate is checked after every
   step, and a blocking failure halts the loop.
5. A final `report.md` and `run.json` are written.

### Run directory layout

```text
.loope/runs/run-0001/
  plan.md              the generated loop plan
  report.md            final loop report (per-step status + outcome)
  run.json             machine-readable run record
  workspace/           the working tree the agents read and edit
  agents/
    implementer-claude/{home/, prompt.md, transcript.jsonl, result.md}
    reviewer-codex/{home/, prompt.md, transcript.jsonl, result.md}
    ...
```

`.loope/` is gitignored, so runs never pollute version control.

## Review orchestration

The review phase is structured and can fan out across agents:

- **Structured verdicts** — each reviewer ends with `VERDICT: PASS` or
  `VERDICT: BLOCK`, which Loope parses (for Codex, from its `--json` event stream /
  last-message output). The blocker signal drives the gates: if a review found
  blockers and the revise turn changes nothing, the loop blocks.
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
the `∞` loop glyph, a `design → implement → review → verify` pipeline, live per-step
progress (`running…` resolving in place to `✓`/`✗`), and a colored summary box. Agents
are tinted by the logo's palette: Claude blue, Codex orange.

Color is automatic on a terminal and off when piped or when `NO_COLOR` is set; override
with `--color auto|always|never`. Piped/CI output stays plain markdown.

## Supported adapters

| Adapter     | Role                                | Binary (override env)        |
| ----------- | ----------------------------------- | ---------------------------- |
| `claude`    | Implements and revises              | `claude` (`LOOPE_CLAUDE_BIN`) |
| `codex`     | Reviews code and design consistency | `codex` (`LOOPE_CODEX_BIN`)   |
| `opencode`  | Alternative implementation backend  | `opencode` (`LOOPE_OPENCODE_BIN`) |
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
- [Product Prototype](docs/prototype/2026-06-28-loope-product-prototype.md)
- [MVP Plan](docs/plans/2026-06-28-loope-mvp-plan.md)
- [Agent Integration Plan](docs/plans/2026-06-28-loope-agent-integration-plan.md)

## License

[MIT](LICENSE)
