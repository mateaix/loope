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
cargo run -- plan --design "Build dashboard"
cargo run -- adapters
```

## Commands

```bash
loope plan "Add billing settings"
loope plan --design "Build API key settings page"
loope adapters
```

## Supported adapters

| Adapter     | Role                                  |
| ----------- | ------------------------------------- |
| `claude`    | Implements and revises                |
| `codex`     | Reviews code and design consistency   |
| `opencode`  | Alternative implementation backend    |
| `generic`   | Fallback for any custom agent         |

> The MVP only generates deterministic loop plans and prompts. It does not invoke real external tools yet. This keeps the first prototype stable while the adapter contract is still being designed.

## SDD artifacts

- [MVP Spec](docs/specs/2026-06-28-loope-mvp-spec.md)
- [Product Prototype](docs/prototype/2026-06-28-loope-product-prototype.md)
- [Implementation Plan](docs/plans/2026-06-28-loope-mvp-plan.md)

## License

[MIT](LICENSE)
