//! Render the normalized agent event stream (the data flowing from Claude / Codex) as a
//! feed: one icon-tagged line per action, message, model, or token-usage event. Shared by
//! the live "running" pane and the browse "activity" preview.

use loope::adapter::event::{ActionKind, LoopEvent};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::super::style;

/// One event as a styled feed line.
pub fn activity_line(event: &LoopEvent) -> Line<'static> {
    match event {
        LoopEvent::Action { kind, target } => {
            let (icon, color) = glyph(*kind);
            let mut spans = vec![
                Span::styled(format!("  {icon} "), Style::new().fg(color)),
                Span::styled(format!("{:<6}", kind.label()), Style::new().fg(style::DIM)),
            ];
            if !target.trim().is_empty() {
                spans.push(Span::raw(format!(" {}", tail(target, 80))));
            }
            Line::from(spans)
        }
        LoopEvent::Message { text } => Line::from(vec![
            Span::styled("  › ", Style::new().fg(style::BRAND)),
            Span::styled(first_line(text, 100), Style::new().fg(style::DIM)),
        ]),
        LoopEvent::Model { name } => Line::from(Span::styled(
            format!("  ◆ model {name}"),
            Style::new().fg(style::DIM),
        )),
        LoopEvent::Usage {
            input_tokens,
            output_tokens,
        } => Line::from(Span::styled(
            format!("  ∑ {input_tokens}→{output_tokens} tok"),
            Style::new().fg(style::DIM),
        )),
    }
}

/// An icon + accent color per action kind.
fn glyph(kind: ActionKind) -> (&'static str, Color) {
    match kind {
        ActionKind::Read => ("◇", style::DIM),
        ActionKind::Edit | ActionKind::Write => ("✎", style::PASS),
        ActionKind::Command => ("▸", style::BRAND),
        ActionKind::Search => ("⌕", style::DIM),
        ActionKind::Other => ("·", style::DIM),
    }
}

/// Keep the tail of a long target (paths/commands read better right-aligned), by chars.
fn tail(s: &str, max: usize) -> String {
    let s = s.trim();
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().skip(count - (max - 1)).collect();
        format!("…{kept}")
    }
}

/// The first line of `s`, truncated by chars.
fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(max - 1).collect::<String>())
    }
}
