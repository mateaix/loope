# Loope CLI UX Spec (v0.3)

## Background

Loope's loop now executes for real, but the CLI prints plain markdown. This spec
gives the CLI a distinct visual identity built from Loope's own motifs, while staying
safe to pipe and test.

## Brand language

- **Loop glyph:** the infinity mark `∞` stands in for the logo's loop.
- **Agent colors** mirror the logo's two nodes:
  - Claude → blue `#1c9bf0` (the blue node)
  - Codex → orange `#f5a11e` (the orange node)
  - other adapters → neutral / dim
- **Status:** green `✓` pass, red `✗` block, dim `◌`/`…` in-progress.
- Tone: calm and minimal. The loop is the hero; color is an accent, not decoration.

## Surfaces

### Banner

Shown on `--help` / no args and at the start of a `run`:

```text
  ∞ loope   loop engineering
  claude ●  codex ●
```

`∞` and `loope` bold; the legend dots colored by agent.

### Loop pipeline (run start)

A one-line view of the steps about to run:

```text
  ∞  implement → review → revise → verify
```

### Live step progress (run)

Each step prints a running line that resolves in place when it finishes:

```text
  ◌ 2 reviewer    Codex   running…
  ✓ 2 reviewer    Codex   review produced
```

Agent name colored by adapter; icon/color by gate result. Implemented with `\r` +
clear-to-end-of-line, so no extra lines accumulate.

### Run summary

A boxed outcome after the steps, colored by result:

```text
  ╭─ ∞ run-0001 · all gates passed ─╮
  ╰─────────────────────────────────╯
  run dir: .loope/runs/run-0001
```

### `runs`

Each run id prefixed with `∞`, dim run dirs.

## Color gating

- Color is on only when **stdout is a TTY** and `NO_COLOR` is unset.
- `--color auto|always|never` overrides (default `auto`). `always` forces ANSI even
  when piped (for demos); `never` forces plain.
- **Plain fallback is exact:** when color is off, `run` prints the current markdown
  report and `Run directory:` line unchanged, `plan`/`adapters`/`show` are unchanged.
  This keeps every existing automated test valid (tests capture piped, non-TTY output).

## Constraints

- Standard library only — ANSI escapes by hand; `std::io::IsTerminal` for TTY
  detection. No new crates.
- Truecolor (`\x1b[38;2;r;g;bm`); terminals without truecolor degrade gracefully.
- The on-disk `report.md` / `run.json` stay plain (machine-readable); visuals are
  terminal-only.

## Acceptance Criteria

- On a TTY, `loope run` shows the banner, pipeline, live per-step progress, and a
  colored summary box; agents are colored Claude-blue / Codex-orange.
- `loope run --color never ... | cat` and all `cargo test` output are byte-for-byte
  the current plain behavior; the full suite stays green with no changes to assertions.
- `--color always` emits ANSI even when piped.
- No new dependencies; `cargo clippy` clean.
