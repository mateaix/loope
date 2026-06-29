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

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(header_line(app), header);

    // A persistent prompt lives below the browser so you can launch a new run without
    // returning to the home screen.
    if app.can_launch && app.screen == Screen::Browse {
        let [main, prompt] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(body);
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
    let inner = block.inner(area);

    let body = if !focused && app.input.is_empty() {
        Line::from(Span::styled(
            "› press i to type a new requirement (or /command)",
            Style::new().fg(style::DIM),
        ))
    } else {
        Line::from(vec![
            Span::styled("› ", Style::new().fg(style::BRAND)),
            Span::raw(app.input.clone()),
        ])
    };
    frame.render_widget(Paragraph::new(body).block(block), area);

    if focused {
        let typed = Span::raw(app.input.as_str()).width() as u16;
        let cursor_x = inner.x.saturating_add(2).saturating_add(typed);
        frame.set_cursor_position((cursor_x.min(inner.x + inner.width.saturating_sub(1)), inner.y));
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

fn footer_line(app: &App) -> Line<'static> {
    let hints = if app.live {
        " running… · ↑/↓ steps · a activity · d diff · ? help · q quit "
    } else if app.focus == super::app::Focus::Input {
        " type a requirement · / command · enter run · esc cancel "
    } else if app.can_launch {
        " ↑/↓ move · → open · i new requirement · a activity · d diff · ? help · q quit "
    } else {
        " ↑/↓ move · → open · a activity · d diff · t transcript · ? help · q quit "
    };
    Line::from(Span::styled(hints, Style::new().fg(style::DIM)))
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
        Line::from("  a           toggle the agent activity stream"),
        Line::from("  d / t       toggle diff / transcript"),
        Line::from("  g / G       top / bottom"),
        Line::from("  PgUp/PgDn   scroll preview"),
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
