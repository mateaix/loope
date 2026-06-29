# Loope Convergence Highlight Implementation Plan

> Detect the "caught & fixed" arc once in the engine, persist it, and render a gorgeous
> card in the CLI and TUI. Default build stays std-only; the card detail is richer under
> `--features tui`. Commit once at the end.

**Spec:** [Convergence Highlight Spec](../specs/2026-06-29-loope-convergence-highlight-spec.md)

---

## Tasks

### Task 1: Detect + persist the highlight (engine)

- [ ] `engine`: a `Highlight` struct (reviewer, flagged_iter, finding, implementer,
      fixed_iter, fix_changes, converged) and `fn detect_highlight(&[StepOutcome],
      stop_reason) -> Option<Highlight>`: earliest reviewer `BLOCK` at iter *k* that a
      later `PASS`/convergence resolved; the implementer change at iter *k+1* is the fix;
      finding = first ~2 lines of the BLOCK message.
- [ ] In `finalize`, when a highlight exists, write `highlight.md` to the run root and add
      `"highlight":true` to `run.json` (`to_run_json`).
- [ ] Unit tests: block→fix→converge; block-unresolved; no-block; multi-reviewer (earliest
      wins).

### Task 2: CLI render (std-only)

- [ ] `cli::ui`: a `print_highlight(card, color)` that renders the bordered card (✗→✎→✓,
      adapter-colored names, finding, `+/−`, `blocker found → fixed`); plain text when
      color is off.
- [ ] `loope show <run>` / the post-run report read `highlight.md` (parse the persisted
      card, or re-read the run via the same struct) and print it **above** the report box;
      `--no-highlight` suppresses it. No card when absent.

### Task 3: TUI render

- [ ] `cli::tui`: render the card as a bordered band atop the run-detail pane when the
      selected run has a highlight; `h` toggles it; the converged live frame shows it.
- [ ] Reuse the engine `Highlight` (loaded from `highlight.md`/run dir) so the TUI and CLI
      show identical content.

### Task 4: Verify + docs

- [ ] Detection unit tests + CLI render test + a TUI `TestBackend` snapshot asserting the
      card labels appear; `cargo clippy` clean and green with and without `--features tui`;
      default build still std-only.
- [ ] `docs/guide/usage.md` (a "Convergence highlight" note) + README (slogan already
      added; mention the card). Link this spec/plan from the README. Commit once.
