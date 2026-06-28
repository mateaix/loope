//! Terminal presentation for the Loope CLI.
//!
//! The visual identity is built from Loope's own motifs: the infinity loop glyph `∞`
//! and the logo's two node colors — Claude blue and Codex orange. Color is enabled
//! only on a real TTY (overridable with `--color`), so piping, CI, and tests fall
//! back to plain output.

use std::io::{IsTerminal, Write, stdout};

use loope::executor::{StepObserver, StepOutcome};
use loope::{Adapter, LoopPlan, LoopStep, Role};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

/// Whether color was requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    /// Parse a `--color` value; unknown strings fall back to `Auto`.
    pub fn parse(s: &str) -> ColorChoice {
        match s.trim().to_ascii_lowercase().as_str() {
            "always" => ColorChoice::Always,
            "never" => ColorChoice::Never,
            _ => ColorChoice::Auto,
        }
    }

    /// Resolve to an on/off decision against the environment and the output stream.
    pub fn enabled(self) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::env::var_os("NO_COLOR").is_none() && stdout().is_terminal()
            }
        }
    }
}

/// Truecolor foreground escape.
fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

/// Brand color for an adapter: Claude blue, Codex orange, others neutral.
fn adapter_color(adapter: Adapter) -> String {
    match adapter {
        Adapter::Claude => fg(28, 155, 240),
        Adapter::Codex => fg(245, 161, 30),
        Adapter::OpenCode => fg(120, 180, 120),
        Adapter::Generic => fg(150, 150, 150),
    }
}

const GREEN: (u8, u8, u8) = (46, 160, 67);
const RED: (u8, u8, u8) = (220, 80, 80);

/// The brand banner, shown on `--help` and at the start of a run.
pub fn banner(color: bool) {
    if color {
        let blue = fg(28, 155, 240);
        let orange = fg(245, 161, 30);
        println!(
            "\n  {blue}∞{RESET} {BOLD}loope{RESET}  {DIM}loop engineering{RESET}\n  \
             {blue}●{RESET} claude   {orange}●{RESET} codex"
        );
    } else {
        println!("\n  loope — loop engineering");
    }
}

/// A one-line view of the steps about to run, e.g. `∞  implement -> review -> ...`.
pub fn pipeline(plan: &LoopPlan, color: bool) {
    if !color {
        return;
    }
    let blue = fg(28, 155, 240);
    let parts: Vec<String> = plan
        .steps
        .iter()
        .map(|s| {
            let label = role_verb(s.role);
            format!("{}{label}{RESET}", adapter_color(s.adapter))
        })
        .collect();
    println!("\n  {blue}∞{RESET}  {}", parts.join(&format!("{DIM} → {RESET}")));
    println!();
}

/// A short verb for a role, used in the pipeline view.
fn role_verb(role: Role) -> &'static str {
    match role {
        Role::Designer => "design",
        Role::Implementer => "implement",
        Role::Reviewer => "review",
        Role::Verifier => "verify",
    }
}

/// Live step renderer: prints a `running…` line that resolves in place on finish.
pub struct PrettyObserver;

impl StepObserver for PrettyObserver {
    fn on_step_start(&self, step: &LoopStep) {
        print!(
            "  {DIM}◌ {} {:<11}{RESET} {}{}{RESET} {DIM}running…{RESET}",
            step.id,
            step.role.as_str(),
            adapter_color(step.adapter),
            step.adapter.display_name(),
            RESET = RESET,
            DIM = DIM,
        );
        let _ = stdout().flush();
    }

    fn on_step_finish(&self, outcome: &StepOutcome) {
        let (icon, r, g, b) = if outcome.gate_passed {
            ("✓", GREEN.0, GREEN.1, GREEN.2)
        } else {
            ("✗", RED.0, RED.1, RED.2)
        };
        // \r returns to column 0; \x1b[K clears the running line.
        println!(
            "\r\x1b[K  {status}{icon}{RESET} {DIM}{id}{RESET} {role:<11} {ac}{agent}{RESET}  {DIM}{notes}{RESET}",
            status = fg(r, g, b),
            icon = icon,
            id = outcome.step_id,
            role = outcome.role.as_str(),
            ac = adapter_color(outcome.adapter),
            agent = outcome.adapter.display_name(),
            notes = outcome.gate_notes,
        );
    }
}

