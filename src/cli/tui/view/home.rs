//! The home screen: a prompt to type a requirement, with recent runs to browse.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::super::app::App;
use super::super::style;

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, input, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, header);
    render_body(frame, app, body);
    render_input(frame, app, input);
    render_footer(frame, app, footer);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" ∞ loope", Style::new().fg(style::BRAND).bold()),
        Span::styled("   loop engineering", Style::new().fg(style::DIM)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_body(frame: &mut Frame, app: &App, area: Rect) {
    if app.runs.is_empty() {
        let welcome = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Type what you want built and press Enter.",
                Style::new().fg(style::DIM),
            )),
            Line::from(Span::styled(
                "  Loope drives Claude + Codex in a loop — implement → review → verify —",
                Style::new().fg(style::DIM),
            )),
            Line::from(Span::styled(
                "  iterating until it converges.",
                Style::new().fg(style::DIM),
            )),
        ])
        .wrap(Wrap { trim: true });
        frame.render_widget(welcome, area);
        return;
    }

    let items: Vec<ListItem> = app
        .runs
        .iter()
        .take(area.height.saturating_sub(2) as usize)
        .map(|run| {
            let color = if run.converged { style::PASS } else { style::FAIL };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(run.id.clone(), Style::new().fg(style::DIM)),
                Span::styled(format!("  {}", run.stop_reason), Style::new().fg(color)),
            ]))
        })
        .collect();
    let list = List::new(items).block(
        Block::new().title(Span::styled(" recent runs ", Style::new().fg(style::DIM))),
    );
    frame.render_widget(list, area);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .title(Span::styled(" requirement ", Style::new().fg(style::BRAND)))
        .border_style(Style::new().fg(style::BRAND));
    let inner = block.inner(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", Style::new().fg(style::BRAND)),
            Span::raw(app.input.clone()),
        ]))
        .block(block),
        area,
    );
    // Blinking cursor at the end of the typed text.
    let cursor_x = inner.x + 2 + app.input.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(inner.x + inner.width), inner.y));
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(error) = &app.error {
        Line::from(Span::styled(format!(" {error}"), Style::new().fg(style::FAIL)))
    } else {
        let mut hints = " Enter run".to_string();
        if !app.runs.is_empty() {
            hints.push_str(" · Tab browse runs");
        }
        hints.push_str(" · Esc quit ");
        Line::from(Span::styled(hints, Style::new().fg(style::DIM)))
    };
    frame.render_widget(Paragraph::new(line), area);
}
