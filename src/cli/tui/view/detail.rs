//! The run-detail pane (right): the steps grouped by iteration on top, and a preview of
//! the selected step (result / diff / transcript) below.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use super::super::app::{App, Focus};
use super::super::model::StepView;
use super::super::style;
use super::{pane_block, preview};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let Some(detail) = &app.detail else {
        frame.render_widget(pane_block("detail", app.focus == Focus::Detail), area);
        return;
    };

    let [steps_area, preview_area] =
        Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);

    render_steps(frame, app, detail, steps_area);
    preview::render(frame, app, detail, preview_area);
}

fn render_steps(frame: &mut Frame, app: &App, detail: &super::super::model::RunDetail, area: Rect) {
    let focused = app.focus == Focus::Detail;
    let mut shown_iter = usize::MAX;
    let items: Vec<ListItem> = detail
        .steps
        .iter()
        .map(|step| {
            let mut lines = Vec::new();
            if step.iteration != shown_iter {
                shown_iter = step.iteration;
                lines.push(iteration_header(step.iteration));
            }
            lines.push(step_line(step));
            ListItem::new(lines)
        })
        .collect();

    let title = format!("detail · {}", detail.id);
    let list = List::new(items)
        .block(pane_block(&title, focused))
        .highlight_style(Style::new().bg(ratatui::style::Color::Rgb(30, 35, 45)))
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !detail.steps.is_empty() {
        state.select(Some(app.detail_selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn iteration_header(iteration: usize) -> Line<'static> {
    let label = if iteration == 0 {
        "── design ──".to_string()
    } else {
        format!("── iteration {iteration} ──")
    };
    Line::from(Span::styled(label, Style::new().fg(style::BRAND)))
}

fn step_line(step: &StepView) -> Line<'static> {
    let (icon, color) = if step.passed {
        ("✓", style::PASS)
    } else {
        ("✗", style::FAIL)
    };
    let mut spans = vec![
        Span::styled(format!("{icon} "), Style::new().fg(color)),
        Span::styled(format!("{} ", step.num), Style::new().fg(style::DIM)),
        Span::raw(format!("{:<11} ", step.role)),
        Span::styled("· ", Style::new().fg(style::DIM)),
        Span::styled(step.adapter.clone(), Style::new().fg(style::adapter_color(&step.adapter))),
    ];
    if !step.gate_result.is_empty() {
        spans.push(Span::styled(
            format!("  {}", step.gate_result),
            Style::new().fg(style::DIM),
        ));
    }
    for change in &step.changes {
        spans.push(Span::styled(format!("  {change}"), Style::new().fg(style::DIM)));
    }
    Line::from(spans)
}
