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

| Flag                  | Meaning                                                           |
| --------------------- | ---------------------------------------------------------------- |
| `--dry-run`           | Execute with deterministic stub agents (no external CLIs/network) |
| `--design`            | Insert a design-contract step before implementation              |
| `--workdir DIR`       | Source directory to run against (default: current directory)     |
| `--in-place`          | Edit the working directory directly instead of a copied tree     |
| `--approve auto\|manual` | `manual` confirms before launching any agent (default `auto`) |

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

1. Install the `claude` and `codex` CLIs and authenticate them (or point
   `LOOPE_CLAUDE_BIN` / `LOOPE_CODEX_BIN` at compatible binaries).
2. From a project directory, run `loope run --approve manual "<requirement>"`.
3. Confirm the prompt, then inspect `.loope/runs/<run-id>/` — each agent's `prompt.md`,
   `transcript.jsonl`, and `result.md`, plus the final `report.md`.

## SDD artifacts

- [MVP Spec](docs/specs/2026-06-28-loope-mvp-spec.md)
- [Agent Integration Spec](docs/specs/2026-06-28-loope-agent-integration-spec.md)
- [Product Prototype](docs/prototype/2026-06-28-loope-product-prototype.md)
- [MVP Plan](docs/plans/2026-06-28-loope-mvp-plan.md)
- [Agent Integration Plan](docs/plans/2026-06-28-loope-agent-integration-plan.md)

## License

[MIT](LICENSE)
