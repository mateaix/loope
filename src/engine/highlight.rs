//! The convergence highlight: the run's hero moment — a reviewer caught a blocker that a
//! later iteration fixed, and the run converged. Detected once from a run's outcomes,
//! persisted to the run directory, and rendered as a card by the CLI and TUI.

use super::executor::{StepOutcome, StopReason};
use crate::Role;

/// The "caught & fixed" arc, ready to render.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Highlight {
    /// Adapter that raised the block (e.g. "Codex").
    pub reviewer: String,
    pub flagged_iter: usize,
    /// One-line gist of the blocking finding.
    pub finding: String,
    /// Adapter that fixed it (e.g. "Claude").
    pub implementer: String,
    pub fixed_iter: usize,
    /// The fix's changed files, e.g. `src/auth.rs +29 -4`.
    pub fix_changes: Vec<String>,
    pub converged: bool,
}

impl Highlight {
    /// Detect the highlight: the earliest reviewer `BLOCK` that a later iteration resolved
    /// (a subsequent review `PASS`, or the run converged), plus the implementer change that
    /// fixed it. Returns `None` when no block was ever raised or none was resolved.
    pub fn detect(outcomes: &[StepOutcome], stop: StopReason) -> Option<Self> {
        let block = outcomes.iter().find(|o| {
            o.role == Role::Reviewer
                && o.review_verdict.as_ref().is_some_and(|v| v.has_blockers)
        })?;
        let k = block.iteration;

        let later_pass = outcomes.iter().any(|o| {
            o.role == Role::Reviewer
                && o.iteration > k
                && o.review_verdict.as_ref().is_some_and(|v| !v.has_blockers)
        });
        let converged = stop == StopReason::Converged;
        if !later_pass && !converged {
            return None;
        }

        // The fix is the first implementer step after the block.
        let fix = outcomes
            .iter()
            .find(|o| o.role == Role::Implementer && o.iteration > k)?;

        let finding = first_finding(
            &block.result.message,
            block.review_verdict.as_ref().map(|v| v.summary.as_str()).unwrap_or(""),
        );
        Some(Self {
            reviewer: block.adapter.display_name().to_string(),
            flagged_iter: k,
            finding,
            implementer: fix.adapter.display_name().to_string(),
            fixed_iter: fix.iteration,
            fix_changes: fix
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
            converged,
        })
    }

    /// Serialize to a tiny `key: value` record for the run directory.
    pub fn to_storage(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("reviewer: {}\n", self.reviewer));
        out.push_str(&format!("flagged: {}\n", self.flagged_iter));
        out.push_str(&format!("implementer: {}\n", self.implementer));
        out.push_str(&format!("fixed: {}\n", self.fixed_iter));
        out.push_str(&format!("converged: {}\n", self.converged));
        out.push_str(&format!("finding: {}\n", one_line(&self.finding)));
        for change in &self.fix_changes {
            out.push_str(&format!("change: {}\n", one_line(change)));
        }
        out
    }

    /// Parse the record written by [`Highlight::to_storage`].
    pub fn from_storage(text: &str) -> Option<Self> {
        let mut reviewer = None;
        let mut flagged = None;
        let mut implementer = None;
        let mut fixed = None;
        let mut converged = false;
        let mut finding = String::new();
        let mut fix_changes = Vec::new();
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("reviewer: ") {
                reviewer = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("flagged: ") {
                flagged = v.trim().parse().ok();
            } else if let Some(v) = line.strip_prefix("implementer: ") {
                implementer = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("fixed: ") {
                fixed = v.trim().parse().ok();
            } else if let Some(v) = line.strip_prefix("converged: ") {
                converged = v.trim() == "true";
            } else if let Some(v) = line.strip_prefix("finding: ") {
                finding = v.to_string();
            } else if let Some(v) = line.strip_prefix("change: ") {
                fix_changes.push(v.to_string());
            }
        }
        Some(Self {
            reviewer: reviewer?,
            flagged_iter: flagged?,
            finding,
            implementer: implementer?,
            fixed_iter: fixed?,
            fix_changes,
            converged,
        })
    }
}

