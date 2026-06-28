# Loope MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or inline TDD execution to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI prototype that turns one requirement into a deterministic multi-agent Loop Engineering plan.

**Architecture:** The Rust library owns the domain model: roles, adapters, loop steps, and markdown rendering. The CLI is intentionally thin and only parses `plan`, `plan --design`, and `adapters`.

**Tech Stack:** Rust 2024 edition, standard library only, Cargo tests.

---

## Tasks

### Task 1: SDD Artifacts

- [x] Write `docs/specs/2026-06-28-loope-mvp-spec.md`.
- [x] Write `docs/prototype/2026-06-28-loope-product-prototype.md`.
- [x] Write this implementation plan.

### Task 2: Tests First

- [x] Add integration tests for default loop planning, design-aware planning, adapter listing, and CLI output.
- [x] Run `cargo test` and confirm the tests fail before implementation.

### Task 3: Library

- [x] Implement roles and adapters.
- [x] Implement deterministic loop plan generation.
- [x] Implement markdown rendering.

### Task 4: CLI

- [x] Implement `loope plan`.
- [x] Implement `loope plan --design`.
- [x] Implement `loope adapters`.

### Task 5: Verification

- [x] Run `cargo test`.
- [x] Run `cargo run -- plan "Add login"`.
- [x] Run `cargo run -- plan --design "Add dashboard"`.
- [x] Run `cargo run -- adapters`.
