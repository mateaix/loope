//! Rendering: a pure function of [`App`]. The frame is a header, a two-pane body
//! (run list | run detail), and a footer of key hints, with an optional help overlay.

mod activity;
mod detail;
mod home;
mod preview;
mod runs;

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::app::{App, Screen};
use super::style;

/// Draw the whole UI for the current state.
pub fn draw(frame: &mut Frame, app: &App) {
    if app.screen == Screen::Home {
        home::render(frame, app);
        if app.show_help {
            draw_help(frame);
        }
        return;
    }

    let [header, context, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(header_line(app), header);
    frame.render_widget(workspace_line(app), context);

    // A persistent prompt lives below the browser so you can launch a new run without
    // returning to the home screen.
    if app.can_launch && app.screen == Screen::Browse {
        let prompt_h = wrapped_input_height(&app.input, body.width);
        let [main, prompt] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(prompt_h)]).areas(body);
        draw_body(frame, app, main);
        render_prompt(frame, app, prompt);
    } else {
        draw_body(frame, app, body);
    }

    frame.render_widget(footer_line(app), footer);

    if app.show_help {
        draw_help(frame);
    }
}

/// The persistent prompt at the bottom of the browser.
fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == super::app::Focus::Input;
    let label = if app.command_mode() {
        " command ".to_string()
    } else if app.attachments.is_empty() {
        " requirement ".to_string()
    } else {
        format!(" requirement · 📎 {} image(s) ", app.attachments.len())
    };
    let color = if focused { style::BRAND } else { style::DIM };
    let block = Block::bordered()
        .title(Span::styled(label, Style::new().fg(color)))
        .border_style(Style::new().fg(color));

    if !focused && app.input.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "› press i to type a new requirement (or /command)",
                Style::new().fg(style::DIM),
            )))
            .block(block),
            area,
        );
    } else {
        // Wrapping prompt: long input flows onto new lines and the box grows (see the
        // caller's wrapped_input_height); nothing scrolls off the left.
        render_prompt_input(frame, block, area, &app.input, focused);
    }
}

/// The workspace context: project path · git branch · worktree.
pub(super) fn workspace_line(app: &App) -> Line<'static> {
    let mut spans = vec![
        Span::styled(" 📁 ", Style::new().fg(style::DIM)),
        Span::styled(app.project_path.clone(), Style::new().fg(style::DIM)),
    ];
    if let Some(branch) = &app.branch {
        spans.push(Span::styled("  ⎇ ", Style::new().fg(style::DIM)));
        spans.push(Span::styled(branch.clone(), Style::new().fg(style::PASS)));
    }
    if let Some(worktree) = &app.worktree {
        spans.push(Span::styled(
            format!("  ⟂ worktree: {worktree}"),
            Style::new().fg(style::CODEX),
        ));
    }
    Line::from(spans)
}

fn header_line(app: &App) -> Line<'static> {
    let mut spans = vec![Span::styled(" ∞ loope", Style::new().fg(style::BRAND).bold())];

    if app.live {
        let id = app.detail.as_ref().map(|d| d.id.clone()).unwrap_or_default();
        spans.push(Span::styled(format!("  ·  {id}  ·  "), Style::new().fg(style::DIM)));
        if let Some((n, total)) = app.live_iter {
            spans.push(Span::styled(format!("iteration {n}/{total} "), Style::new().fg(style::BRAND)));
        }
        spans.push(Span::styled(app.spinner_char(), Style::new().fg(style::BRAND)));
        if let Some(elapsed) = app.active_elapsed() {
            spans.push(Span::styled(format!("  {elapsed}"), Style::new().fg(style::PASS)));
        }
        if app.stopping {
            spans.push(Span::styled("  stopping…", Style::new().fg(style::FAIL)));
        }
        return Line::from(spans);
    }

    if let Some(run) = app.selected_run() {
        let (mark, color) = if run.converged {
            ("converged", style::PASS)
        } else {
            (run.stop_reason.as_str(), style::FAIL)
        };
        spans.push(Span::styled(format!("  ·  {}  ·  ", run.id), Style::new().fg(style::DIM)));
        spans.push(Span::styled(mark.to_string(), Style::new().fg(color)));
        spans.push(Span::styled(
            format!("  ·  {} iter", run.iterations),
            Style::new().fg(style::DIM),
        ));
    } else {
        spans.push(Span::styled("  ·  browse runs", Style::new().fg(style::DIM)));
    }
    Line::from(spans)
}

fn footer_line(app: &App) -> Line<'static> {
    let hints = if app.live {
        " running… · esc stop · ↑/↓ steps · a activity · d diff · q quit "
    } else if app.focus == super::app::Focus::Input {
        " type a requirement · ctrl+v paste image · / command · enter run · esc cancel "
    } else if app.can_launch {
        " ↑/↓ move · → open · i new requirement · a activity · d diff · ? help · q quit "
    } else {
        " ↑/↓ move · → open · a activity · d diff · t transcript · ? help · q quit "
    };
    Line::from(Span::styled(hints, Style::new().fg(style::DIM)))
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let [list, detail] = Layout::horizontal([
        Constraint::Length(runs_pane_width(area.width)),
        Constraint::Min(0),
    ])
    .areas(area);
    runs::render(frame, app, list);
    detail::render(frame, app, detail);
}

/// Width (columns) for the left runs list: about a quarter of the row, clamped so it never
/// balloons on a wide terminal nor collapses on a narrow one.
pub(super) fn runs_pane_width(total: u16) -> u16 {
    (total.saturating_mul(24) / 100).clamp(24, 42)
}

