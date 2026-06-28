//! Terminal presentation for the Loope CLI.
//!
//! The visual identity is built from Loope's own motifs: the infinity loop glyph `∞`
//! and the logo's two node colors — Claude blue and Codex orange. Color is enabled
//! only on a real TTY (overridable with `--color`), so piping, CI, and tests fall
//! back to plain output.

use std::io::{IsTerminal, Write, stdout};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use loope::event::{ActionKind, LoopEvent};
use loope::executor::{StepObserver, StepOutcome};
use loope::workspace::FileChange;
use loope::{Adapter, LoopStep, Role};

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

/// Foreground escape for a brand RGB color at the active color level.
fn fg(r: u8, g: u8, b: u8) -> String {
    crate::theme::rgb(r, g, b)
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

/// A one-line view of one iteration, e.g. `∞  implement → review → verify ↻`.
/// `repeats` appends a loop marker to signal the cycle iterates to convergence.
pub fn pipeline(steps: &[(Role, Adapter)], repeats: bool, color: bool) {
    if !color {
        return;
    }
    let blue = fg(28, 155, 240);
    let parts: Vec<String> = steps
        .iter()
        .map(|(role, adapter)| {
            let label = role_verb(*role);
            format!("{}{label}{RESET}", adapter_color(*adapter))
        })
        .collect();
    let loop_mark = if repeats {
        format!("{DIM} ↻{RESET}")
    } else {
        String::new()
    };
    println!(
        "\n  {blue}∞{RESET}  {}{loop_mark}",
        parts.join(&format!("{DIM} → {RESET}"))
    );
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

/// Live step renderer: a streaming activity feed under each step header.
pub struct PrettyObserver {
    /// When true, suppress the per-event feed but still show step headers/results.
    pub quiet: bool,
}

impl StepObserver for PrettyObserver {
    fn on_step_start(&self, step: &LoopStep) {
        let blue = fg(28, 155, 240);
        println!(
            "\n  {blue}▸{RESET} {DIM}{id}{RESET} {role:<11} {DIM}·{RESET} {ac}{agent}{RESET}",
            id = step.id,
            role = step.role.as_str(),
            ac = adapter_color(step.adapter),
            agent = step.adapter.display_name(),
        );
        let _ = stdout().flush();
    }

    fn on_event(&self, event: &LoopEvent) {
        if self.quiet {
            return;
        }
        match event {
            LoopEvent::Action { kind, target } => {
                println!(
                    "      {DIM}{icon} {label:<6}{RESET} {DIM}{target}{RESET}",
                    icon = action_icon(*kind),
                    label = kind.label(),
                );
            }
            LoopEvent::Message { text } => {
                println!("      {DIM}› {text}{RESET}");
            }
            LoopEvent::Model { .. } | LoopEvent::Usage { .. } => {}
        }
        let _ = stdout().flush();
    }

    fn on_step_finish(&self, outcome: &StepOutcome) {
        let (icon, accent) = if outcome.gate_passed {
            ("✓", GREEN)
        } else {
            ("✗", RED)
        };
        let (r, g, b) = accent;
        println!(
            "  {status}{icon}{RESET} {DIM}{id}{RESET} {role:<11} {ac}{agent}{RESET}  {DIM}{notes}{RESET}{stats}",
            status = fg(r, g, b),
            id = outcome.step_id,
            role = outcome.role.as_str(),
            ac = adapter_color(outcome.adapter),
            agent = outcome.adapter.display_name(),
            notes = outcome.gate_notes,
            stats = change_stats(&outcome.changes),
        );
    }
}

/// A small glyph for an action kind.
fn action_icon(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Read => "◇",
        ActionKind::Edit | ActionKind::Write => "✎",
        ActionKind::Command => "▸",
        ActionKind::Search => "⌕",
        ActionKind::Other => "·",
    }
}

/// Colored ` path +A −R` stats for a finished step (first few files).
fn change_stats(changes: &[FileChange]) -> String {
    if changes.is_empty() {
        return String::new();
    }
    let green = fg(GREEN.0, GREEN.1, GREEN.2);
    let red = fg(RED.0, RED.1, RED.2);
    let mut parts = Vec::new();
    for change in changes.iter().take(3) {
        if change.binary {
            parts.push(format!("{DIM}{} (bin){RESET}", change.path));
        } else {
            parts.push(format!(
                "{DIM}{}{RESET} {green}+{}{RESET} {red}−{}{RESET}",
                change.path, change.added, change.removed
            ));
        }
    }
    if changes.len() > 3 {
        parts.push(format!("{DIM}+{} more{RESET}", changes.len() - 3));
    }
    format!("  {}", parts.join(&format!("{DIM}, {RESET}")))
}

/// Spinner frames for the live status line.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Messages the live renderer consumes from the executor's observer.
pub enum RenderMsg {
    Iteration {
        n: usize,
        total: usize,
    },
    StepStart {
        id: usize,
        role: Role,
        adapter: Adapter,
    },
    Action {
        kind: ActionKind,
        target: String,
    },
    Message {
        text: String,
    },
    StepFinish {
        id: usize,
        role: Role,
        adapter: Adapter,
        passed: bool,
        notes: String,
        changes: Vec<FileChange>,
    },
    Stop,
}

/// Owns terminal output during a run: a ticker thread animates a pinned live status
/// line while completed lines are committed to scrollback.
pub struct LiveRenderer {
    tx: Sender<RenderMsg>,
    handle: Option<JoinHandle<()>>,
}

impl LiveRenderer {
    /// Start the renderer for a run of `total` steps.
    pub fn start(total: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || render_loop(rx, total));
        Self {
            tx,
            handle: Some(handle),
        }
    }

    /// A sender the observer uses to feed events.
    pub fn sender(&self) -> Sender<RenderMsg> {
        self.tx.clone()
    }

    /// Stop the renderer, clearing the live line, and join its thread.
    pub fn stop(mut self) {
        let _ = self.tx.send(RenderMsg::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The currently running step, for the animated live line.
struct Current {
    id: usize,
    role: Role,
    adapter: Adapter,
    start: Instant,
    last_action: Option<String>,
}

/// The render loop: redraw the live line ~10×/s, commit finished lines to scrollback.
fn render_loop(rx: Receiver<RenderMsg>, total: usize) {
    let mut current: Option<Current> = None;
    let mut live_drawn = false;
    let mut frame = 0usize;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(RenderMsg::Iteration { n, total }) => {
                let blue = fg(28, 155, 240);
                commit(
                    &mut live_drawn,
                    &format!("\n  {blue}∞ iteration {n}/{total}{RESET}"),
                );
            }
            Ok(RenderMsg::StepStart { id, role, adapter }) => {
                commit(&mut live_drawn, &step_header(id, role, adapter));
                current = Some(Current {
                    id,
                    role,
                    adapter,
                    start: Instant::now(),
                    last_action: None,
                });
                redraw(&mut live_drawn, frame, current.as_ref(), total);
            }
            Ok(RenderMsg::Action { kind, target }) => {
                commit(&mut live_drawn, &action_line(kind, &target));
                if let Some(c) = current.as_mut() {
                    c.last_action = Some(format!("{} {}", kind.label(), target));
                }
                redraw(&mut live_drawn, frame, current.as_ref(), total);
            }
            Ok(RenderMsg::Message { text }) => {
                commit(&mut live_drawn, &format!("      {DIM}› {text}{RESET}"));
                redraw(&mut live_drawn, frame, current.as_ref(), total);
            }
            Ok(RenderMsg::StepFinish {
                id,
                role,
                adapter,
                passed,
                notes,
                changes,
            }) => {
                let elapsed = current.as_ref().map(|c| c.start.elapsed()).unwrap_or_default();
                clear_live(&mut live_drawn);
                commit(
                    &mut live_drawn,
                    &finish_line(id, role, adapter, passed, &notes, &changes, elapsed),
                );
                current = None;
            }
            Ok(RenderMsg::Stop) => {
                clear_live(&mut live_drawn);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                frame = frame.wrapping_add(1);
                if current.is_some() {
                    redraw(&mut live_drawn, frame, current.as_ref(), total);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                clear_live(&mut live_drawn);
                break;
            }
        }
    }
}

/// Print a line into scrollback above the live region.
fn commit(live_drawn: &mut bool, line: &str) {
    let mut out = stdout().lock();
    if *live_drawn {
        let _ = write!(out, "\r\x1b[2K");
    }
    let _ = writeln!(out, "{line}");
    *live_drawn = false;
    let _ = out.flush();
}

/// Clear the live line if drawn.
fn clear_live(live_drawn: &mut bool) {
    if *live_drawn {
        let mut out = stdout().lock();
        let _ = write!(out, "\r\x1b[2K");
        let _ = out.flush();
        *live_drawn = false;
    }
}

/// Repaint the live line in place.
fn redraw(live_drawn: &mut bool, frame: usize, current: Option<&Current>, total: usize) {
    let Some(c) = current else {
        return;
    };
    let mut out = stdout().lock();
    let _ = write!(out, "\r\x1b[2K{}", live_line(c, frame, total));
    let _ = out.flush();
    *live_drawn = true;
}

fn live_line(c: &Current, frame: usize, total: usize) -> String {
    let blue = fg(28, 155, 240);
    let spin = SPINNER[frame % SPINNER.len()];
    let elapsed = fmt_elapsed(c.start.elapsed());
    let action = c.last_action.clone().unwrap_or_default();
    format!(
        "  {blue}{spin}{RESET} {role:<11} {DIM}·{RESET} {ac}{ag}{RESET}  {DIM}{elapsed}{RESET}  {DIM}{action}{RESET}  {DIM}[{id}/{total}]{RESET}",
        role = c.role.as_str(),
        ac = adapter_color(c.adapter),
        ag = c.adapter.display_name(),
        id = c.id,
    )
}

fn step_header(id: usize, role: Role, adapter: Adapter) -> String {
    let blue = fg(28, 155, 240);
    format!(
        "\n  {blue}▸{RESET} {DIM}{id}{RESET} {role:<11} {DIM}·{RESET} {ac}{ag}{RESET}",
        role = role.as_str(),
        ac = adapter_color(adapter),
        ag = adapter.display_name(),
    )
}

fn action_line(kind: ActionKind, target: &str) -> String {
    format!(
        "      {DIM}{icon} {label:<6}{RESET} {DIM}{target}{RESET}",
        icon = action_icon(kind),
        label = kind.label(),
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_line(
    id: usize,
    role: Role,
    adapter: Adapter,
    passed: bool,
    notes: &str,
    changes: &[FileChange],
    elapsed: Duration,
) -> String {
    let (icon, (r, g, b)) = if passed { ("✓", GREEN) } else { ("✗", RED) };
    format!(
        "  {sc}{icon}{RESET} {DIM}{id}{RESET} {role:<11} {ac}{ag}{RESET}  {DIM}{el}{RESET}  {DIM}{notes}{RESET}{stats}",
        sc = fg(r, g, b),
        role = role.as_str(),
        ac = adapter_color(adapter),
        ag = adapter.display_name(),
        el = fmt_elapsed(elapsed),
        stats = change_stats(changes),
    )
}

/// Format a duration as `m:ss`.
fn fmt_elapsed(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// A [`StepObserver`] that forwards events to a [`LiveRenderer`].
pub struct LiveObserver {
    tx: Sender<RenderMsg>,
}

impl LiveObserver {
    pub fn new(tx: Sender<RenderMsg>) -> Self {
        Self { tx }
    }
}

impl StepObserver for LiveObserver {
    fn on_iteration_start(&self, n: usize, total: usize) {
        let _ = self.tx.send(RenderMsg::Iteration { n, total });
    }

    fn on_step_start(&self, step: &LoopStep) {
        let _ = self.tx.send(RenderMsg::StepStart {
            id: step.id,
            role: step.role,
            adapter: step.adapter,
        });
    }

    fn on_event(&self, event: &LoopEvent) {
        match event {
            LoopEvent::Action { kind, target } => {
                let _ = self.tx.send(RenderMsg::Action {
                    kind: *kind,
                    target: target.clone(),
                });
            }
            LoopEvent::Message { text } => {
                let _ = self.tx.send(RenderMsg::Message { text: text.clone() });
            }
            LoopEvent::Model { .. } | LoopEvent::Usage { .. } => {}
        }
    }

    fn on_step_finish(&self, outcome: &StepOutcome) {
        let _ = self.tx.send(RenderMsg::StepFinish {
            id: outcome.step_id,
            role: outcome.role,
            adapter: outcome.adapter,
            passed: outcome.gate_passed,
            notes: outcome.gate_notes.clone(),
            changes: outcome.changes.clone(),
        });
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
    let mut total = String::new();
    let mut seen_steps = false;
    for line in md.lines() {
        if line.starts_with("## Steps") {
            seen_steps = true;
        }
        if let Some(value) = line.strip_prefix("- Run: ") {
            run_id = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("- Outcome: ") {
            outcome = value.trim().to_string();
        } else if !seen_steps {
            // the run-level (not per-step) "Took" appears before the Steps section
            if let Some(value) = line.strip_prefix("- Took: ") {
                total = value.trim().to_string();
            }
        }
    }

    let passed = outcome.contains("converged") || outcome.contains("design contract");
    let accent = if passed { GREEN } else { RED };
    let title = if total.is_empty() {
        format!(
            "∞ {} · {}",
            if run_id.is_empty() { "run" } else { &run_id },
            outcome
        )
    } else {
        format!(
            "∞ {} · {} · {}",
            if run_id.is_empty() { "run" } else { &run_id },
            outcome,
            total
        )
    };
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
        if !step.changes.is_empty() {
            note = format!("{note} · {}", step.changes.join(", "));
        }
        if let Some(took) = step.took {
            note = format!("{note}  {took}");
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
    changes: Vec<String>,
    took: Option<String>,
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
                changes: Vec::new(),
                took: None,
            });
        } else if let Some(step) = current.as_mut() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("- Gate result: ") {
                step.gate_result = value.trim().to_string();
            } else if let Some(value) = trimmed.strip_prefix("- Verdict: ") {
                step.verdict = Some(value.trim().to_string());
            } else if let Some(value) = trimmed.strip_prefix("- Changed: ") {
                step.changes.push(value.trim().to_string());
            } else if let Some(value) = trimmed.strip_prefix("- Took: ") {
                step.took = Some(value.trim().to_string());
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
            "converged"
        } else if self.halted {
            "halted: a step failed"
        } else {
            "did not converge (max-iters)"
        }
    }
}

/// Print a unified diff. In plain mode the text is printed verbatim. On a TTY each
/// hunk gets a line-number gutter, with additions green and removals red.
pub fn print_diff(diff: &str, color: bool) {
    if !color {
        print!("{diff}");
        if !diff.is_empty() && !diff.ends_with('\n') {
            println!();
        }
        return;
    }
    let green = fg(GREEN.0, GREEN.1, GREEN.2);
    let red = fg(RED.0, RED.1, RED.2);
    let blue = fg(28, 155, 240);
    // Track line numbers within the current hunk, seeded from each `@@` header.
    let mut old_no = 0usize;
    let mut new_no = 0usize;
    for line in diff.lines() {
        if line.starts_with("diff ") {
            println!("  {BOLD}{line}{RESET}");
        } else if line.starts_with("+++") || line.starts_with("---") {
            println!("  {DIM}{line}{RESET}");
        } else if line.starts_with("@@") {
            if let Some((o, n)) = parse_hunk_header(line) {
                old_no = o;
                new_no = n;
            }
            println!("  {blue}{line}{RESET}");
        } else if let Some(rest) = line.strip_prefix('+') {
            println!("  {DIM}{:>4} {RESET}{green}+{rest}{RESET}", new_no);
            new_no += 1;
        } else if let Some(rest) = line.strip_prefix('-') {
            println!("  {DIM}{:>4} {RESET}{red}-{rest}{RESET}", old_no);
            old_no += 1;
        } else if let Some(rest) = line.strip_prefix(' ') {
            println!("  {DIM}{:>4} {RESET} {DIM}{rest}{RESET}", new_no);
            old_no += 1;
            new_no += 1;
        } else {
            println!("  {DIM}{line}{RESET}");
        }
    }
}

/// Parse the starting old/new line numbers from a `@@ -a,b +c,d @@` header.
fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    let inner = line.trim_start_matches('@').trim();
    let mut parts = inner.split_whitespace();
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');
    let old_n = old.split(',').next()?.parse().ok()?;
    let new_n = new.split(',').next()?.parse().ok()?;
    Some((old_n, new_n))
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
    fn parses_hunk_header_line_numbers() {
        assert_eq!(parse_hunk_header("@@ -12,3 +15,4 @@"), Some((12, 15)));
        assert_eq!(parse_hunk_header("@@ -0,0 +1,3 @@"), Some((0, 1)));
        assert_eq!(parse_hunk_header("not a hunk"), None);
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
