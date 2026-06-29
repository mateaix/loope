//! The run configuration the home prompt launches with — the interactive equivalent of
//! the `loope run` flags. Slash commands mutate it; it maps to the same `LoopConfig` and
//! invoker the CLI builds.

use std::time::Duration;

use loope::Adapter;
use loope::adapter::{Invoker, StubInvoker, SubprocessInvoker};
use loope::engine::LoopConfig;

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub max_iters: usize,
    pub implementer: Adapter,
    pub reviewers: Vec<Adapter>,
    pub designer: Adapter,
    pub include_design: bool,
    pub verify_command: Option<String>,
    pub dry_run: bool,
}

impl RunOptions {
    pub fn new(dry_run: bool) -> Self {
        Self {
            max_iters: 3,
            implementer: Adapter::Claude,
            reviewers: vec![Adapter::Codex],
            designer: Adapter::Claude,
            include_design: false,
            verify_command: None,
            dry_run,
        }
    }

    /// Build the engine config for one requirement.
    pub fn config(&self, requirement: String) -> LoopConfig {
        LoopConfig {
            requirement,
            include_design: self.include_design,
            designer: self.designer,
            implementer: self.implementer,
            reviewers: self.reviewers.clone(),
            max_iters: self.max_iters,
            verify_command: self.verify_command.clone(),
        }
    }

    /// The invoker for these options (stub for `dry_run`, real CLIs otherwise).
    pub fn make_invoker(&self) -> Box<dyn Invoker + Send + Sync> {
        if self.dry_run {
            Box::new(StubInvoker)
        } else {
            Box::new(SubprocessInvoker {
                isolate_home: false,
                opencode_model: None,
                timeout: Some(Duration::from_secs(600)),
            })
        }
    }

    /// A compact one-line summary for the status line.
    pub fn summary(&self) -> String {
        let reviewers = self
            .reviewers
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join("+");
        let mut out = format!(
            "iters {} · {}→{}",
            self.max_iters,
            self.implementer.as_str(),
            if reviewers.is_empty() { "—" } else { &reviewers }
        );
        if let Some(cmd) = &self.verify_command {
            out.push_str(&format!(" · verify: {cmd}"));
        }
        if self.include_design {
            out.push_str(" · design");
        }
        if self.dry_run {
            out.push_str(" · dry-run");
        }
        out
    }
}