/// The first meaningful line of a review message: skip blanks, the `VERDICT:` line, and a
/// bare "Blocking Findings" header. Falls back to the parsed verdict summary.
fn first_finding(message: &str, summary: &str) -> String {
    for raw in message.lines() {
        let line = raw.trim().trim_start_matches(['#', '*', '-', '>', ' ']).trim();
        let lower = line.to_ascii_lowercase();
        if line.is_empty()
            || lower.starts_with("verdict")
            || lower.trim_end_matches([':', '*']) == "blocking findings"
        {
            continue;
        }
        return truncate(line, 140);
    }
    if summary.is_empty() {
        "see the review".to_string()
    } else {
        truncate(summary, 140)
    }
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::InvocationResult;
    use crate::engine::review::ReviewVerdict;
    use crate::{Adapter, Role};

    fn reviewer(iter: usize, blockers: bool, msg: &str) -> StepOutcome {
        StepOutcome {
            step_id: iter * 2,
            role: Role::Reviewer,
            adapter: Adapter::Codex,
            prompt: String::new(),
            result: InvocationResult {
                success: true,
                message: msg.to_string(),
                changed_files: Vec::new(),
                transcript: String::new(),
            },
            gate: String::new(),
            gate_passed: true,
            gate_notes: String::new(),
            review_verdict: Some(ReviewVerdict {
                has_blockers: blockers,
                summary: if blockers { "blockers found".into() } else { "no blocking findings".into() },
            }),
            changes: Vec::new(),
            duration_ms: 0,
            iteration: iter,
        }
    }

    fn implementer(iter: usize) -> StepOutcome {
        StepOutcome {
            step_id: iter * 2 - 1,
            role: Role::Implementer,
            adapter: Adapter::Claude,
            prompt: String::new(),
            result: InvocationResult::failure("x"),
            gate: String::new(),
            gate_passed: true,
            gate_notes: String::new(),
            review_verdict: None,
            changes: Vec::new(),
            duration_ms: 0,
            iteration: iter,
        }
    }

    #[test]
    fn detects_block_then_fix_then_converge() {
        let outcomes = vec![
            implementer(1),
            reviewer(1, true, "Overflow in multiply panics\nVERDICT: BLOCK"),
            implementer(2),
            reviewer(2, false, "looks good\nVERDICT: PASS"),
        ];
        let h = Highlight::detect(&outcomes, StopReason::Converged).unwrap();
        assert_eq!(h.reviewer, "Codex");
        assert_eq!(h.flagged_iter, 1);
        assert_eq!(h.implementer, "Claude");
        assert_eq!(h.fixed_iter, 2);
        assert_eq!(h.finding, "Overflow in multiply panics");
        assert!(h.converged);
    }

    #[test]
    fn no_highlight_when_block_never_resolved() {
        let outcomes = vec![
            implementer(1),
            reviewer(1, true, "bad\nVERDICT: BLOCK"),
            implementer(2),
            reviewer(2, true, "still bad\nVERDICT: BLOCK"),
        ];
        assert_eq!(Highlight::detect(&outcomes, StopReason::MaxIters), None);
    }

    #[test]
    fn no_highlight_when_first_try_converges() {
        let outcomes = vec![implementer(1), reviewer(1, false, "good\nVERDICT: PASS")];
        assert_eq!(Highlight::detect(&outcomes, StopReason::Converged), None);
    }

    #[test]
    fn storage_round_trips() {
        let h = Highlight {
            reviewer: "Codex".into(),
            flagged_iter: 1,
            finding: "overflow panics".into(),
            implementer: "Claude".into(),
            fixed_iter: 2,
            fix_changes: vec!["src/lib.rs +5 -1".into()],
            converged: true,
        };
        assert_eq!(Highlight::from_storage(&h.to_storage()), Some(h));
    }
}
