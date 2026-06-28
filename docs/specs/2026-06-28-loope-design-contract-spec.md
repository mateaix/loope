# Loope Design Contract Spec (v0.8)

## Background

Loope's stated job is to take a requirement and produce its **SDD / Design Contract /
Loop Plan**, then drive the loop. The Loop Plan is real (`loope plan`), but the design
step is a placeholder: `--design` inserts a designer step using the `generic` adapter,
which has no CLI and fails in a real run. The design contract it promises is never
actually produced or used.

This phase makes the design step real: a design agent produces a **Design Contract**
document from the requirement, Loope persists it as a first-class artifact, and the
implementer and reviewer work against it — so frontend/backend/review decisions don't
drift. It also adds a focused `loope design` command that produces a contract on its own.

## Goals

1. The design step runs a **real agent** (default Claude) and produces a Design Contract.
2. The contract is persisted as `design-contract.md` in the run, and placed in the
   workspace so the agents can read it.
3. The implementer implements **against** the contract and the reviewer checks **design
   consistency** with it (the contract is woven into their prompts).
4. A `loope design [--designer A] "<requirement>"` command produces a contract on its
   own (the "generate a Design Contract" entry point), mirroring `loope plan`.
5. Stays hermetic for tests (the stub designer produces a deterministic contract); no
   new crates.

## Non-Goals (v0.8)

- Generating a full multi-document SDD (spec + plan + prototype). The Design Contract is
  the design deliverable; a richer SDD generator is future work.
- Figma / visual-design import, screenshots, or design-token tooling.
- Enforcing the contract mechanically (it is a prompt-level contract the agents honor),
  beyond the existing gates.

## Core Concepts

### The Design Contract

A markdown document the design agent produces from the requirement, covering: user
flows, UI states, component boundaries, API/data contracts, and acceptance criteria
(the existing designer prompt). It is the design agent's final message.

### Persistence

When a design step runs, Loope writes the contract to two places:

- `<run>/design-contract.md` — the first-class run artifact.
- `<run>/workspace/DESIGN_CONTRACT.md` — inside the working tree, so the implementer and
  reviewer (and any real CLI operating in the workspace) can read it directly.

The contract text comes from the design agent's result message; Loope writes the files
itself, so it does not depend on the agent choosing to write a file.

### Feeding the loop

After the design step, the contract is threaded into downstream prompts:

- the implementer is told to implement **against the design contract** (the contract is
  appended to its prompt), and
- the reviewer is told to check **consistency with the design contract**.

This is in addition to the existing upstream-artifact passing (reviewer sees the
implementer result; the revise turn sees the review).

### Default designer

`--design` now defaults the designer adapter to **Claude** (a real agent) instead of
`generic`. `--designer <adapter>` still overrides it.

### Gate

The design step passes when the agent succeeded and produced a non-empty contract; a
failed design agent halts the loop with its error (like any other step).

## CLI Surface

```bash
loope design "Build a settings page for API keys"          # produce a Design Contract
loope design --designer codex --dry-run "Build dashboard"  # choose the designer / stub
loope run --design "Build a settings page for API keys"    # design-aware loop
```

`loope design` runs a design-only plan (one designer step), writes
`design-contract.md` under a run directory, and prints the contract. It honors the
existing run flags (`--workdir`, `--dry-run`, `--color`, `--isolate-home`,
`--opencode-model`).

## Acceptance Criteria

- `loope run --design "<req>"` runs a real design step (default Claude) whose contract
  is saved to `<run>/design-contract.md` and `<run>/workspace/DESIGN_CONTRACT.md`, and
  the implementer's prompt contains the contract.
- `loope design "<req>"` produces and prints a Design Contract and writes
  `design-contract.md`; `--dry-run` makes it hermetic.
- A failed design agent halts the loop with its message; no panic.
- `cargo test` stays green with no binaries or network; `cargo clippy` clean; no new
  crates.

## Testing Strategy

- Unit: a design-aware stub run writes `design-contract.md` and the implementer prompt
  includes the contract text.
- Integration: `loope design --dry-run "..."` writes `design-contract.md` and prints it;
  `loope run --design --dry-run "..."` still passes and carries the contract.
- Manual: a real `loope design` and `loope run --design` with Claude.

## Related

- [[2026-06-28-loope-mvp-spec]] — the design-contract concept this realizes.
- [[2026-06-28-loope-agent-integration-spec]] — the execution model it extends.
