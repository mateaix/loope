//! Loope — a Loop Engineering orchestrator.
//!
//! The crate is organized into three domains:
//! - [`model`] — the loop's vocabulary (roles, adapters) and plan generation.
//! - [`adapter`] — how agents are invoked and the events they emit.
//! - [`engine`] — running the loop: the executor, the run workspace, and review verdicts.
//!
//! The domain vocabulary is re-exported at the crate root for convenience.

pub mod adapter;
pub mod engine;
pub mod model;

pub use model::{
    Adapter, LoopOptions, LoopPlan, LoopStep, Role, generate_design_plan, generate_plan,
    list_adapters,
};
pub(crate) use model::prompt_for_step;
