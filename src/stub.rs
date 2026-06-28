//! Deterministic stub invoker used by `--dry-run` and all automated tests.
//!
//! It produces role-aware output with no randomness, no clock, and no I/O beyond
//! the workspace it is handed. This lets the whole loop run end-to-end in CI without
//! the real `claude` / `codex` binaries or a network.

use std::fs;

use crate::Role;
use crate::adapter::{AgentInvocation, InvocationResult, Invoker};

/// An [`Invoker`] that simulates each role deterministically.
pub struct StubInvoker;

impl Invoker for StubInvoker {
    fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
        match inv.role {
            Role::Designer => InvocationResult {
                success: true,
                message: "Design contract (stub): user flows, UI states, component \
                          boundaries, API/data contracts, and acceptance criteria recorded."
                    .to_string(),
                changed_files: vec!["DESIGN_CONTRACT.md".to_string()],
                transcript: stub_transcript(inv),
            },
            Role::Implementer => {
                // Simulate a real change so the reviewer has something to read.
                let note = inv.workspace_dir.join("IMPLEMENTATION_NOTES.md");
                let _ = fs::write(
                    &note,
                    format!(
                        "# Implementation notes (stub)\n\nThis turn was handled by {}.\n",
                        inv.adapter.display_name()
                    ),
                );
                InvocationResult {
                    success: true,
                    message: format!(
                        "{} applied a scoped stub change for the requirement.",
                        inv.adapter.display_name()
                    ),
                    changed_files: vec!["IMPLEMENTATION_NOTES.md".to_string()],
                    transcript: stub_transcript(inv),
                }
            }
            Role::Reviewer => InvocationResult {
                success: true,
                message: "Review (stub): the change is scoped to the requirement and \
                          consistent with the available artifacts.\nVERDICT: PASS"
                    .to_string(),
                changed_files: Vec::new(),
                transcript: stub_transcript(inv),
            },
            Role::Verifier => InvocationResult {
                success: true,
                message: "Verification (stub): checks simulated OK; no unresolved risks recorded."
                    .to_string(),
                changed_files: Vec::new(),
                transcript: stub_transcript(inv),
            },
        }
    }
}

/// One deterministic JSON line standing in for a captured output stream.
fn stub_transcript(inv: &AgentInvocation) -> String {
    format!(
        "{{\"role\":\"{}\",\"adapter\":\"{}\",\"mode\":\"stub\"}}\n",
        inv.role.as_str(),
        inv.adapter.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Adapter;
    use std::path::PathBuf;

    fn invocation(role: Role, adapter: Adapter, workspace: PathBuf) -> AgentInvocation {
        AgentInvocation {
            adapter,
            role,
            prompt: "do the thing".to_string(),
            workspace_dir: workspace,
            home_dir: PathBuf::from("/tmp/does-not-matter"),
            read_only: matches!(role, Role::Reviewer | Role::Verifier),
        }
    }

    #[test]
    fn implementer_reports_a_change_and_is_deterministic() {
        let dir = std::env::temp_dir();
        let inv = invocation(Role::Implementer, Adapter::Claude, dir.clone());
        let a = StubInvoker.invoke(&inv);
        let b = StubInvoker.invoke(&inv);
        assert!(a.success);
        assert!(!a.changed_files.is_empty());
        assert_eq!(a.message, b.message);
        assert_eq!(a.transcript, b.transcript);
    }

    #[test]
    fn reviewer_emits_pass_verdict() {
        let inv = invocation(Role::Reviewer, Adapter::Codex, std::env::temp_dir());
        let result = StubInvoker.invoke(&inv);
        assert!(result.success);
        assert!(result.message.contains("VERDICT: PASS"));
        assert!(result.changed_files.is_empty());
    }

    #[test]
    fn verifier_reports_checks() {
        let inv = invocation(Role::Verifier, Adapter::Generic, std::env::temp_dir());
        let result = StubInvoker.invoke(&inv);
        assert!(result.success);
        assert!(result.message.to_lowercase().contains("verification"));
    }
}
