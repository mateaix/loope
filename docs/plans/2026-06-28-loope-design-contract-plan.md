# Loope Design Contract Implementation Plan (v0.8)

> Implement task-by-task with tests first. Keep the suite hermetic (stub designer
> produces a deterministic contract). Commit once at the end.

**Spec:** [Design Contract Spec (v0.8)](../specs/2026-06-28-loope-design-contract-spec.md)

**Goal:** Make the design step real — produce a Design Contract artifact and feed it
into the loop — plus a `loope design` command.

**Architecture:** Default the designer to Claude; the executor captures the design
step's output as the contract, persists `design-contract.md` (run root + workspace), and
threads it into the implementer/reviewer prompts. A small design-only plan powers the
`loope design` command.

**Tech Stack:** Rust 2024, standard library only. No new crates.

---

## Tasks

### Task 1: Capture + persist the contract (lib)

- [ ] Default `LoopOptions.designer` to `Adapter::Claude`.
- [ ] In the executor, when a Designer step finishes, take its message as the contract,
      write `<run>/design-contract.md` and `<run>/workspace/DESIGN_CONTRACT.md`, and
      track it for downstream prompts.
- [ ] Thread the contract into the implementer prompt ("implement against the design
      contract") and the reviewer prompt ("check design consistency").
- [ ] Unit tests: a design-aware stub run writes the contract and the implementer prompt
      contains it.

### Task 2: `generate_design_plan` + design-only execution (lib)

- [ ] `generate_design_plan(requirement, designer)` → a one-step plan (designer only).
- [ ] Confirm `execute_plan` runs it and writes `design-contract.md` (designer gate
      passes on a non-empty contract).
- [ ] Unit test for the design-only plan shape.

### Task 3: `loope design` command (cli)

- [ ] Add `cmd_design`: parse `--designer`, `--workdir`, `--dry-run`, `--color`,
      `--isolate-home`, `--opencode-model`; build the design-only plan; run it; print the
      contract; report the run directory.
- [ ] Dispatch `design` in `main`; update `print_help`.

### Task 4: Verify + docs + real run

- [ ] `cargo test` green, no binaries/network; `cargo clippy` clean; no new crates.
- [ ] Update the existing design integration test for the new designer adapter/dir and
      the contract artifact.
- [ ] Real run: `loope design` and `loope run --design` with Claude produce a contract.
- [ ] README: `design` command, the contract artifact, and the design-aware loop note.
