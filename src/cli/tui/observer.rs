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
    /// One normalized event from the agent's stream (action, message, model, usage).
    Event(LoopEvent),
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
        // Forward the whole stream; the UI decides how to render each event.
        let _ = self.tx.send(LiveMsg::Event(event.clone()));
    }

    fn on_step_finish(&self, outcome: &StepOutcome) {
        let _ = self.tx.send(LiveMsg::StepFinish(Box::new(step_view(outcome))));
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
