//! The run-list pane (left): one row per run, newest first.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use super::super::app::{App, Focus};
use super::super::style;
use super::pane_block;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Runs;
    let items: Vec<ListItem> = app
        .runs
        .iter()
        .map(|run| {
            let (mark, color) = if run.converged {
                ("✓", style::PASS)
            } else {
                ("✗", style::FAIL)
            };
            let mut meta = format!("  {} · {} steps", run.stop_reason, run.steps);
            if !run.age.is_empty() {
                meta.push_str(&format!(" · {}", run.age));
            }
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), Style::new().fg(color)),
                Span::raw(run.id.clone()),
                Span::styled(meta, Style::new().fg(style::DIM)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(pane_block("runs", focused))
        .highlight_style(style::selection())
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !app.runs.is_empty() {
        state.select(Some(app.runs_selected));
    }
    frame.render_stateful_widget(list, area, &mut state);

    if app.runs.is_empty() {
        // Hint when there is nothing to browse.
        let hint = ratatui::widgets::Paragraph::new(Line::from(Span::styled(
            "  no runs yet — try `loope run`",
            Style::new().fg(style::DIM),
        )));
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        frame.render_widget(hint, inner);
    }
}