/// Render a Loope report. In plain mode the markdown is printed verbatim (so piping
/// and tests are unchanged); in color mode it becomes a summary box plus a colored
/// per-step recap. Used by both `run` (final output) and `show`.
pub fn print_report(md: &str, run_dir: Option<&std::path::Path>, color: bool) {
    if !color {
        print!("{md}");
        if !md.ends_with('\n') {
            println!();
        }
        if let Some(dir) = run_dir {
            println!("\nRun directory: {}", dir.display());
        }
        return;
    }

    let mut run_id = String::new();
    let mut outcome = String::new();
    for line in md.lines() {
        if let Some(value) = line.strip_prefix("- Run: ") {
            run_id = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("- Outcome: ") {
            outcome = value.trim().to_string();
        }
    }

    let passed = outcome.contains("all gates passed");
    let accent = if passed { GREEN } else { RED };
    let title = format!(
        "∞ {} · {}",
        if run_id.is_empty() { "run" } else { &run_id },
        outcome
    );
    print_box(&title, accent);

    for step in parse_steps(md) {
        let (sr, sg, sb) = if step.passed { GREEN } else { RED };
        let icon = if step.passed { "✓" } else { "✗" };
        let mut note = step.gate_result;
        if let Some(verdict) = step.verdict {
            note = if note.is_empty() {
                verdict
            } else {
                format!("{note} · {verdict}")
            };
        }
        println!(
            "  {sc}{icon}{RESET} {DIM}{num}{RESET} {role:<11} {DIM}·{RESET} {ac}{adapter}{RESET}  {DIM}{note}{RESET}",
            sc = fg(sr, sg, sb),
            num = step.num,
            role = step.role,
            ac = adapter_color_by_name(&step.adapter),
            adapter = step.adapter,
        );
    }

    if let Some(dir) = run_dir {
        println!("  {DIM}run dir: {}{RESET}", dir.display());
    }
}

/// Draw a single-line title in a colored box.
fn print_box(title: &str, (r, g, b): (u8, u8, u8)) {
    let accent = fg(r, g, b);
    let width = title.chars().count();
    let bar = "─".repeat(width + 2);
    println!("\n  {accent}╭{bar}╮{RESET}");
    println!("  {accent}│ {BOLD}{title}{RESET}{accent} │{RESET}");
    println!("  {accent}╰{bar}╯{RESET}");
}

/// One step parsed from a report for the colored recap.
struct StepLine {
    num: String,
    role: String,
    adapter: String,
    passed: bool,
    gate_result: String,
    verdict: Option<String>,
}

/// Parse the `## Steps` section of a Loope report markdown.
fn parse_steps(md: &str) -> Vec<StepLine> {
    let mut steps: Vec<StepLine> = Vec::new();
    let mut current: Option<StepLine> = None;
    for line in md.lines() {
        if let Some((num, role, adapter, passed)) = parse_step_header(line) {
            if let Some(step) = current.take() {
                steps.push(step);
            }
            current = Some(StepLine {
                num,
                role,
                adapter,
                passed,
                gate_result: String::new(),
                verdict: None,
            });
        } else if let Some(step) = current.as_mut() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("- Gate result: ") {
                step.gate_result = value.trim().to_string();
            } else if let Some(value) = trimmed.strip_prefix("- Verdict: ") {
                step.verdict = Some(value.trim().to_string());
            }
        }
    }
    if let Some(step) = current.take() {
        steps.push(step);
    }
    steps
}

/// Parse a step header line like `1. **implementer via Claude** — PASS`.
fn parse_step_header(line: &str) -> Option<(String, String, String, bool)> {
    let (num, rest) = line.split_once(". **")?;
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (mid, status) = rest.split_once("** — ")?;
    let (role, adapter) = mid.split_once(" via ")?;
    let passed = status.trim() == "PASS";
    Some((
        num.to_string(),
        role.to_string(),
        adapter.to_string(),
        passed,
    ))
}

/// Color for an adapter named by its display name (e.g. "Claude").
fn adapter_color_by_name(name: &str) -> String {
    match Adapter::parse(&name.to_lowercase()) {
        Some(adapter) => adapter_color(adapter),
        None => fg(150, 150, 150),
    }
}

/// One run's at-a-glance outcome for the `runs` listing.
pub struct RunSummary {
    pub passed: bool,
    pub halted: bool,
    pub steps: usize,
}

impl RunSummary {
    fn label(&self) -> &'static str {
        if self.passed {
            "all gates passed"
        } else if self.halted {
            "halted on a blocking gate"
        } else {
            "completed with gate failures"
        }
    }
}

/// Render the `runs` listing, with each run's outcome and step count when known.
pub fn runs_list(items: &[(String, Option<RunSummary>)], color: bool) {
    if !color {
        for (id, summary) in items {
            match summary {
                Some(s) => println!("{id}  {}  ({} steps)", s.label(), s.steps),
                None => println!("{id}"),
            }
        }
        return;
    }

    let blue = fg(28, 155, 240);
    for (id, summary) in items {
        match summary {
            Some(s) => {
                let (r, g, b) = if s.passed { GREEN } else { RED };
                let icon = if s.passed { "✓" } else { "✗" };
                println!(
                    "  {blue}∞{RESET} {id}  {sc}{icon}{RESET} {label}  {DIM}({steps} steps){RESET}",
                    sc = fg(r, g, b),
                    label = s.label(),
                    steps = s.steps,
                );
            }
            None => println!("  {blue}∞{RESET} {id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_step_header() {
        let parsed = parse_step_header("2. **reviewer via Codex** — PASS").unwrap();
        assert_eq!(parsed, ("2".into(), "reviewer".into(), "Codex".into(), true));
        let blocked = parse_step_header("3. **implementer via Claude** — BLOCK").unwrap();
        assert!(!blocked.3);
        assert!(parse_step_header("   - Gate: something").is_none());
        assert!(parse_step_header("# Loope Run Report").is_none());
    }

    #[test]
    fn parses_steps_with_gate_result_and_verdict() {
        let md = "## Steps\n\n\
            1. **implementer via Claude** — PASS\n\
            \u{20}  - Gate result: scoped change produced\n\
            2. **reviewer via Codex** — PASS\n\
            \u{20}  - Gate result: review produced\n\
            \u{20}  - Verdict: PASS (no blocking findings)\n";
        let steps = parse_steps(md);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].role, "implementer");
        assert_eq!(steps[0].gate_result, "scoped change produced");
        assert_eq!(steps[1].adapter, "Codex");
        assert_eq!(steps[1].verdict.as_deref(), Some("PASS (no blocking findings)"));
    }
}
