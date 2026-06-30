//! The preview region under the step list: the selected step's result, its diff, or its
//! raw transcript, scrollable.

use std::fs;
use std::path::PathBuf;

use loope::adapter::event::parse_event_line;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use super::super::app::{App, Preview};
use super::super::model::{RunDetail, StepView};
use super::super::style;
use super::activity::activity_line;

pub fn render(frame: &mut Frame, app: &App, detail: &RunDetail, area: Rect) {
    // While a step is running, the preview becomes its live activity feed.
    if app.live && app.active.is_some() {
        render_running(frame, app, area);
        return;
    }

    let (label, text) = match app.preview {
        Preview::Result => ("result", result_text(detail.steps.get(app.detail_selected))),
        Preview::Diff => ("diff", diff_text(detail, app.detail_selected)),
        Preview::Transcript => ("transcript", transcript_text(detail, app.detail_selected)),
        Preview::Activity => ("activity", activity_text(detail, app.detail_selected)),
    };

    let block = Block::bordered()
        .title(Span::styled(format!(" {label} "), Style::new().fg(style::DIM)))
        .border_style(Style::new().fg(style::DIM));

    let mut paragraph = Paragraph::new(text).block(block).scroll((app.preview_scroll, 0));
    // Wrap prose (result / transcript / activity) so long lines stay visible; never wrap
    // the diff, where columns must line up.
    if app.preview != Preview::Diff {
        paragraph = paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(paragraph, area);
}

/// The live data stream of the currently-running step: a header (model + tokens) over the
/// agent's event feed, pinned to the latest lines.
fn render_running(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(active) = &app.active {
        let mut head = vec![
            Span::styled(format!("{} ", app.spinner_char()), Style::new().fg(style::BRAND)),
            Span::styled(active.clone(), Style::new().fg(style::BRAND).bold()),
        ];
        if let Some(elapsed) = app.active_elapsed() {
            head.push(Span::styled(format!("  · {elapsed}"), Style::new().fg(style::PASS)));
        }
        if let Some(model) = &app.model {
            head.push(Span::styled(format!("  · {model}"), Style::new().fg(style::DIM)));
        }
        if let Some((input, output)) = app.tokens {
            head.push(Span::styled(
                format!("  · {input}→{output} tok"),
                Style::new().fg(style::DIM),
            ));
        }
        lines.push(Line::from(head));
        lines.push(Line::from(""));
    }
    for event in &app.activity {
        lines.push(activity_line(event));
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

/// Browse view: replay a finished step's recorded event stream from `events.jsonl`.
fn activity_text(detail: &RunDetail, selected: usize) -> Text<'static> {
    let Some(step) = detail.steps.get(selected) else {
        return Text::raw("");
    };
    let path = agent_dir(detail, step).join("events.jsonl");
    let lines: Vec<Line> = fs::read_to_string(&path)
        .ok()
        .into_iter()
        .flat_map(|raw| {
            raw.lines()
                .filter_map(parse_event_line)
                .map(|event| activity_line(&event))
                .collect::<Vec<_>>()
        })
        .collect();
    if lines.is_empty() {
        Text::from(Line::from(Span::styled(
            "(no recorded activity)",
            Style::new().fg(style::DIM),
        )))
    } else {
        Text::from(lines)
    }
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
