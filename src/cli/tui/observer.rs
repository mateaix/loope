//! Bridges the engine's [`StepObserver`] to the UI thread. The executor runs on a worker
//! thread and pushes `LiveMsg`s over a channel; the engine never sees a ratatui type.

use std::sync::mpsc::Sender;

use loope::LoopStep;
use loope::adapter::event::LoopEvent;
use loope::engine::{StepObserver, StepOutcome};

use super::model::StepView;

/// A live update from the running loop, consumed by the UI thread.
pub enum LiveMsg {
    Iteration { n: usize, total: usize },
    StepStart { role: String, adapter: String },
    Activity(String),
    StepFinish(Box<StepView>),
}

/// A [`StepObserver`] that forwards everything to the UI over a channel.
pub struct TuiObserver {
    tx: Sender<LiveMsg>,
}

impl TuiObserver {
    pub fn new(tx: Sender<LiveMsg>) -> Self {
        Self { tx }
    }
}

impl StepObserver for TuiObserver {
    fn on_iteration_start(&self, n: usize, total: usize) {
        let _ = self.tx.send(LiveMsg::Iteration { n, total });
    }

    fn on_step_start(&self, step: &LoopStep) {
        let _ = self.tx.send(LiveMsg::StepStart {
            role: step.role.as_str().to_string(),
            adapter: step.adapter.display_name().to_string(),
        });
    }

    fn on_event(&self, event: &LoopEvent) {
        if let Some(line) = format_event(event) {
            let _ = self.tx.send(LiveMsg::Activity(line));
        }
    }

    fn on_step_finish(&self, outcome: &StepOutcome) {
        let _ = self.tx.send(LiveMsg::StepFinish(Box::new(step_view(outcome))));
    }
}

/// A short one-line rendering of an event for the activity feed (`None` to skip).
fn format_event(event: &LoopEvent) -> Option<String> {
    match event {
        LoopEvent::Action { kind, target } => {
            let verb = kind.label();
            if target.is_empty() {
                Some(verb.to_string())
            } else {
                Some(format!("{verb} {}", shorten(target)))
            }
        }
        LoopEvent::Message { text } => {
            let line = text.lines().next().unwrap_or("").trim();
            (!line.is_empty()).then(|| format!("› {}", truncate(line, 110)))
        }
        // The model banner and token usage are noise in the live feed.
        LoopEvent::Model { .. } | LoopEvent::Usage { .. } => None,
    }
}

fn shorten(target: &str) -> String {
    // Keep the tail (file paths and commands read better right-aligned).
    truncate_tail(target.trim(), 70)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn truncate_tail(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let tail: String = s.chars().skip(count - (max - 1)).collect();
        format!("…{tail}")
    }
}

fn step_view(outcome: &StepOutcome) -> StepView {
    StepView {
        iteration: outcome.iteration,
        num: outcome.step_id,
        role: outcome.role.as_str().to_string(),
        adapter: outcome.adapter.display_name().to_string(),
        passed: outcome.gate_passed,
        gate_result: outcome.gate_notes.clone(),
        verdict: outcome
            .review_verdict
            .as_ref()
            .map(|v| format!("{} ({})", v.label(), v.summary)),
        message: outcome.result.message.clone(),
        changes: outcome
            .changes
            .iter()
            .map(|c| {
                if c.binary {
                    format!("{} (binary)", c.path)
                } else {
                    format!("{} +{} -{}", c.path, c.added, c.removed)
                }
            })
            .collect(),
    }
}
