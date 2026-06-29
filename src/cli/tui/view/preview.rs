//! The preview region under the step list: the selected step's result, its diff, or its
//! raw transcript, scrollable.

use std::fs;
use std::path::PathBuf;

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use super::super::app::{App, Preview};
use super::super::model::{RunDetail, StepView};
use super::super::style;

pub fn render(frame: &mut Frame, app: &App, detail: &RunDetail, area: Rect) {
    // While a step is running, the preview becomes its live activity feed.
    if app.live && app.active.is_some() {
        render_activity(frame, app, area);
        return;
    }

    let (label, text) = match app.preview {
        Preview::Result => ("result", result_text(detail.steps.get(app.detail_selected))),
        Preview::Diff => ("diff", diff_text(detail, app.detail_selected)),
        Preview::Transcript => ("transcript", transcript_text(detail, app.detail_selected)),
    };

    let block = Block::bordered()
        .title(Span::styled(format!(" {label} "), Style::new().fg(style::DIM)))
        .border_style(Style::new().fg(style::DIM));

    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .scroll((app.preview_scroll, 0)),
        area,
    );
}

/// The activity feed of the currently-running step, pinned to the latest lines.
fn render_activity(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(active) = &app.active {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", app.spinner_char()), Style::new().fg(style::BRAND)),
            Span::raw(active.clone()),
        ]));
        lines.push(Line::from(""));
    }
    for entry in &app.activity {
        lines.push(Line::raw(entry.clone()));
    }

    let inner_height = area.height.saturating_sub(2) as usize;
    let scroll = lines.len().saturating_sub(inner_height) as u16;
    let block = Block::bordered()
        .title(Span::styled(" running ", Style::new().fg(style::BRAND)))
        .border_style(Style::new().fg(style::BRAND));
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        area,
    );
}

fn result_text(step: Option<&StepView>) -> Text<'static> {
    let Some(step) = step else {
        return Text::raw("");
    };
    let mut lines = vec![Line::from(vec![
        Span::raw(format!("{} via ", step.role)),
        Span::styled(step.adapter.clone(), Style::new().fg(style::adapter_color(&step.adapter))),
    ])];
    if !step.gate_result.is_empty() {
        lines.push(dim_field("gate", &step.gate_result));
    }
    if let Some(verdict) = &step.verdict {
        lines.push(dim_field("verdict", verdict));
    }
    for change in &step.changes {
        lines.push(dim_field("changed", change));
    }
    if !step.message.is_empty() {
        lines.push(Line::from(""));
        for line in step.message.lines() {
            lines.push(Line::raw(line.to_string()));
        }
    }
    Text::from(lines)
}

fn dim_field(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key}: "), Style::new().fg(style::DIM)),
        Span::raw(value.to_string()),
    ])
}

fn diff_text(detail: &RunDetail, selected: usize) -> Text<'static> {
    let Some(step) = detail.steps.get(selected) else {
        return Text::raw("");
    };
    let path = agent_dir(detail, step).join("changes.diff");
    let raw = fs::read_to_string(&path)
        .ok()
        .filter(|s| !s.trim().is_empty())
        // Reviewer/verifier steps have no diff; fall back to the run's cumulative diff.
        .or_else(|| fs::read_to_string(detail.dir.join("changes.diff")).ok());
    match raw {
        Some(diff) if !diff.trim().is_empty() => color_diff(&diff),
        _ => Text::from(Line::from(Span::styled(
            "(this step produced no diff)",
            Style::new().fg(style::DIM),
        ))),
    }
}

fn transcript_text(detail: &RunDetail, selected: usize) -> Text<'static> {
    let Some(step) = detail.steps.get(selected) else {
        return Text::raw("");
    };
    let path = agent_dir(detail, step).join("transcript.jsonl");
    match fs::read_to_string(&path) {
        Ok(raw) if !raw.trim().is_empty() => {
            Text::from(raw.lines().map(|l| Line::raw(l.to_string())).collect::<Vec<_>>())
        }
        _ => Text::from(Line::from(Span::styled(
            "(no transcript)",
            Style::new().fg(style::DIM),
        ))),
    }
}

/// The per-step artifact directory, e.g. `agents/01-implementer-claude/`.
fn agent_dir(detail: &RunDetail, step: &StepView) -> PathBuf {
    detail.dir.join("agents").join(format!(
        "{:02}-{}-{}",
        step.num,
        step.role,
        step.adapter.to_ascii_lowercase()
    ))
}

/// Color a unified diff: additions green, removals red, hunk headers blue.
fn color_diff(diff: &str) -> Text<'static> {
    let lines = diff
        .lines()
        .map(|line| {
            let color = if line.starts_with("@@") {
                Some(style::BRAND)
            } else if line.starts_with("diff ") || line.starts_with("+++") || line.starts_with("---")
            {
                Some(style::DIM)
            } else if line.starts_with('+') {
                Some(style::PASS)
            } else if line.starts_with('-') {
                Some(style::FAIL)
            } else {
                None
            };
            match color {
                Some(c) => Line::from(Span::styled(line.to_string(), Style::new().fg(c))),
                None => Line::raw(line.to_string()),
            }
        })
        .collect::<Vec<_>>();
    Text::from(lines)
}
