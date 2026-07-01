# TUI Layout Refinement Spec

## Background

Driving a real run in the TUI (matecloud RFC review, a wide terminal) surfaced three layout
problems that fight the actual reading task:

1. **The left runs list is too wide.** `draw_body` splits the body `horizontal([Percentage(32),
   Percentage(68)])` (`view/mod.rs`). On a wide terminal 32% is a lot of empty column for a
   short list like `0004  converged · 2 steps · 5m ago`, squeezing the detail + preview.
2. **The top (steps) pane eats the height the core content needs.** `detail::render` splits
   `vertical([Percentage(55), Percentage(45)])` (`view/detail.rs`): the steps list gets 55%,
   the **preview** (the selected step's diff / result — the thing you're actually reading)
   gets 45%. A run has only a handful of steps, so most of that 55% is blank while the diff
   is cramped.
3. **The requirement input doesn't wrap.** `render_prompt_input` **horizontally scrolls** so
   the caret stays visible, which means as you type past the width the start of the text
   scrolls off the left and disappears. Long requirements become unreadable while typing.

## Goals

1. **Narrow, width-aware runs list.** The left pane takes only the width it needs (clamped),
   so the detail + preview gets the rest of the row.
2. **Preview-dominant detail pane.** The steps list is sized to its content (bounded); the
   **preview takes all remaining height** — the core content shows as fully as possible.
3. **Wrapping, growing input.** The requirement box **wraps** long text across lines instead
   of scrolling sideways, and **grows in height** (bounded) so nothing you typed disappears.
4. **No regressions, `deps = 1`, behind the `tui` feature.** Preview scrolling, help, and the
   home screen keep working.

## Non-Goals

- New panes, columns, or navigation.
- Reflowing the diff/preview content itself (it already wraps/scrolls).
- Any change outside the `tui` feature.

## Design

### 1. Runs list width (`view/mod.rs` `draw_body`)

Replace the `Percentage(32)` left column with a **clamped fixed width**: roughly a quarter of
the row, bounded to a sane `[MIN, MAX]` (about `28..=40` columns) so it never balloons on a
wide terminal nor collapses on a narrow one. The detail column takes `Min(0)`.

```text
left  = clamp(area.width * 24 / 100, 28, 40)   // columns
[list, detail] = horizontal([Length(left), Min(0)])
```

A pure `runs_pane_width(total_width) -> u16` helper carries the clamp (unit-tested).

### 2. Preview-dominant detail split (`view/detail.rs`)

Size the steps list to the number of lines it actually renders (iteration headers + one line
per step), **capped** so a huge run can't starve the preview; the preview takes the rest:

```text
steps_h = clamp(rendered_step_lines, 3, cap)   // cap ≈ 45% of body height
[steps_area, preview_area] = vertical([Length(steps_h), Min(0)])
```

For a typical 2–6 step run the steps list shrinks to ~4–10 rows and the **preview gets the
whole remaining pane**. A pure `steps_pane_height(step_lines, body_height) -> u16` helper
carries the sizing (unit-tested). The highlight card split above it is unchanged.

### 3. Wrapping, growing input (`view/mod.rs` `render_prompt_input` + callers)

Replace horizontal scroll with a **wrapping multi-line** input:

- Render the prompt text with `Paragraph::wrap` so it flows onto new lines within the box.
- The box height grows with the wrapped line count, **bounded** to `[3, MAX]` (≈ 8 rows);
  past the cap the input scrolls **vertically** (keeping the caret line visible), never
  sideways.
- The caller computes the needed height from the wrapped line count and the inner width and
  allocates that in its `Layout::vertical` (the browse prompt at `mod.rs`, the home input at
  `view/home.rs`), instead of a fixed `Length(3)`.
- The caret sits at the end of the wrapped text.

A pure `wrapped_input_height(text_len, inner_width) -> u16` helper (chars → rows, clamped)
drives both the caller's layout and the renderer (unit-tested).

## Acceptance Criteria

- On a wide terminal the runs list is a slim column (≤ ~40 cols) and the detail/preview fills
  the rest.
- Selecting a step shows the diff/result in a preview that occupies the majority of the
  detail pane; the steps list no longer leaves large blank space.
- Typing a long requirement wraps within the box and the box grows (up to the cap); earlier
  text stays visible — nothing scrolls off the left.
- `cargo test` / `cargo clippy` green with and without `--features tui`; `deps = 1`.

## Testing Strategy

- **Pure helpers** (unit tests, no terminal): `runs_pane_width` (clamps low/typical/wide),
  `steps_pane_height` (content-sized, capped, floored), `wrapped_input_height` (rows for
  short/long text, clamped to `[3, MAX]`).
- **Render smoke**: existing TUI view tests still pass; a `TestBackend` render of the browse
  screen at a wide size asserts the list column is ≤ MAX and the preview area is taller than
  the steps area.

## Related

- [[2026-06-27-loope-tui-spec]] — the TUI this refines.
- [[2026-06-28-loope-live-rendering-spec]] — the activity/preview panes affected.
