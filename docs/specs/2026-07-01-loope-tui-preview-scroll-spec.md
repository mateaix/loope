# TUI Preview Scroll Spec

## Background

Pressing `d` opens the diff preview, but it can't be scrolled — the content below the fold is
unreachable. The current model (`view/preview.rs`, `app.rs`):

- The preview is rendered `Paragraph::…scroll((preview_scroll, 0))`.
- `preview_scroll` is only changed by **`PageUp` / `PageDown`** (`± 10`, **unbounded**).
- There is **no preview focus**: `Up`/`Down`/`j`/`k` move the *step selection* (and reset
  `preview_scroll` to 0), and `g`/`G` move the *list* edge — none of them scroll the preview.

So the only way to scroll a diff is `PageUp`/`PageDown`. On macOS / iTerm (and any terminal
with scrollback) those keys usually drive the **terminal's own scrollback**, never reaching
loope — and many laptops have no dedicated `PageUp`/`PageDown`. Result: the diff looks frozen
and you can't read past the first screen. There is also no bound (you can "scroll" into blank
space) and no indication that more content exists.

## Goals

1. **The preview scrolls with keys that actually arrive** — arrows and `j`/`k`, not just
   `PageUp`/`PageDown`. Pressing `d` (or `t`/`a`) makes the preview immediately scrollable.
2. **Scrolling is bounded** to the content: you can reach the exact bottom and cannot scroll
   past it; `G` jumps to the bottom, `g` to the top.
3. **Discoverable** — a small affordance shows there's more to read and roughly where you are.
4. **Mouse wheel scrolls the preview** too, so it "just works".
5. **No regressions**, `deps = 1`, behind the `tui` feature; step and run navigation still
   work.

## Design

### A preview focus

Add `Focus::Preview`. Opening a scrollable preview (`d` diff / `t` transcript / `a` activity)
**auto-focuses it**, so the very next arrow key scrolls. `Tab` cycles `Runs → Detail →
Preview → Runs`; `Esc`/`Left` from `Preview` returns to `Detail` (where arrows move steps
again). `Result` is short, so it stays under `Detail` (no auto-focus).

In `Focus::Preview`:

| Key | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | scroll one line |
| `PageUp`/`PageDown` | scroll one page |
| `g` / `G` | jump to top / bottom |
| `d`/`t`/`a` | switch preview kind (stays focused); pressing the same again → back to result + Detail |
| `Esc`/`Left`/`Tab` | leave the preview |

### Bounded scroll

`preview::render` knows the content and the viewport, so it computes
`max_scroll = content_rows.saturating_sub(viewport_rows)` and records it on the app through
interior mutability (a `Cell<u16>` `preview_max_scroll`). `update()` clamps every scroll
mutation to `[0, max_scroll]`, and `G` sets `preview_scroll = max_scroll`. Content rows are
the diff's line count (unwrapped — exact for the diff, the failing case) or the wrapped row
count for the wrapped previews (computed with the pane width; a small under-count is
acceptable and never over-scrolls).

Changing step or preview kind resets `preview_scroll` to 0 (already the case).

### Affordance

When `max_scroll > 0`, the preview border title shows a compact indicator — e.g.
`diff  ↕ 34%` (position) or `▾ more` at the bottom — so it's obvious the pane scrolls and
where you are.

### Mouse wheel

Enable mouse capture in the terminal setup and handle `Event::Mouse`: `ScrollUp` /
`ScrollDown` scroll the preview by a few lines (bounded), regardless of focus, when the cursor
is over the preview pane (or unconditionally, for simplicity). Text selection with the mouse
is a known trade-off of mouse capture; keep it scoped and documented.

## Acceptance Criteria

- After pressing `d`, `↓`/`j` and `↑`/`k` scroll the diff a line at a time; `PageUp`/`PageDown`
  a page; `g`/`G` to top/bottom — without touching `PageUp`/`PageDown` on the keyboard.
- Scrolling stops exactly at the last line; `G` shows the true bottom; no scrolling into blank
  space.
- The preview shows a "more / position" affordance when content overflows.
- The mouse wheel scrolls the preview.
- `cargo test` / `cargo clippy` green with and without `--features tui`; `deps = 1`.

## Testing Strategy

- **Pure helpers** (unit tests): `clamp_scroll(scroll, max)`, `scroll_indicator(scroll, max)`
  (percent / more-markers), page/line step math.
- **Update logic** (no terminal): in `Focus::Preview`, `Down`×N then clamp at `max_scroll`;
  `G` → `max_scroll`; `g` → 0; opening `d` sets `Focus::Preview`; `Esc` → `Detail`.
- **Render smoke**: existing view tests pass; a `TestBackend` render of a long diff asserts
  the bottom line is visible after `G`.

## Related

- [[2026-06-27-loope-tui-spec]] — the TUI this fixes.
- [[2026-07-01-loope-tui-layout-spec]] — the preview pane this makes scrollable.
