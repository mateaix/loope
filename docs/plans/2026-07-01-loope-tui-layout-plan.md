# TUI Layout Refinement Plan

Implementation plan for [[2026-07-01-loope-tui-layout-spec]]. All work is behind the `tui`
feature; the `loope` crate stays `deps = 1`. Each task ships a pure, unit-tested sizing helper
plus its wiring.

## T1 — Narrow, width-aware runs list

- Add `runs_pane_width(total: u16) -> u16` (≈ `total*24/100`, clamped `28..=40`).
- `draw_body`: `horizontal([Length(runs_pane_width(area.width)), Min(0)])`.
- Tests: clamp at narrow / typical / ultra-wide widths.
- **Verify:** `--features tui` builds; render smoke shows a slim list column.

## T2 — Preview-dominant detail split

- Add `steps_pane_height(step_lines: u16, body_height: u16) -> u16` (content-sized, floored
  at 3, capped ≈ 45% of body).
- Compute the rendered step-line count (iteration headers + one per step) and use
  `vertical([Length(steps_pane_height(..)), Min(0)])` for `[steps, preview]`; keep the
  highlight-card split above it.
- Tests: a 2-step run → small steps area; a huge run → capped; a tall terminal → preview
  strictly taller than steps.
- **Verify:** the preview occupies the majority of the pane for typical runs.

## T3 — Wrapping, growing input

- Add `wrapped_input_height(chars: usize, inner_width: u16) -> u16` (rows = ceil(chars/width)
  + 1, clamped `[3, MAX≈8]`... measured in outer rows incl. borders).
- Rewrite `render_prompt_input` to wrap (`Paragraph::wrap`), scroll **vertically** past the
  cap, and place the caret at the wrapped end (drop the horizontal `scroll_x`).
- Callers (`view/mod.rs render_prompt`, `view/home.rs render_input`) allocate the box height
  from `wrapped_input_height(...)` instead of a fixed `Length(3)`.
- Tests: short text → 3 rows; long text → grows then clamps at MAX.
- **Verify:** typing a long requirement wraps and the box grows; nothing scrolls off-left.

## T4 — Verify, docs

- `cargo test` / `cargo clippy` both feature configs; `deps = 1`.
- Update `docs/guide/usage.md` (Terminal UI section) if any key/behavior note changes; note
  the wrapping input. Link the spec + plan from the README/index.

## Related

- Spec: [[2026-07-01-loope-tui-layout-spec]]
