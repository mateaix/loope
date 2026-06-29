//! The hub: the UI-agnostic brain behind multi-tool management.
//!
//! Everything here is std-only and frontend-agnostic, so the CLI, the TUI, and (later) a
//! graphical front-end all drive the same logic. Two pieces today:
//! - [`registry`] — the agent registry: which coding-agent CLIs exist, what they can do,
//!   and whether they are installed (with a short detection cache).
//! - [`store`] — the local metadata store under `~/.loope/` (app state, session names),
//!   kept separate from each project's `.loope/runs/` artifacts.

pub mod registry;
pub mod store;

pub use registry::{AgentDescriptor, AgentRegistry, Capabilities, Detected, Prober, RealProber};
pub use store::Store;
