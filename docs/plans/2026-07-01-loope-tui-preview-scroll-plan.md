# TUI Preview Scroll Plan

Implementation plan for [[2026-07-01-loope-tui-preview-scroll-spec]]. Behind the `tui`
feature; `loope` stays `deps = 1`. Pure sizing/clamp helpers are unit-tested.

## T1 — Preview focus + real scroll keys

- Add `Focus::Preview`. `set_preview(Diff|Transcript|Activity)` sets `Focus::Preview` (Result
  stays under Detail); `Tab` cycles `Runs → Detail → Preview → Runs`; `Esc`/`Left` from
  Preview → Detail.
- In `update()`, when `focus == Preview`: `Up`/`Down` scroll ±1 line, `PageUp`/`PageDown`
  ±page, `Top`/`Bottom` (`g`/`G`) → 0 / `max_scroll`. In `Detail`, `Up`/`Down` still move
  steps.
- Tests: opening `d` focuses Preview; arrows scroll not step-switch; `Esc` returns to Detail.

## T2 — Bounded scroll + affordance

- Add `preview_max_scroll: Cell<u16>` (interior mutability). `preview::render` sets it to
  `content_rows.saturating_sub(viewport_rows)` (diff = line count; wrapped = wrapped rows for
  the pane width).
- `clamp_scroll(scroll, max)` pure helper; clamp every scroll mutation and on render.
- `scroll_indicator(scroll, max) -> Option<String>` (e.g. ` ↕ 34% `); show it in the preview
  border title when `max > 0`.
- Tests: `clamp_scroll` floors/caps; `G` → max; `scroll_indicator` at top/mid/bottom.

## T3 — Mouse-wheel scroll

- Enable mouse capture in terminal setup/teardown (`EnableMouseCapture` /
  `DisableMouseCapture`).
- Handle `Event::Mouse` in the loop: `ScrollUp`/`ScrollDown` → scroll the preview a few lines
  (bounded via the same clamp).
- Tests: a scroll-down mouse event advances `preview_scroll` (clamped).

## T4 — Verify, docs

- `cargo test` / `cargo clippy` both feature configs; `deps = 1`.
- Update `docs/guide/usage.md` (Interactive TUI keys: preview focus, scroll keys, mouse) and
  the help overlay. Link spec + plan from README/index.
- Reinstall; confirm `d` → diff scrolls with arrows/`j`/`k`/`G` and the wheel.

## Related

- Spec: [[2026-07-01-loope-tui-preview-scroll-spec]]
