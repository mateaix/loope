//! Rendering: a pure function of [`App`]. The frame is a header, a two-pane body
//! (run list | run detail), and a footer of key hints, with an optional help overlay.

mod detail;
mod preview;
mod runs;

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::app::App;
use super::style;

/// Draw the whole UI for the current state.
pub fn draw(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(header_line(app), header);
    draw_body(frame, app, body);
    frame.render_widget(footer_line(), footer);

    if app.show_help {
        draw_help(frame);
    }
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

fn footer_line() -> Line<'static> {
    Line::from(Span::styled(
        " ↑/↓ move · → open · ← back · tab pane · d diff · t transcript · ? help · q quit ",
        Style::new().fg(style::DIM),
    ))
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(32), Constraint::Percentage(68)]).areas(area);
    runs::render(frame, app, list);
    detail::render(frame, app, detail);
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
        Line::from("  ↑/k ↓/j     move selection"),
        Line::from("  →/l Enter   open / focus detail"),
        Line::from("  ←/h Esc     back / focus list"),
        Line::from("  Tab         switch pane"),
        Line::from("  d / t       toggle diff / transcript"),
        Line::from("  g / G       top / bottom"),
        Line::from("  PgUp/PgDn   scroll preview"),
        Line::from("  r           refresh"),
        Line::from("  q / Ctrl-C  quit"),
        Line::from(""),
        Line::from(Span::styled("  press any key to close", Style::new().fg(style::DIM))),
    ];
    let area = centered(frame.area(), 40, lines.len() as u16 + 2);
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
