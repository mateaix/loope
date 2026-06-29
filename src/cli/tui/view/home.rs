//! The home screen: a prompt to type a requirement (or a `/` command), a status line of
//! the current run options, recent runs to browse, and a slash-command palette.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::super::app::App;
use super::super::style;

/// A block-letter `LOOPE` wordmark for the splash banner.
const WORDMARK: [&str; 5] = [
    "█     █████ █████ █████ █████",
    "█     █   █ █   █ █   █ █    ",
    "█     █   █ █   █ █████ ████ ",
    "█     █   █ █   █ █     █    ",
    "█████ █████ █████ █     █████",
];

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    // The full splash needs room; fall back to a one-line header on short terminals.
    let big = area.height >= 20;
    let banner_height = if big { 8 } else { 1 };

    let [banner, agents, status, body, input, footer] = Layout::vertical([
        Constraint::Length(banner_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(area);

    if big {
        render_banner(frame, banner);
    } else {
        render_header(frame, banner);
    }
    render_agents(frame, app, agents);
    render_status(frame, app, status);
    if app.command_mode() {
        render_palette(frame, app, body);
    } else {
        render_recent(frame, app, body);
    }
    render_input(frame, app, input);
    render_footer(frame, app, footer);
}

/// The large centered splash: the `LOOPE` wordmark and the `∞` tagline.
fn render_banner(frame: &mut Frame, area: Rect) {
    let brand = Style::new().fg(style::BRAND).bold();
    let mut lines = vec![Line::from("")];
    lines.extend(WORDMARK.iter().map(|row| Line::styled(*row, brand)));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "∞  loop engineering",
        Style::new().fg(style::DIM),
    )));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn render_header(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ∞ loope", Style::new().fg(style::BRAND).bold()),
            Span::styled("   loop engineering", Style::new().fg(style::DIM)),
        ])),
        area,
    );
}

fn render_agents(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled("  agents  ", Style::new().fg(style::DIM))];
    for (i, status) in app.agents.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(style::DIM)));
        }
        let (mark, color) = if status.available {
            ("✓", style::PASS)
        } else {
            ("✗", style::FAIL)
        };
        spans.push(Span::styled(
            format!("{} ", status.adapter.display_name()),
            Style::new().fg(style::adapter_color(status.adapter.as_str())),
        ));
        spans.push(Span::styled(mark, Style::new().fg(color)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  ⚙ {}", app.options.summary()),
            Style::new().fg(style::DIM),
        ))),
        area,
    );
}

fn render_recent(frame: &mut Frame, app: &App, area: Rect) {
    if app.runs.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Type what you want built and press Enter — or / for commands.",
                    Style::new().fg(style::DIM),
                )),
                Line::from(Span::styled(
                    "  Loope drives Claude + Codex in a loop until it converges.",
                    Style::new().fg(style::DIM),
                )),
            ])
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .runs
        .iter()
        .take(area.height.saturating_sub(1) as usize)
        .map(|run| {
            let color = if run.converged { style::PASS } else { style::FAIL };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(run.id.clone(), Style::new().fg(style::DIM)),
                Span::styled(format!("  {}", run.stop_reason), Style::new().fg(color)),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(items)
            .block(Block::new().title(Span::styled(" recent runs ", Style::new().fg(style::DIM)))),
        area,
    );
}

fn render_palette(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .palette()
        .iter()
        .map(|spec| {
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(format!("/{}", spec.name), Style::new().fg(style::BRAND)),
            ];
            if !spec.args.is_empty() {
                spans.push(Span::styled(format!(" {}", spec.args), Style::new().fg(style::DIM)));
            }
            spans.push(Span::styled(format!("   {}", spec.help), Style::new().fg(style::DIM)));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(Block::new().title(Span::styled(" commands ", Style::new().fg(style::BRAND))))
        .highlight_style(style::selection())
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    if !app.palette().is_empty() {
        state.select(Some(app.palette_selected()));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.command_mode() {
        " command ".to_string()
    } else if app.attachments.is_empty() {
        " requirement ".to_string()
    } else {
        format!(" requirement · 📎 {} image(s) ", app.attachments.len())
    };
    let color = style::BRAND;
    let block = Block::bordered()
        .title(Span::styled(title, Style::new().fg(color)))
        .border_style(Style::new().fg(color));
    let inner = block.inner(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", Style::new().fg(style::BRAND)),
            Span::raw(app.input.clone()),
        ]))
        .block(block),
        area,
    );
    // Use the display width (CJK/fullwidth glyphs take two cells), matching exactly how
    // ratatui lays out the text, so the cursor stays on the last typed character.
    let prefix = Span::raw("› ").width() as u16;
    let typed = Span::raw(app.input.as_str()).width().min(u16::MAX as usize) as u16;
    let cursor_x = inner.x.saturating_add(prefix).saturating_add(typed);
    let max_x = inner.x + inner.width.saturating_sub(1);
    frame.set_cursor_position((cursor_x.min(max_x), inner.y));
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(error) = &app.error {
        Line::from(Span::styled(format!(" {error}"), Style::new().fg(style::FAIL)))
    } else if let Some(message) = &app.message {
        Line::from(Span::styled(format!(" {message}"), Style::new().fg(style::BRAND)))
    } else if app.command_mode() {
        Line::from(Span::styled(
            " ↑/↓ select · Tab complete · Enter run · Esc cancel ",
            Style::new().fg(style::DIM),
        ))
    } else {
        let mut hints = " Enter run · / command".to_string();
        if !app.runs.is_empty() {
            hints.push_str(" · Tab browse");
        }
        hints.push_str(" · Esc quit ");
        Line::from(Span::styled(hints, Style::new().fg(style::DIM)))
    };
    frame.render_widget(Paragraph::new(line), area);
}
