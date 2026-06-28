# Loope

Loop Engineering orchestrator for collaborative coding agents.

Loope is a Rust CLI prototype for a new development pattern: do not ask one agent to do everything. Put agents into a repeatable loop with clear roles, artifacts, and gates.

Default loop:

```text
requirement
  -> Claude implements
  -> Codex reviews
  -> Claude revises
  -> verifier checks
```

Design-aware loop:

```text
requirement
  -> Design Contract
  -> Claude implements against the contract
  -> Codex reviews code and design consistency
  -> Claude revises
  -> verifier checks against the contract
```

## Quick Start

```bash
cargo test
cargo run -- plan "Add login"
cargo run -- plan --design "Build dashboard"
cargo run -- adapters
```

## MVP Commands

```bash
loope plan "Add billing settings"
loope plan --design "Build API key settings page"
loope adapters
```

## Supported Adapters

- `claude`
- `codex`
- `opencode`
- `generic`

The MVP only generates deterministic loop plans and prompts. It does not invoke real external tools yet. This keeps the first prototype stable while the adapter contract is still being designed.

## SDD Artifacts

- [MVP Spec](docs/specs/2026-06-28-loope-mvp-spec.md)
- [Product Prototype](docs/prototype/2026-06-28-loope-product-prototype.md)
- [Implementation Plan](docs/plans/2026-06-28-loope-mvp-plan.md)

## License

Apache-2.0
