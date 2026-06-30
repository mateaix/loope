//! The run-detail pane (right): the steps grouped by iteration on top, and a preview of
//! the selected step (result / diff / transcript) below.

use loope::engine::Highlight;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};
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

    // The hero card leads the pane when the review caught & fixed a blocker.
    let body = if let Some(highlight) = &detail.highlight {
        let [card, rest] =
            Layout::vertical([Constraint::Length(5), Constraint::Min(0)]).areas(area);
        render_highlight(frame, highlight, card);
        rest
    } else {
        area
    };

    let [steps_area, preview_area] =
        Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(body);

    render_steps(frame, app, detail, steps_area);
    preview::render(frame, app, detail, preview_area);
}

/// The "caught & fixed" card: ✗ flagged → ✎ fixed → ✓ passed.
fn render_highlight(frame: &mut Frame, highlight: &Highlight, area: Rect) {
    let outcome = if highlight.converged { "converged" } else { "review passed" };
    let changes = highlight.fix_changes.join(", ");
    let lines = vec![
        Line::from(vec![
            Span::styled("✗ ", Style::new().fg(style::FAIL)),
            Span::styled(
                highlight.reviewer.clone(),
                Style::new().fg(style::adapter_color(&highlight.reviewer)),
            ),
            Span::styled(
                format!(" flagged · iter {}   ", highlight.flagged_iter),
                Style::new().fg(style::DIM),
            ),
            Span::raw(highlight.finding.clone()),
        ]),
        Line::from(vec![
            Span::styled("✎ ", Style::new().fg(style::BRAND)),
            Span::styled(
                highlight.implementer.clone(),
                Style::new().fg(style::adapter_color(&highlight.implementer)),
            ),
            Span::styled(
                format!(" fixed · iter {}   ", highlight.fixed_iter),
                Style::new().fg(style::DIM),
            ),
            Span::styled(changes, Style::new().fg(style::DIM)),
        ]),
        Line::from(vec![
            Span::styled(format!("✓ {outcome}"), Style::new().fg(style::PASS)),
            Span::styled("   blocker found → fixed", Style::new().fg(style::DIM)),
        ]),
    ];
    let block = Block::bordered()
        .title(Span::styled(" ✦ caught & fixed ", Style::new().fg(style::BRAND).bold()))
        .border_style(Style::new().fg(style::BRAND));
    frame.render_widget(Paragraph::new(lines).block(block), area);
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
        .highlight_style(style::selection())
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
    if let Some((label, color)) = block_hint(step) {
        spans.push(Span::styled("  · ", Style::new().fg(style::DIM)));
        spans.push(Span::styled(label, Style::new().fg(color).add_modifier(Modifier::BOLD)));
    }
    for change in &step.changes {
        spans.push(Span::styled(format!("  {change}"), Style::new().fg(style::DIM)));
    }
    Line::from(spans)
}

/// A `BLOCK · N blocker(s)` chip for a reviewer step whose verdict blocks — so the row shows
/// at a glance that there's something to fix, without opening the result pane.
fn block_hint(step: &StepView) -> Option<(String, Color)> {
    let verdict = step.verdict.as_deref()?;
    // Blocking verdicts read "BLOCK …" / "blocked …"; "approve (no blockers)" must not match.
    if !verdict.trim().to_ascii_lowercase().starts_with("block") {
        return None;
    }
    let label = match count_blockers(&step.message) {
        0 => "BLOCK".to_string(),
        1 => "BLOCK · 1 blocker".to_string(),
        n => format!("BLOCK · {n} blockers"),
    };
    Some((label, style::FAIL))
}

/// Count the numbered findings in a review message's "Blocking Findings" section (stopping at
/// the next `**…**` header, e.g. SUGGEST).
fn count_blockers(message: &str) -> usize {
    let Some(start) = message.find("Blocking Findings") else {
        return 0;
    };
    let mut count = 0;
    for line in message[start..].lines().skip(1) {
        let t = line.trim_start();
        if count > 0 && t.starts_with("**") {
            break;
        }
        if let Some((num, _)) = t.split_once(". ")
            && !num.is_empty()
            && num.chars().all(|c| c.is_ascii_digit())
        {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reviewer(verdict: Option<&str>, message: &str) -> StepView {
        StepView {
            num: 2,
            role: "reviewer".to_string(),
            adapter: "Codex".to_string(),
            verdict: verdict.map(str::to_string),
            message: message.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn block_hint_counts_findings() {
        let msg = "**Blocking Findings**\n\n1. missing dialog permission\n   continues here\n2. another issue\n\n**SUGGEST:** tidy up\n3. not counted (after section)";
        let (label, _) = block_hint(&reviewer(Some("BLOCK (blockers found)"), msg)).unwrap();
        assert_eq!(label, "BLOCK · 2 blockers");
    }

    #[test]
    fn block_hint_singular_and_unknown_count() {
        let one = block_hint(&reviewer(Some("blocked (needs work)"), "**Blocking Findings**\n\n1. fix it")).unwrap();
        assert_eq!(one.0, "BLOCK · 1 blocker");
        // Verdict blocks but no parseable findings → bare BLOCK.
        let bare = block_hint(&reviewer(Some("BLOCK"), "see above")).unwrap();
        assert_eq!(bare.0, "BLOCK");
    }

    #[test]
    fn no_hint_when_not_blocking() {
        assert!(block_hint(&reviewer(Some("approve (no blockers)"), "all good")).is_none());
        assert!(block_hint(&reviewer(None, "change produced")).is_none());
    }
}
