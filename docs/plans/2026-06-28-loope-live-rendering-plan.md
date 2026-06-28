# Loope Live Terminal Rendering Implementation Plan (v0.6)

> Implement task-by-task with tests first. The animated region is TTY-only; keep the
> Plain path byte-compatible so the suite stays green. Commit once at the end.

**Spec:** [Live Terminal Rendering Spec (v0.6)](../specs/2026-06-28-loope-live-rendering-spec.md)

**Goal:** Make an interactive run feel alive (animated pinned status, elapsed timers,
progress) and scannable (hunked diffs), while CI/pipe output stays plain.

**Architecture:** A `LiveRenderer` owns terminal output during a run and runs a ticker
thread that redraws a small pinned live region (~100 ms). The executor's observer sends
events to it over an `mpsc` channel; finished steps commit to scrollback. A
`ColorLevel` resolves brand colors to truecolor / nearest-256 / none. Diffs render as
hunks. Plain mode is the existing line-streamed path.

**Tech Stack:** Rust 2024, standard library only. No new crates.

---

## Tasks

### Task 1: Color capability + theme tokens

- [ ] `ColorLevel { None, Ansi256, TrueColor }` resolved from `--color`, `NO_COLOR`,
      `FORCE_COLOR`, `COLORTERM`, `TERM`, and `is_terminal`.
- [ ] Semantic theme tokens (accent, agent colors, success, danger, dim) that emit
      truecolor or nearest-256 per level; nearest-256 mapping for the brand RGBs.
- [ ] Unit tests: level resolution from a mocked env; nearest-256 mapping.

### Task 2: Timing + progress on the model

- [ ] Record per-step start/elapsed (pass timing into the observer; std `Instant` is
      fine in the binary). Accumulate total wall-clock.
- [ ] Carry `step n of m` and surface tokens from `Usage` events.
- [ ] Final summary gains per-step durations + total time (+ tokens when present).

### Task 3: LiveRenderer (ticker thread + pinned region)

- [ ] `LiveRenderer` with an `mpsc` receiver and a ticker thread redrawing the live
      region every ~100 ms via cursor-up + clear-to-end + reprint.
- [ ] Message protocol: StepStart, Action, Message, Usage, StepFinish, Stop.
- [ ] Commit finished step lines to scrollback; render the spinner/elapsed/last-action/
      `[n/m]` live line(s); support multiple concurrent reviewer lines.
- [ ] Exclusive stdout during the run; clean teardown clears the live region.

### Task 4: Wire renderer behind the observer

- [ ] A `StepObserver` impl that forwards to the `LiveRenderer` channel (Live mode).
- [ ] `main` selects Live vs Plain from `ColorLevel` + `--no-progress`; Plain keeps the
      current `PrettyObserver`/print path unchanged.
- [ ] Ensure the executor never writes to the terminal in Live mode (renderer owns it).

### Task 5: Hunked diff rendering

- [ ] Parse unified diffs into hunks; render `@@ … @@` header + line-number gutter +
      +/− coloring; collapse hunks beyond a cap with `… +N more lines`.
- [ ] `show --diff` and the run change view use it; honor `ColorLevel`.
- [ ] Unit tests for hunk parsing and rendering.

### Task 6: Verify + docs + real run

- [ ] `cargo test` green, no binaries/network; `cargo clippy` clean; no new crates.
- [ ] Plain output byte-compatible (assert in tests); `--no-progress` disables animation.
- [ ] Real run: animated status region + hunked `show --diff` end to end.
- [ ] README: document the live status region, `--no-progress`, timing summary, and the
      improved diff view.
