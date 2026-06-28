//! Structured review verdicts.
//!
//! A reviewer is asked to end its message with `VERDICT: PASS` or `VERDICT: BLOCK`.
//! [`parse_review_verdict`] turns the reviewer's final message into a structured
//! signal the gates can act on, with a conservative heuristic when no explicit
//! verdict line is present.

/// A reviewer's conclusion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewVerdict {
    /// Whether the reviewer flagged one or more blocking issues.
    pub has_blockers: bool,
    /// Short human-readable label.
    pub summary: String,
}

impl ReviewVerdict {
    fn pass() -> Self {
        Self {
            has_blockers: false,
            summary: "no blocking findings".to_string(),
        }
    }

    fn block() -> Self {
        Self {
            has_blockers: true,
            summary: "blockers found".to_string(),
        }
    }

    /// `PASS` or `BLOCK`, for compact display.
    pub fn label(&self) -> &'static str {
        if self.has_blockers { "BLOCK" } else { "PASS" }
    }
}

/// Parse a reviewer's final message into a [`ReviewVerdict`].
///
/// Prefers the last explicit `VERDICT: PASS|BLOCK` line; otherwise falls back to a
/// conservative phrase heuristic.
pub fn parse_review_verdict(message: &str) -> ReviewVerdict {
    for line in message.lines().rev() {
        let trimmed = line.trim().trim_start_matches(['*', '`', '-', ' ']);
        let rest = trimmed
            .strip_prefix("VERDICT:")
            .or_else(|| trimmed.strip_prefix("verdict:"));
        if let Some(rest) = rest {
            let value = rest.trim().to_ascii_uppercase();
            if value.starts_with("BLOCK") {
                return ReviewVerdict::block();
            }
            if value.starts_with("PASS") {
                return ReviewVerdict::pass();
            }
        }
    }

    // No explicit verdict: infer conservatively from the prose.
    let lower = message.to_lowercase();
    let says_clean = lower.contains("no blocking") || lower.contains("no blocker");
    let says_block = lower.contains("blocker")
        || lower.contains("must fix")
        || lower.contains("must be fixed");
    if says_block && !says_clean {
        ReviewVerdict::block()
    } else {
        ReviewVerdict::pass()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_pass_and_block() {
        assert!(!parse_review_verdict("looks good\nVERDICT: PASS").has_blockers);
        assert!(parse_review_verdict("found issues\nVERDICT: BLOCK").has_blockers);
    }

    #[test]
    fn last_verdict_line_wins() {
        let msg = "VERDICT: BLOCK\n...after addressing...\nVERDICT: PASS";
        assert!(!parse_review_verdict(msg).has_blockers);
    }

    #[test]
    fn tolerates_decoration_and_case() {
        assert!(parse_review_verdict("**verdict: block** - fix it").has_blockers);
        assert!(!parse_review_verdict("`VERDICT: PASS`").has_blockers);
    }

    #[test]
    fn heuristic_without_explicit_verdict() {
        assert!(parse_review_verdict("This is a blocker that must be fixed.").has_blockers);
        assert!(!parse_review_verdict("No blocking findings; looks solid.").has_blockers);
        assert!(!parse_review_verdict("Looks fine to me.").has_blockers);
    }

    #[test]
    fn label_matches_state() {
        assert_eq!(parse_review_verdict("VERDICT: PASS").label(), "PASS");
        assert_eq!(parse_review_verdict("VERDICT: BLOCK").label(), "BLOCK");
    }
}
