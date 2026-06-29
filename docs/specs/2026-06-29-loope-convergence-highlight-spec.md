# Loope Convergence Highlight Spec ("the gorgeous moment")

## Background

Loope's one differentiator is the **adversarial second agent**: one model writes, another
reviews, and the loop iterates until the code is right. The single most persuasive event
in a whole run is when the reviewer (e.g. Codex) catches a *real bug the implementer
missed*, the next iteration fixes it, and the run then converges:

```text
✗  Codex flagged it   →   ✎  Claude fixed it   →   ✓  converged
```

Today that arc is **buried** in a flat step list — you have to read iteration 1's
`VERDICT: BLOCK`, scroll to iteration 2's implementer change, and infer the connection.
The product's "aha" is invisible. This spec makes that moment the **hero**: detected
automatically, rendered as a single gorgeous, self-explanatory **highlight card** that a
programmer wants to screenshot and share.

> Slogan it embodies: **Two AIs. One writes, one reviews. They argue until the code is
> right — you just hit enter.**

## Goals

1. **Detect** the convergence highlight from a run: the earliest reviewer **BLOCK** that a
   later iteration **resolved** (a subsequent review `PASS`, or the run converged), plus
   the implementer change that fixed it.
2. **Persist** it with the run so every consumer renders the same thing without
   re-deriving it (write `highlight.md` to the run directory; record presence in
   `run.json`).
3. **Surface it automatically**, leading the output rather than hiding behind a key:
   - `loope run` / `loope show <run>` print a colored **highlight card** at the top of the
     report when a run caught-and-fixed a blocker.
   - The TUI shows the same card as a banner atop the run detail, and on a live run's
     converged frame.
4. **Make it gorgeous and legible at a glance**: three rows (flagged / fixed / passed),
   `✗`→`✎`→`✓`, adapter-colored names (Codex orange, Claude blue), the finding trimmed to
   ~2 lines, the fix's `+/−` stat, and the tagline `blocker found → fixed`.
5. Works in **both** build configs (plain colored card in the std-only CLI; richer card in
   the `tui` feature); no new dependencies; the slogan lands in the README.

## Non-Goals (v1)

- LLM-generated prose ("Codex found a race condition…"): we show the reviewer's own first
  lines, not a new summary. No extra model calls, no extra tokens.
- Image/GIF export: the terminal frame *is* the shareable artifact; we make it
  screenshot-worthy, the user takes the screenshot.
- Highlighting non-blocker improvements, style nits, or runs that converged on the first
  try (no drama, no card — that's correct: the card only appears when the review *earned*
  its keep).
- A list of every resolved blocker — we surface **one** hero (focus), not a digest.

## Core Concepts

### What qualifies (detection)

Over a run's ordered steps (each has: iteration, role, adapter, `passed`/verdict,
message, changes), the highlight is the **first** reviewer step whose verdict is `BLOCK`
in iteration *k* such that the run later improved:

- a reviewer in an iteration *> k* returned `PASS`, **or**
- the run's stop reason is `converged`.

The **fix** is the implementer step of the resolving iteration (*k+1*) — its changed files
and `+/−` stats. The **finding** is the BLOCK reviewer's message, trimmed to its first
~2 non-empty lines.

If no reviewer ever blocked, or a block was never resolved, there is **no highlight**
(the card is simply absent). One run → at most one card.

```text
Highlight {
  reviewer:        "Codex",        // who caught it
  flagged_iter:    1,
  finding:         "multiply(u64::MAX, 2) overflows and panics; use saturating_mul",
  implementer:     "Claude",       // who fixed it
  fixed_iter:      2,
  fix_changes:     ["src/auth.rs +29 -4"],
  converged:       true,
}
```

### Where it's computed and stored

Detection runs once in the engine's `finalize`, which already holds the
`StepOutcome`s (role, adapter, iteration, `gate_passed`, `review_verdict`, `changes`,
`result.message`). It writes a human-readable `highlight.md` into the run directory and
sets a `"highlight":true` marker in `run.json`. Every consumer (CLI `show`, TUI) renders
from that — detection lives in exactly one place and is unit-tested on synthetic
outcomes.

### Rendering — the card

```text
╭─ ✦ caught & fixed ─────────────────────────────────────────╮
│ ✗  Codex flagged · iteration 1                             │
│      multiply(u64::MAX, 2) overflows and panics —          │
│      use saturating_mul                                    │
│ ✎  Claude fixed · iteration 2     src/auth.rs +29 −4       │
│ ✓  converged                          blocker found → fixed │
╰────────────────────────────────────────────────────────────╯
```

- `✗` red, `✎` brand/blue, `✓` green; reviewer/implementer names in their adapter color.
- Plain (no-color / piped) mode prints the same content without styling, so CI logs and
  copy-paste still read well.
- CLI: printed by the report renderer above the run box. TUI: a bordered band at the top
  of the detail pane (and the converged live frame), with `h` to expand/collapse if space
  is tight.

## CLI / TUI Surface

```bash
loope run --verify-cmd "cargo test" "…"   # on convergence-after-block, the card leads the report
loope show 0007                            # the card leads a past run's report
loope show 0007 --no-highlight             # suppress it
```

TUI: the card auto-appears on any run whose detail has a highlight; `h` toggles it.

## Acceptance Criteria

- A run where a reviewer blocks in iteration 1 and the next iteration fixes it and
  converges produces `highlight.md` and `"highlight":true`; `loope show` and the TUI lead
  with the card (flagged → fixed → converged, with the finding and the fix's `+/−`).
- A run that converged on the first iteration (no block) shows **no** card anywhere.
- Plain/piped output renders the card as readable text; colored output renders it gorgeous.
- Detection is pure and unit-tested (block-then-pass, block-never-resolved, no-block,
  multi-reviewer); `cargo clippy` clean and tests green **with and without** `--features
  tui`; default build stays std-only; the README carries the slogan.

## Testing Strategy

- **Detection** (library): synthetic `StepOutcome` sequences — block→fix→converge yields a
  highlight with the right reviewer/iteration/changes; block-unresolved and no-block yield
  `None`; the earliest resolved block wins when several exist.
- **CLI render**: a run dir with a `highlight.md` makes `loope show` print the card; with
  none, it doesn't; `--no-highlight` suppresses it.
- **TUI render** (TestBackend): the detail frame contains the card's labels (`caught &
  fixed`, the reviewer/implementer, `blocker found → fixed`) when a highlight exists.

## Related

- [[2026-06-28-loope-iterative-loop-spec]] — the blocker/convergence machinery this
  dramatizes.
- [[2026-06-28-loope-review-orchestration-spec]] — the verdicts the detection reads.
- [[2026-06-29-loope-tui-spec]] — the browser/live views the card lands in.
