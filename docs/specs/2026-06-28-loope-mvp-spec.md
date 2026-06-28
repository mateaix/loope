# Loope MVP Spec

## Requirement Background

Loope is an open-source Rust project for **Loop Engineering**: a development loop where multiple coding agents cooperate with clear roles, artifacts, and gates instead of one agent freely editing everything.

The first target scenario is:

1. A user submits one product/development requirement.
2. Loope asks one tool to implement, for example Claude.
3. Loope asks another tool to review, for example Codex.
4. Loope records the loop plan and expected artifacts so the cycle is repeatable.
5. Later, Loope can insert a design phase so frontend design, backend implementation, and review stay consistent.

## Product Positioning

Loope is not another coding agent. It is a local orchestration layer above Codex, Claude, OpenCode, and similar tools.

One sentence:

> Loope turns a requirement into a repeatable multi-agent engineering loop: design, implement, review, revise, verify.

## MVP Scope

In scope:

- Rust CLI and library.
- Deterministic loop plan generation.
- Built-in roles: `designer`, `implementer`, `reviewer`, `verifier`.
- Built-in tool adapters: `claude`, `codex`, `opencode`, `generic`.
- Default coding loop: Claude implements, Codex reviews, verifier checks.
- Optional design loop: designer produces a design contract before implementation.
- Markdown output suitable for humans and agent prompts.
- Local run directory `.loope/` for future loop artifacts.

Out of scope:

- Actually invoking Claude/Codex/OpenCode subprocesses.
- Parsing real tool streaming output.
- Web UI.
- Figma/visual design integration.
- Git automation and PR creation.

## Core Concepts

### Loop Engineering

A loop is a controlled sequence of agent turns. Each turn has:

- Role.
- Tool adapter.
- Objective.
- Inputs.
- Expected artifact.
- Gate condition.

### Design Contract

For frontend or full-stack work, the design phase emits a design contract before code starts. It defines:

- User flows.
- UI states.
- Component boundaries.
- API/data contracts.
- Acceptance criteria.

The implementation agent must code against this contract, and the review agent checks consistency against it.

## Default Flow

For a normal coding request:

```text
requirement
  -> implement_with_claude
  -> review_with_codex
  -> revise_with_claude
  -> verify
```

For design-aware frontend/full-stack work:

```text
requirement
  -> design_contract
  -> implement_with_claude
  -> review_with_codex
  -> revise_with_claude
  -> verify_against_design
```

## CLI Prototype

Generate a plan:

```bash
loope plan "Add billing settings page"
```

Generate a design-aware plan:

```bash
loope plan --design "Add billing settings page"
```

Show supported adapters:

```bash
loope adapters
```

## Acceptance Criteria

- `cargo test` passes.
- `cargo run -- plan "Add login"` prints a loop containing Claude implementation and Codex review.
- `cargo run -- plan --design "Add dashboard"` includes a design contract step before implementation.
- `cargo run -- adapters` lists `claude`, `codex`, `opencode`, and `generic`.
- The project contains docs, tests, and a README sufficient for migration into a standalone repository.

## Related Knowledge

- [[output/GitHub项目解读/2026-06-08-LiteLLM-Agent-Platform项目解读.md]]
- [[output/GitHub项目解读/2026-06-17-Omnigent项目分析.md]]
- [[output/GitHub项目解读/2026-06-24-OpenKnowledge项目分析.md]]
- [[output/开源项目方向/2026-06-24-开源方向重评估-候选排序.md]]
