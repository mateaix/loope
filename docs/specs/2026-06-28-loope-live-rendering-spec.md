# Loope Live Terminal Rendering Spec (v0.6)

## Background

Loope's job is to take a requirement, generate its SDD / Design Contract / Loop Plan,
orchestrate Claude / Codex / OpenCode, drive the **implement → review → revise →
verify** loop, and record every round's artifacts. It is used two ways:

- **Interactively in a terminal**, where the developer watches the loop run.
- **In CI / GitHub Actions / scripts**, where output is a plain log.

v0.5 added a live activity feed, but it is *static-print*: events arrive in bursts and
then the terminal sits silent for the minutes an agent runs, with no spinner, no
elapsed time, and no sense of progress. Diffs are full-context and hard to scan. The
result feels dead and unpolished. This phase makes an interactive run feel **alive and
scannable** while keeping CI output plain.

## Goals

1. A **live status region** pinned below the scrolling activity that animates on a
   timer — spinner, current step (role · adapter), elapsed time, last action, and
   overall progress (step *n* of *m*) — so the terminal feels alive even while the
   agent is quiet.
2. **Per-step elapsed times** and a final **timing summary** (plus tokens when an
   adapter reports them).
3. **Richer diffs**: hunk headers with line ranges, a line-number gutter, +/− coloring,
   and large-diff collapse.
4. **Terminal-capability-aware color**: detect truecolor vs 256-color vs none; honor
   `NO_COLOR` / `FORCE_COLOR` / `--color`; centralize semantic theme tokens.
5. **Clean degradation**: non-TTY / pipe / CI / `--color never` keeps the current
   line-streamed output (no cursor control, no spinner), byte-compatible with today.

## Non-Goals (v0.6)

- A full interactive TUI / REPL / input box. Loope runs a bounded batch loop and exits;
  there is no prompt to keep on screen.
- Alternate screen buffer, virtual scrolling, mouse, or modal dialogs.
- A cell-grid double-buffer. The live region is a small, fixed-height block redrawn in
  place — a full screen diff engine is unnecessary.
- New crates. Everything is hand-written ANSI over `std`.

## Core Concepts

### Render modes

The mode is decided once at startup from the output stream and environment:

- **Live** — stdout is a TTY and color is on: an animated, pinned status region above
  committed scrollback.
- **Plain** — non-TTY, pipe, CI, `NO_COLOR`, or `--color never`: the current
  line-streamed behavior, no cursor control. This path is unchanged and remains the
  contract every automated test depends on.

### Committed scrollback vs the live region

Completed lines — step headers, finished action lines, finished step results — are
**committed**: printed once to scrollback and never rewritten. Directly below them sits
a small fixed-height **live region** showing the in-progress step's animated status. To
update, the renderer moves the cursor up over the live region, clears it, and reprints;
the committed lines above are never touched, so there is no flicker.

### The render loop

A single `LiveRenderer` owns terminal output during a run and runs a **ticker thread**
that redraws the live region roughly every 100 ms. The executor's observer forwards
events to the renderer over a channel (`std::sync::mpsc`): step-start, action, message,
usage, and step-finish. Step-finish commits the step's result line (with its elapsed
time) to scrollback. Because each agent subprocess's stdout is captured (piped), the
renderer has **exclusive** terminal output for the duration of the run; the executor
thread never writes to the terminal directly. When the run ends, the renderer stops and
the final summary is printed normally.

### Live status content

While a step runs:

```text
  ⠹ implement · Claude   0:42   ✎ edit src/lib.rs              [1/4]
```

a braille spinner, role · adapter (agent-colored), elapsed `m:ss`, the most recent
action, and `[n/m]` progress. A parallel reviewer phase shows one animated line per
reviewer.

### Diff rendering

Unified diffs (already persisted to `changes.diff`) are parsed into hunks and rendered
with a `@@ … @@` header, a line-number gutter, and +/− coloring; hunks beyond a cap
collapse to a `… +N more lines` note. Used by `show --diff` and the run's change view.

### Color capability

A `ColorLevel { None, Ansi256, TrueColor }` is resolved from `--color`, `NO_COLOR`,
`FORCE_COLOR`, `COLORTERM`, `TERM`, and whether stdout is a TTY. Brand colors emit
truecolor when available and the nearest 256-color code otherwise; nothing when `None`.
Semantic tokens (accent, the per-agent colors, success, danger, dim) are defined once
and resolved through the active `ColorLevel`.

## CLI Surface (additions)

```bash
loope run --no-progress "..."   # keep committed lines but disable the animated region
```

`--color auto|always|never` continues to apply and now also selects the `ColorLevel`.
`--quiet` (from v0.5) keeps suppressing the per-event feed.

## Acceptance Criteria

- On a TTY, a running step shows an animated spinner + elapsed timer + last action +
  `[n/m]` that updates ~10×/s even while the agent is silent; finished steps stay in
  scrollback with their elapsed time.
- The final summary lists per-step durations and total wall-clock time, plus tokens
  when an adapter reports them.
- `show --diff` renders diffs as hunks with line numbers and +/− colors; large diffs
  collapse.
- A 256-color terminal gets nearest-256 brand colors; `NO_COLOR` / non-TTY get plain;
  `--color` overrides; `--no-progress` disables the animation.
- Non-TTY / pipe / CI output is byte-compatible with today (line-streamed, no cursor
  codes); the whole test suite stays green; std-only; `cargo clippy` clean.

## Testing Strategy

- Unit: spinner frame cycling, elapsed-time formatting, `ColorLevel` resolution from a
  mocked environment, nearest-256 mapping, and diff hunk parsing/rendering.
- The animated region is TTY-only, so tests assert the Plain path is byte-identical to
  today and that the timing/diff/color helpers are correct in isolation.
- Manual: a real run shows the animated status region and hunked diffs end to end.

## Related

- [[2026-06-28-loope-cli-ux-spec]] — the visual identity this extends.
- [[2026-06-28-loope-live-visibility-spec]] — the event stream this animates.