/// Outer height cap (rows incl. border) for the growing input box.
pub(super) const INPUT_MAX_ROWS: u16 = 8;

/// Display width of a char in terminal cells (wide CJK / fullwidth / emoji = 2).
fn char_width(c: char) -> usize {
    if c == '\t' {
        return 4;
    }
    let u = c as u32;
    let wide = matches!(u,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 |
        0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 |
        0xFFE0..=0xFFE6 | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD);
    if wide { 2 } else { 1 }
}

/// Greedily wrap `text` into rows no wider than `width` cells (character wrap, unicode-aware —
/// no word breaking, so the caret position is exact).
fn wrap_by_width(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut rows = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    for ch in text.chars() {
        let w = char_width(ch);
        if cur_w + w > width && !cur.is_empty() {
            rows.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        cur.push(ch);
        cur_w += w;
    }
    rows.push(cur);
    rows
}

/// Outer height (rows incl. border) the input box needs to show `input` wrapped in a box of
/// `outer_width`, floored at 3 and capped at [`INPUT_MAX_ROWS`] (past which it scrolls).
pub(super) fn wrapped_input_height(input: &str, outer_width: u16) -> u16 {
    let inner_w = outer_width.saturating_sub(2).max(1);
    let rows = wrap_by_width(&format!("› {input}"), inner_w).len().max(1) as u16;
    rows.saturating_add(2).clamp(3, INPUT_MAX_ROWS)
}

/// Render a `› <input>` prompt inside `block` that **wraps** long input onto new lines and
/// grows (the box height is chosen by the caller via [`wrapped_input_height`]); past the cap
/// it scrolls vertically so the caret line stays visible — text never scrolls off the left.
/// Places the cursor only when `focused`.
pub(super) fn render_prompt_input(frame: &mut Frame, block: Block, area: Rect, input: &str, focused: bool) {
    let inner = block.inner(area);
    let rows = wrap_by_width(&format!("› {input}"), inner.width);
    let total = rows.len() as u16;
    // Scroll vertically only when the wrapped text is taller than the (capped) box.
    let scroll_y = total.saturating_sub(inner.height.max(1));
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if i == 0 {
                // Tint the "› " prefix on the first row.
                let rest: String = r.chars().skip(2).collect();
                Line::from(vec![
                    Span::styled("› ", Style::new().fg(style::BRAND)),
                    Span::raw(rest),
                ])
            } else {
                Line::from(Span::raw(r.clone()))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block).scroll((scroll_y, 0)), area);
    if focused {
        let caret_row = total.saturating_sub(1);
        let caret_col = rows.last().map(|r| line_width(r)).unwrap_or(0) as u16;
        let x = inner.x.saturating_add(caret_col.min(inner.width.saturating_sub(1)));
        let y = inner.y.saturating_add(caret_row.saturating_sub(scroll_y));
        frame.set_cursor_position((x, y));
    }
}

/// Display width (cells) of a string.
fn line_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// A bordered pane whose border and title light up when focused.
pub(super) fn pane_block(title: &str, focused: bool) -> Block<'_> {
    let color = if focused { style::BRAND } else { style::DIM };
    Block::bordered()
        .title(Span::styled(format!(" {title} "), Style::new().fg(color)))
        .border_style(Style::new().fg(color))
}

fn draw_help(frame: &mut Frame) {
    let lines = vec![
        Line::from(Span::styled("  Loope TUI — keys", Style::new().fg(style::BRAND).bold())),
        Line::from(""),
        Line::from("  ↑/k ↓/j     move selection · scroll the preview when it's focused"),
        Line::from("  →/l Enter   drill in: runs → steps → preview"),
        Line::from("  ←/h Esc     step back out (preview → steps → list)"),
        Line::from("  Tab         cycle panes (list · steps · preview)"),
        Line::from("  a           toggle the agent activity stream"),
        Line::from("  d / t       open diff / transcript (focuses the preview to scroll)"),
        Line::from("  g / G       top / bottom (of the list or the focused preview)"),
        Line::from("  PgUp/PgDn · wheel   scroll the preview"),
        Line::from("  r           refresh"),
        Line::from("  q / Ctrl-C  quit"),
        Line::from(""),
        Line::from(Span::styled("  press any key to close", Style::new().fg(style::DIM))),
    ];
    let area = centered(frame.area(), 50, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" help ")
                .border_style(Style::new().fg(style::BRAND)),
        ),
        area,
    );
}

/// A centered rectangle `width` × `height` within `area`.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [h] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [v] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(h);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_pane_width_clamps() {
        assert_eq!(runs_pane_width(80), 24); // 19 -> floor 24
        assert_eq!(runs_pane_width(150), 36); // 24%
        assert_eq!(runs_pane_width(400), 42); // 96 -> cap 42
    }

    #[test]
    fn wrapped_input_height_grows_then_caps() {
        assert_eq!(wrapped_input_height("", 40), 3); // empty -> minimum
        assert_eq!(wrapped_input_height("add login", 40), 3); // one row
        assert!(wrapped_input_height(&"x".repeat(120), 40) > 3); // wraps
        assert_eq!(wrapped_input_height(&"x".repeat(1000), 40), INPUT_MAX_ROWS); // capped
    }

    #[test]
    fn wrap_by_width_char_wraps_unicode() {
        assert_eq!(wrap_by_width("abcde", 3), vec!["abc", "de"]);
        assert_eq!(wrap_by_width("中文", 3).len(), 2); // wide chars = 2 cells
    }
}
