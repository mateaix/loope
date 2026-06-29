//! The loop engine: running a [`crate::model::LoopPlan`]'s requirement to convergence.
//!
//! - [`executor`] orchestrates the iterations (implement → review → verify).
//! - [`workspace`] manages the per-run directory, snapshots, and diffs.
//! - [`review`] interprets reviewer verdicts for the convergence gate.

pub mod executor;
pub mod highlight;
pub mod review;
pub mod workspace;

pub use executor::{
    LoopConfig, LoopRun, StepObserver, StepOutcome, StopReason, execute_loop,
};
pub use highlight::Highlight;
