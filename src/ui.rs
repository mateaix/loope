//! Terminal presentation for the Loope CLI.
//!
//! The visual identity is built from Loope's own motifs: the infinity loop glyph `∞`
//! and the logo's two node colors — Claude blue and Codex orange. Color is enabled
//! only on a real TTY (overridable with `--color`), so piping, CI, and tests fall
//! back to plain output.

use std::io::{IsTerminal, Write, stdout};

use loope::executor::{LoopRun, StepObserver, StepOutcome};
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

/// Final summary box, colored by outcome.
pub fn summary(run: &LoopRun, run_dir: &std::path::Path, color: bool) {
    let outcome = if run.all_passed() {
        "all gates passed"
    } else if run.halted {
        "halted on a blocking gate"
    } else {
        "completed with gate failures"
    };

    if !color {
        println!("\n{}", run.to_report_markdown());
        println!("\nRun directory: {}", run_dir.display());
        return;
    }

    let (r, g, b) = if run.all_passed() { GREEN } else { RED };
    let accent = fg(r, g, b);
    let title = format!("∞ {} · {}", run.run_id, outcome);
    let width = title.chars().count();
    let bar = "─".repeat(width + 2);
    println!("\n  {accent}╭{bar}╮{RESET}");
    println!("  {accent}│ {BOLD}{title}{RESET}{accent} │{RESET}");
    println!("  {accent}╰{bar}╯{RESET}");
    println!("  {DIM}run dir: {}{RESET}", run_dir.display());
}

/// Render the `runs` listing.
pub fn runs_list(ids: &[String], color: bool) {
    if !color {
        for id in ids {
            println!("{id}");
        }
        return;
    }
    let blue = fg(28, 155, 240);
    for id in ids {
        println!("  {blue}∞{RESET} {id}");
    }
}
