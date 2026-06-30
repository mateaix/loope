//! Loop executor: runs an iterative loop (design once, then implement → review →
//! verify until convergence or the iteration cap), driving each step through an
//! [`Invoker`] and persisting a run.

use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::adapter::{AgentInvocation, InvocationResult, Invoker};
use crate::adapter::event::{LoopEvent, events_to_jsonl};
use crate::engine::review::{ReviewVerdict, parse_review_verdict};
use crate::engine::workspace::{
    FileChange, FileDiff, RunWorkspace, atomic_write, changed_files, compute_file_diffs,
    content_snapshot, render_diffs, snapshot,
};
use crate::{Adapter, Role, prompt_for_step};

/// Why a run stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    /// Verification passed and no reviewer reported blockers.
    Converged,
    /// Reached the iteration cap without converging.
    MaxIters,
    /// A step's agent failed or timed out — the loop halted immediately.
    StepFailed,
    /// A design-only run produced its contract.
    DesignOnly,
    /// The user asked to stop (the loop halted at a step boundary).
    Cancelled,
    /// An iteration made no progress (no change, or the same failure as before); stopped
    /// early instead of burning the remaining iteration budget.
    Stalled,
}

impl StopReason {
    /// Human label for the report/summary.
    pub fn label(self) -> &'static str {
        match self {
            StopReason::Converged => "converged",
            StopReason::MaxIters => "stopped at the iteration cap",
            StopReason::StepFailed => "halted: a step failed",
            StopReason::DesignOnly => "design contract produced",
            StopReason::Cancelled => "stopped by user",
            StopReason::Stalled => "stopped: no progress",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            StopReason::Converged => "converged",
            StopReason::MaxIters => "max_iters",
            StopReason::StepFailed => "step_failed",
            StopReason::DesignOnly => "design_only",
            StopReason::Cancelled => "cancelled",
            StopReason::Stalled => "stalled",
        }
    }
}

/// Configuration for an iterative loop run.
#[derive(Clone, Debug)]
pub struct LoopConfig {
    pub requirement: String,
    pub include_design: bool,
    pub designer: Adapter,
    pub implementer: Adapter,
    pub reviewers: Vec<Adapter>,
    /// Max iterations of implement → review → verify. `0` means design-only.
    pub max_iters: usize,
    /// When set, the verifier runs this shell command; verification passes iff it
    /// exits zero. With no command, verification is informational (always passing).
    pub verify_command: Option<String>,
}

/// Observes step execution so a frontend can render live progress.
pub trait StepObserver {
    /// Called at the start of each iteration (1-based) of a run of `total` iterations.
    fn on_iteration_start(&self, _iteration: usize, _total: usize) {}
    /// Called just before a step's agent (or verify command) runs.
    fn on_step_start(&self, step: &crate::LoopStep);
    /// Called for each normalized event a step emits while it runs. Default no-op.
    fn on_event(&self, _event: &crate::adapter::event::LoopEvent) {}
    /// Called once a step's outcome (including its gate result) is known.
    fn on_step_finish(&self, outcome: &StepOutcome);
    /// Polled at step boundaries; returning true asks the loop to stop. Default never.
    fn should_cancel(&self) -> bool {
        false
    }
}

/// Outcome of one executed step.
#[derive(Clone, Debug)]
pub struct StepOutcome {
    pub step_id: usize,
    pub role: Role,
    pub adapter: Adapter,
    pub prompt: String,
    pub result: InvocationResult,
    pub gate: String,
    pub gate_passed: bool,
    pub gate_notes: String,
    /// For reviewer steps, the parsed verdict.
    pub review_verdict: Option<ReviewVerdict>,
    /// Per-file changes a write step made to the workspace.
    pub changes: Vec<FileChange>,
    /// Wall-clock time the step took, in milliseconds.
    pub duration_ms: u64,
    /// The iteration (1-based) this step ran in; `0` for the design step.
    pub iteration: usize,
}

/// The result of executing a whole loop.
#[derive(Clone, Debug)]
pub struct LoopRun {
    pub run_id: String,
    pub requirement: String,
    pub outcomes: Vec<StepOutcome>,
    /// Number of implement → review → verify iterations run.
    pub iterations: usize,
    /// Why the run stopped.
    pub stop_reason: StopReason,
    /// Cumulative per-file changes from the original source to the final workspace.
    pub changed: Vec<FileChange>,
    /// The "caught & fixed" hero moment, when the review earned its keep.
    pub highlight: Option<crate::engine::Highlight>,
    /// Total wall-clock time of the run, in milliseconds.
    pub total_ms: u64,
}

impl LoopRun {
    /// True when the run reached a successful stop (converged, or a design-only run).
    pub fn all_passed(&self) -> bool {
        matches!(self.stop_reason, StopReason::Converged | StopReason::DesignOnly)
    }

    /// Total added/removed lines across all cumulative changes.
    pub fn change_stats(&self) -> (usize, usize) {
        self.changed
            .iter()
            .fold((0, 0), |(a, r), c| (a + c.added, r + c.removed))
    }

    /// Human-readable final loop report.
    pub fn to_report_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Loope Run Report\n\n");
        out.push_str(&format!("- Run: {}\n", self.run_id));
        out.push_str(&format!("- Requirement: {}\n", self.requirement));
        out.push_str(&format!("- Outcome: {}\n", self.stop_reason.label()));
        out.push_str(&format!("- Iterations: {}\n", self.iterations));
        let (added, removed) = self.change_stats();
        out.push_str(&format!(
            "- Changed: {} file(s) +{} -{}\n",
            self.changed.len(),
            added,
            removed
        ));
        out.push_str(&format!("- Took: {}\n\n", fmt_duration(self.total_ms)));
        out.push_str("## Steps\n\n");

        let mut shown_iter = usize::MAX;
        for o in &self.outcomes {
            if o.iteration != shown_iter {
                shown_iter = o.iteration;
                if o.iteration == 0 {
                    out.push_str("### Design\n\n");
                } else {
                    out.push_str(&format!("### Iteration {}\n\n", o.iteration));
                }
            }
            let status = if o.gate_passed { "PASS" } else { "BLOCK" };
            out.push_str(&format!(
                "{}. **{} via {}** — {}\n",
                o.step_id,
                o.role.as_str(),
                o.adapter.display_name(),
                status
            ));
            out.push_str(&format!("   - Gate: {}\n", o.gate));
            out.push_str(&format!("   - Took: {}\n", fmt_duration(o.duration_ms)));
            out.push_str(&format!("   - Gate result: {}\n", o.gate_notes));
            if let Some(verdict) = &o.review_verdict {
                out.push_str(&format!(
                    "   - Verdict: {} ({})\n",
                    verdict.label(),
                    verdict.summary
                ));
            }
            out.push_str(&format!("   - Message: {}\n", first_line(&o.result.message)));
            for change in &o.changes {
                if change.binary {
                    out.push_str(&format!("   - Changed: {} (binary)\n", change.path));
                } else {
                    out.push_str(&format!(
                        "   - Changed: {} +{} -{}\n",
                        change.path, change.added, change.removed
                    ));
                }
            }
        }

        out
    }

    /// Compact machine-readable run record.
    pub fn to_run_json(&self) -> String {
        let steps: Vec<String> = self
            .outcomes
            .iter()
            .map(|o| {
                format!(
                    "{{\"id\":{},\"iteration\":{},\"role\":\"{}\",\"adapter\":\"{}\",\"gate_passed\":{},\"success\":{}}}",
                    o.step_id,
                    o.iteration,
                    o.role.as_str(),
                    o.adapter.as_str(),
                    o.gate_passed,
                    o.result.success
                )
            })
            .collect();

        format!(
            "{{\"run_id\":\"{}\",\"requirement\":\"{}\",\"converged\":{},\"highlight\":{},\"iterations\":{},\"stop_reason\":\"{}\",\"steps\":[{}]}}\n",
            json_escape(&self.run_id),
            json_escape(&self.requirement),
            self.all_passed(),
            self.highlight.is_some(),
            self.iterations,
            self.stop_reason.as_str(),
            steps.join(",")
        )
    }
}

/// A step's raw run (before the gate is decided).
struct StepRun {
    result: InvocationResult,
    changes: Vec<FileChange>,
    duration_ms: u64,
}

/// Build a synthetic [`crate::LoopStep`] for a run step.
fn make_step(id: usize, role: Role, adapter: Adapter, gate: &str) -> crate::LoopStep {
    crate::LoopStep {
        id,
        role,
        adapter,
        objective: String::new(),
        expected_artifact: String::new(),
        gate: gate.to_string(),
    }
}

/// Execute an iterative loop: design once (if configured), then repeat
/// implement → review → verify until it converges or hits `max_iters`.
pub fn execute_loop(
    config: &LoopConfig,
    workspace: &RunWorkspace,
    invoker: &(dyn Invoker + Sync),
    observer: Option<&dyn StepObserver>,
) -> io::Result<LoopRun> {
    let run_started = Instant::now();
    // The original source, captured before any step runs, for the cumulative diff.
    let baseline = content_snapshot(&workspace.workspace_dir);
    let mut outcomes: Vec<StepOutcome> = Vec::new();
    let mut step_id = 0usize;
    let mut design_contract: Option<String> = None;
    let mut iterations = 0usize;
    let mut stop_reason = StopReason::MaxIters;

    // --- Design (once) ---
    if config.include_design {
        step_id += 1;
        let step = make_step(
            step_id,
            Role::Designer,
            config.designer,
            "a non-empty design contract is produced",
        );
        let prompt = build_prompt(&step, &config.requirement, None, None);
        let run = execute_one(workspace, &step, &prompt, invoker, observer, None)?;
        let ok = run.result.success && !run.result.message.trim().is_empty();
        if ok {
            let contract = run.result.message.clone();
            atomic_write(&workspace.root.join("design-contract.md"), &contract)?;
            atomic_write(
                &workspace.workspace_dir.join("DESIGN_CONTRACT.md"),
                &contract,
            )?;
            design_contract = Some(contract);
        }
        let notes = if ok {
            "design contract produced"
        } else {
            "design step failed"
        };
        finish(observer, &mut outcomes, step_outcome(&step, 0, run, ok, notes, None, prompt));
        if !ok {
            return finalize(workspace, &config.requirement, &baseline, run_started, outcomes, 0, StopReason::StepFailed);
        }
        if config.max_iters == 0 {
            return finalize(workspace, &config.requirement, &baseline, run_started, outcomes, 0, StopReason::DesignOnly);
        }
    }

    // --- Iterations ---
    let max = config.max_iters.max(1);
    let mut feedback: Option<String> = None;
    // The prior iteration's failure signature, to detect no-progress stalls.
    let mut prev_failure_sig: Option<String> = None;

    'iterations: for iter in 1..=max {
        iterations = iter;
        if observer.is_some_and(|o| o.should_cancel()) {
            stop_reason = StopReason::Cancelled;
            break 'iterations;
        }
        if let Some(observer) = observer {
            observer.on_iteration_start(iter, max);
        }

        // Implement / fix.
        step_id += 1;
        let istep = make_step(
            step_id,
            Role::Implementer,
            config.implementer,
            if iter == 1 {
                "initial implementation"
            } else {
                "address prior review and verification failures"
            },
        );
        let iprompt = build_prompt(
            &istep,
            &config.requirement,
            feedback.as_deref(),
            design_contract.as_deref(),
        );
        let irun = execute_one(workspace, &istep, &iprompt, invoker, observer, None)?;
        if !irun.result.success {
            finish(observer, &mut outcomes, step_outcome(&istep, iter, irun, false, "invocation failed", None, iprompt));
            stop_reason = StopReason::StepFailed;
            break 'iterations;
        }
        let changed = !irun.changes.is_empty();
        let inotes = if changed { "change produced" } else { "no change made" };
        finish(observer, &mut outcomes, step_outcome(&istep, iter, irun, true, inotes, None, iprompt));

        if observer.is_some_and(|o| o.should_cancel()) {
            stop_reason = StopReason::Cancelled;
            break 'iterations;
        }

        // Review (reviewers run concurrently on the current state).
        let mut review_steps = Vec::new();
        let mut review_prompts = Vec::new();
        for &reviewer in &config.reviewers {
            step_id += 1;
            let rstep = make_step(step_id, Role::Reviewer, reviewer, "review produced");
            let rprompt = build_prompt(&rstep, &config.requirement, None, design_contract.as_deref());
            review_steps.push(rstep);
            review_prompts.push(rprompt);
        }
        for (s, p) in review_steps.iter().zip(&review_prompts) {
            workspace.agent_home(s.id, s.role, s.adapter)?;
            atomic_write(&workspace.agent_dir(s.id, s.role, s.adapter).join("prompt.md"), p)?;
        }
        let review_outcomes = if review_steps.len() == 1 {
            if let Some(observer) = observer {
                observer.on_step_start(&review_steps[0]);
            }
            vec![run_reviewer(workspace, &review_steps[0], &review_prompts[0], iter, invoker)?]
        } else {
            run_reviewers_parallel(workspace, &review_steps, &review_prompts, iter, invoker)?
        };

        let mut has_blockers = false;
        let mut blocker_messages = Vec::new();
        let mut review_failed = false;
        for outcome in review_outcomes {
            if let Some(verdict) = &outcome.review_verdict
                && verdict.has_blockers
            {
                has_blockers = true;
                blocker_messages.push(format!(
                    "{}: {}",
                    outcome.adapter.display_name(),
                    first_line(&outcome.result.message)
                ));
            }
            if !outcome.result.success {
                review_failed = true;
            }
            finish(observer, &mut outcomes, outcome);
        }
        if review_failed {
            stop_reason = StopReason::StepFailed;
            break 'iterations;
        }
        if observer.is_some_and(|o| o.should_cancel()) {
            stop_reason = StopReason::Cancelled;
            break 'iterations;
        }

        // Verify (only when a real check command is configured).
        let mut verify_pass = true;
        let mut verify_failure: Option<String> = None;
        if let Some(cmd) = &config.verify_command {
            step_id += 1;
            let vstep = make_step(step_id, Role::Verifier, Adapter::Generic, "verify command exits zero");
            let vprompt = build_prompt(&vstep, &config.requirement, None, design_contract.as_deref());
            let vrun = execute_one(workspace, &vstep, &vprompt, invoker, observer, Some(cmd))?;
            verify_pass = vrun.result.success;
            if !verify_pass {
                // Keep the full output; compose_feedback parses the failing checks out of it
                // and appends a bounded tail.
                verify_failure = Some(vrun.result.transcript.clone());
            }
            let vnotes = if verify_pass {
                "verification passed"
            } else {
                "verification failed"
            };
            finish(observer, &mut outcomes, step_outcome(&vstep, iter, vrun, verify_pass, vnotes, None, vprompt));
        }

        if verify_pass && !has_blockers {
            stop_reason = StopReason::Converged;
            break 'iterations;
        }
        // Stall: this iteration made no progress — the implementer changed nothing, or the
        // verify failure is the same as last time. Stop early rather than burn the budget.
        let failure_sig = verify_failure.as_deref().map(|f| {
            summarize_failures(f).unwrap_or_else(|| tail_lines(f, FEEDBACK_MAX_LINES, FEEDBACK_MAX_BYTES))
        });
        let same_failure = failure_sig.is_some() && failure_sig == prev_failure_sig;
        if !changed || same_failure {
            stop_reason = StopReason::Stalled;
            break 'iterations;
        }
        if iter >= max {
            stop_reason = StopReason::MaxIters;
            break 'iterations;
        }
        prev_failure_sig = failure_sig;
        feedback = Some(compose_feedback(&blocker_messages, verify_failure.as_deref()));
    }

    finalize(workspace, &config.requirement, &baseline, run_started, outcomes, iterations, stop_reason)
}

/// Push an outcome (notifying the observer) into the running list.
fn finish(observer: Option<&dyn StepObserver>, outcomes: &mut Vec<StepOutcome>, outcome: StepOutcome) {
    if let Some(observer) = observer {
        observer.on_step_finish(&outcome);
    }
    outcomes.push(outcome);
}

/// Assemble a [`StepOutcome`] from a [`StepRun`] and its gate decision.
fn step_outcome(
    step: &crate::LoopStep,
    iteration: usize,
    run: StepRun,
    gate_passed: bool,
    gate_notes: &str,
    verdict: Option<ReviewVerdict>,
    prompt: String,
) -> StepOutcome {
    StepOutcome {
        step_id: step.id,
        role: step.role,
        adapter: step.adapter,
        prompt,
        result: run.result,
        gate: step.gate.clone(),
        gate_passed,
        gate_notes: gate_notes.to_string(),
        review_verdict: verdict,
        changes: run.changes,
        duration_ms: run.duration_ms,
        iteration,
    }
}

/// Write the report + run record (plus the cumulative diff and changed-file listing)
/// and return the [`LoopRun`].
fn finalize(
    workspace: &RunWorkspace,
    requirement: &str,
    baseline: &std::collections::BTreeMap<String, String>,
    started: Instant,
    outcomes: Vec<StepOutcome>,
    iterations: usize,
    stop_reason: StopReason,
) -> io::Result<LoopRun> {
    // Cumulative diff: the original source snapshot versus the final workspace.
    let after = content_snapshot(&workspace.workspace_dir);
    let mut candidates: Vec<String> = baseline.keys().cloned().collect();
    for path in after.keys() {
        if !baseline.contains_key(path) {
            candidates.push(path.clone());
        }
    }
    let diffs: Vec<FileDiff> = compute_file_diffs(&workspace.workspace_dir, &candidates, baseline)
        .into_iter()
        .filter(|d| d.change.binary || d.change.added + d.change.removed > 0)
        .collect();
    atomic_write(&workspace.root.join("changes.diff"), &render_diffs(&diffs))?;
    let changed: Vec<FileChange> = diffs.into_iter().map(|d| d.change).collect();
    // A flat listing of applyable (still-present) files, for `loope apply`.
    let listing: String = changed
        .iter()
        .filter(|c| after.contains_key(&c.path))
        .map(|c| format!("{}\n", c.path))
        .collect();
    atomic_write(&workspace.root.join("changed-files.txt"), &listing)?;

    // The hero moment: a reviewer caught a blocker that a later iteration fixed.
    let highlight = crate::engine::Highlight::detect(&outcomes, stop_reason);
    if let Some(highlight) = &highlight {
        atomic_write(&workspace.root.join("highlight"), &highlight.to_storage())?;
    }

    let run = LoopRun {
        run_id: workspace.run_id.clone(),
        requirement: requirement.to_string(),
        outcomes,
        iterations,
        stop_reason,
        changed,
        highlight,
        total_ms: started.elapsed().as_millis() as u64,
    };
    atomic_write(&workspace.root.join("report.md"), &run.to_report_markdown())?;
    atomic_write(&workspace.root.join("run.json"), &run.to_run_json())?;
    Ok(run)
}

/// Compose a feedback block from review blockers and a verification failure.
fn compose_feedback(blockers: &[String], verify_failure: Option<&str>) -> String {
    let mut out = String::new();
    // Lead with the specific failing checks (parsed from the verifier output) — the most
    // actionable signal for the next repair attempt.
    if let Some(failure) = verify_failure
        && let Some(summary) = summarize_failures(failure)
    {
        out.push_str("These checks are failing — fix them:\n");
        out.push_str(&summary);
        out.push('\n');
    }
    if !blockers.is_empty() {
        out.push_str("\nThe previous review raised blocking findings to address:\n");
        for blocker in blockers {
            out.push_str(&format!("- {blocker}\n"));
        }
    }
    if let Some(failure) = verify_failure {
        out.push_str("\nVerify command output (tail):\n");
        out.push_str(tail_lines(failure, FEEDBACK_MAX_LINES, FEEDBACK_MAX_BYTES).trim());
        out.push('\n');
    }
    out
}

/// At most this many failing checks in the structured feedback block.
const MAX_FAILURE_ITEMS: usize = 12;

/// Parse a verifier's output into a compact list of the specific failing checks, for common
/// runners (pytest, cargo test, rustc). Returns `None` when nothing recognizable is found,
/// so callers fall back to the raw tail.
fn summarize_failures(output: &str) -> Option<String> {
    let mut items: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for line in output.lines() {
        let t = line.trim();
        let pick = if let Some(rest) = t.strip_prefix("FAILED ") {
            Some(rest.to_string()) // pytest: `FAILED tests/x.py::test_y - AssertionError: …`
        } else if let Some(rest) = t.strip_prefix("E   ") {
            // pytest assertion / error detail lines
            (rest.contains("Error") || rest.contains("assert")).then(|| rest.trim().to_string())
        } else if t.contains(" ... FAILED") {
            Some(t.replace(" ... FAILED", "").trim().to_string()) // cargo: `test name ... FAILED`
        } else if t.contains("panicked at") || t.starts_with("error[") || t.starts_with("error:") {
            Some(t.to_string()) // rust panics / rustc compile errors
        } else {
            None
        };
        if let Some(p) = pick {
            let p = p.trim().to_string();
            if !p.is_empty() && seen.insert(p.clone()) {
                items.push(p);
                if items.len() >= MAX_FAILURE_ITEMS {
                    break;
                }
            }
        }
    }
    (!items.is_empty()).then(|| items.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n"))
}

/// Upper bounds on the verify output fed back to the next iteration. Failures appear at
/// the end of a command's output, so the tail is the useful part; bounding it keeps the
/// feedback — and the tokens it costs the next agent — small.
const FEEDBACK_MAX_LINES: usize = 40;
const FEEDBACK_MAX_BYTES: usize = 2000;

/// Keep the last `max_lines` lines of `text`, also capped to `max_bytes` from the end,
/// prefixed with an elision marker when anything was dropped.
fn tail_lines(text: &str, max_lines: usize, max_bytes: usize) -> String {
    let trimmed = text.trim_end();
    let lines: Vec<&str> = trimmed.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    let mut tail = lines[start..].join("\n");
    let mut truncated = start > 0;

    if tail.len() > max_bytes {
        let mut cut = tail.len() - max_bytes;
        while cut < tail.len() && !tail.is_char_boundary(cut) {
            cut += 1;
        }
        tail = tail[cut..].to_string();
        truncated = true;
    }

    if truncated {
        format!("…(earlier output trimmed)\n{tail}")
    } else {
        tail
    }
}

/// Run a single non-reviewer step: persist its artifacts and detect its changes,
/// without deciding the gate (the caller does).
fn execute_one(
    workspace: &RunWorkspace,
    step: &crate::LoopStep,
    prompt: &str,
    invoker: &dyn Invoker,
    observer: Option<&dyn StepObserver>,
    verify_command: Option<&str>,
) -> io::Result<StepRun> {
    let agent_dir = workspace.agent_dir(step.id, step.role, step.adapter);
    let home = workspace.agent_home(step.id, step.role, step.adapter)?;
    atomic_write(&agent_dir.join("prompt.md"), prompt)?;

    if let Some(observer) = observer {
        observer.on_step_start(step);
    }

    let read_only = read_only_for(step.role);
    let is_command_verify = step.role == Role::Verifier && verify_command.is_some();
    let before_content =
        (!read_only && !is_command_verify).then(|| content_snapshot(&workspace.workspace_dir));

    let started = Instant::now();
    let result = if is_command_verify {
        run_verify_command(verify_command.unwrap(), &workspace.workspace_dir)
    } else {
        let invocation = AgentInvocation {
            adapter: step.adapter,
            role: step.role,
            prompt: prompt.to_string(),
            workspace_dir: workspace.workspace_dir.clone(),
            home_dir: home,
            read_only,
        };
        let before = (!read_only).then(|| snapshot(&workspace.workspace_dir));
        let mut events: Vec<LoopEvent> = Vec::new();
        let mut result = {
            let mut sink = |event: LoopEvent| {
                if let Some(observer) = observer {
                    observer.on_event(&event);
                }
                events.push(event);
            };
            invoker.invoke_streaming(&invocation, &mut sink)
        };
        atomic_write(&agent_dir.join("events.jsonl"), &events_to_jsonl(&events))?;
        if let Some(before) = before
            && result.changed_files.is_empty()
        {
            let after = snapshot(&workspace.workspace_dir);
            result.changed_files = changed_files(&before, &after);
        }
        result
    };
    let duration_ms = started.elapsed().as_millis() as u64;

    atomic_write(&agent_dir.join("transcript.jsonl"), &result.transcript)?;
    atomic_write(&agent_dir.join("result.md"), &render_result(&result))?;

    let changes = if let Some(before_content) = &before_content {
        let real: Vec<FileDiff> =
            compute_file_diffs(&workspace.workspace_dir, &result.changed_files, before_content)
                .into_iter()
                .filter(|d| d.change.binary || d.change.added + d.change.removed > 0)
                .collect();
        if !real.is_empty() {
            atomic_write(&agent_dir.join("changes.diff"), &render_diffs(&real))?;
        }
        real.into_iter().map(|d| d.change).collect()
    } else {
        Vec::new()
    };

    Ok(StepRun {
        result,
        changes,
        duration_ms,
    })
}
/// Reviewer and verifier never write; designer and implementer may.
fn read_only_for(role: Role) -> bool {
    matches!(role, Role::Reviewer | Role::Verifier)
}

/// Run a single reviewer step (read-only), persist its artifacts, and parse its
/// verdict. A review is always a valid artifact, so the gate passes; the verdict
/// carries the blocker signal.
fn run_reviewer(
    workspace: &RunWorkspace,
    step: &crate::LoopStep,
    prompt: &str,
    iteration: usize,
    invoker: &dyn Invoker,
) -> io::Result<StepOutcome> {
    let home = workspace.agent_home(step.id, step.role, step.adapter)?;
    let invocation = AgentInvocation {
        adapter: step.adapter,
        role: step.role,
        prompt: prompt.to_string(),
        workspace_dir: workspace.workspace_dir.clone(),
        home_dir: home,
        read_only: true,
    };
    let mut events: Vec<LoopEvent> = Vec::new();
    let step_started = Instant::now();
    let result = {
        let mut sink = |event: LoopEvent| events.push(event);
        invoker.invoke_streaming(&invocation, &mut sink)
    };
    let duration_ms = step_started.elapsed().as_millis() as u64;

    let agent_dir = workspace.agent_dir(step.id, step.role, step.adapter);
    atomic_write(&agent_dir.join("events.jsonl"), &events_to_jsonl(&events))?;
    atomic_write(&agent_dir.join("transcript.jsonl"), &result.transcript)?;
    atomic_write(&agent_dir.join("result.md"), &render_result(&result))?;

    // A review is a valid artifact whenever it ran; the verdict carries the signal.
    // When the invocation failed there is no verdict to show.
    let (gate_passed, gate_notes, verdict) = if result.success {
        (
            true,
            "review produced".to_string(),
            Some(parse_review_verdict(&result.message)),
        )
    } else {
        (false, "invocation failed".to_string(), None)
    };

    Ok(StepOutcome {
        step_id: step.id,
        role: step.role,
        adapter: step.adapter,
        prompt: prompt.to_string(),
        result,
        gate: step.gate.clone(),
        gate_passed,
        gate_notes,
        review_verdict: verdict,
        changes: Vec::new(),
        duration_ms,
        iteration,
    })
}

/// Run several reviewers concurrently, one scoped thread each. Reviewers are
/// read-only and write to separate per-adapter directories, so there is no contention.
fn run_reviewers_parallel(
    workspace: &RunWorkspace,
    group: &[crate::LoopStep],
    prompts: &[String],
    iteration: usize,
    invoker: &(dyn Invoker + Sync),
) -> io::Result<Vec<StepOutcome>> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = group
            .iter()
            .zip(prompts)
            .map(|(step, prompt)| {
                scope.spawn(move || run_reviewer(workspace, step, prompt, iteration, invoker))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err(io::Error::other("reviewer thread panicked"))
                })
            })
            .collect()
    })
}

/// Run a real shell check command in the workspace as the verifier step.
fn run_verify_command(command: &str, workspace_dir: &Path) -> InvocationResult {
    match Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(workspace_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let success = output.status.success();
            let message = format!(
                "`{command}` exited {} ({})",
                output.status.code().unwrap_or(-1),
                if success { "ok" } else { "failure" }
            );
            InvocationResult {
                success,
                message,
                changed_files: Vec::new(),
                transcript: format!(
                    "$ {command}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n"
                ),
            }
        }
        Err(err) => {
            InvocationResult::failure(format!("failed to run verify command '{command}': {err}"))
        }
    }
}

/// Build a step's prompt: its base role prompt, plus the design contract (when present)
/// and, for a fix iteration, the feedback to address.
fn build_prompt(
    step: &crate::LoopStep,
    requirement: &str,
    feedback: Option<&str>,
    design_contract: Option<&str>,
) -> String {
    let mut prompt = prompt_for_step(step, requirement);

    // The implementer, reviewer, and verifier all work against the design contract.
    if let Some(contract) = design_contract {
        prompt.push_str("\n\n## Design contract\n\n");
        prompt.push_str(contract);
        prompt.push('\n');
        prompt.push_str(match step.role {
            Role::Reviewer => {
                "\nReturn VERDICT: BLOCK if the change does not meet the contract's \
                 acceptance criteria.\n"
            }
            Role::Verifier => "\nVerification should confirm the contract's acceptance criteria.\n",
            _ => "\nImplement against this design contract.\n",
        });
    }

    // A fix iteration's implementer is given the prior failures to resolve.
    if step.role == Role::Implementer
        && let Some(feedback) = feedback
        && !feedback.trim().is_empty()
    {
        prompt.push_str("\n\n## Address these failures from the previous iteration\n\n");
        prompt.push_str(feedback.trim());
        prompt.push('\n');
    }

    prompt
}

/// Render an [`InvocationResult`] as a per-step `result.md`.
fn render_result(result: &InvocationResult) -> String {
    let mut out = String::new();
    out.push_str("# Result\n\n");
    out.push_str(&format!("- Success: {}\n\n", result.success));
    out.push_str("## Message\n\n");
    out.push_str(&result.message);
    out.push('\n');
    if !result.changed_files.is_empty() {
        out.push_str("\n## Changed files\n\n");
        for file in &result.changed_files {
            out.push_str(&format!("- {file}\n"));
        }
    }
    out
}

/// First line of a possibly multi-line message.
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

/// Format a millisecond duration: `m:ss` for ≥1s, else `NNNms`.
fn fmt_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        let secs = ms / 1000;
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Minimal JSON string escaping for hand-rolled `run.json`.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::AgentInvocation;
    use crate::adapter::stub::StubInvoker;
    use crate::engine::workspace::RunWorkspace;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_base(tag: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("loope-exec-{}-{}-{}", tag, std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp base");
        dir
    }

    fn config(max_iters: usize, verify: Option<&str>) -> LoopConfig {
        LoopConfig {
            requirement: "Add login".to_string(),
            include_design: false,
            designer: Adapter::Claude,
            implementer: Adapter::Claude,
            reviewers: vec![Adapter::Codex],
            max_iters,
            verify_command: verify.map(|s| s.to_string()),
        }
    }

    fn run(cfg: &LoopConfig, invoker: &(dyn Invoker + Sync)) -> (LoopRun, RunWorkspace) {
        let base = temp_base("base");
        let source = temp_base("src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false, "demo run").unwrap();
        let run = execute_loop(cfg, &ws, invoker, None).unwrap();
        (run, ws)
    }

    #[test]
    fn cancellation_stops_the_loop() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CancelSoon {
            polls: AtomicUsize,
        }
        impl StepObserver for CancelSoon {
            fn on_step_start(&self, _step: &crate::LoopStep) {}
            fn on_step_finish(&self, _outcome: &StepOutcome) {}
            fn should_cancel(&self) -> bool {
                // Let the first iteration's implement run, then ask to stop.
                self.polls.fetch_add(1, Ordering::Relaxed) >= 1
            }
        }
        let base = temp_base("cancel");
        let source = temp_base("cancelsrc");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false, "demo").unwrap();
        let observer = CancelSoon { polls: AtomicUsize::new(0) };
        let run = execute_loop(&config(3, None), &ws, &StubInvoker, Some(&observer)).unwrap();
        assert_eq!(run.stop_reason, StopReason::Cancelled);
        assert!(!run.all_passed());
        let _ = fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn converges_in_one_iteration() {
        let (run, ws) = run(&config(3, None), &StubInvoker);
        assert_eq!(run.stop_reason, StopReason::Converged);
        assert_eq!(run.iterations, 1);
        assert!(run.all_passed());
        // implement (1) + review (2) only
        assert!(ws.agent_dir(1, Role::Implementer, Adapter::Claude).join("result.md").exists());
        assert!(ws.agent_dir(2, Role::Reviewer, Adapter::Codex).join("result.md").exists());
        // the cumulative diff captured the stub's new file and a changed-files listing
        assert!(run.changed.iter().any(|c| c.path == "IMPLEMENTATION_NOTES.md"));
        assert!(ws.root.join("changes.diff").exists());
        let listing = fs::read_to_string(ws.root.join("changed-files.txt")).unwrap();
        assert!(listing.contains("IMPLEMENTATION_NOTES.md"));
        let _ = fs::remove_dir_all(ws.root.parent().unwrap().parent().unwrap());
    }

    /// Reviewer blocks on iteration 1 and passes on iteration 2.
    struct BlockThenPass {
        reviews: Mutex<usize>,
    }
    impl Invoker for BlockThenPass {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Reviewer {
                let mut n = self.reviews.lock().unwrap();
                *n += 1;
                let verdict = if *n == 1 { "VERDICT: BLOCK" } else { "VERDICT: PASS" };
                return InvocationResult {
                    success: true,
                    message: format!("blocker number {n}\n{verdict}"),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                };
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn iterates_until_review_passes_and_feeds_back() {
        let invoker = BlockThenPass { reviews: Mutex::new(0) };
        let (run, ws) = run(&config(3, None), &invoker);
        assert_eq!(run.stop_reason, StopReason::Converged);
        assert_eq!(run.iterations, 2);
        // iteration 2's implement step is step 3; its prompt carries the blocker feedback.
        let p = fs::read_to_string(
            ws.agent_dir(3, Role::Implementer, Adapter::Claude).join("prompt.md"),
        )
        .unwrap();
        assert!(p.contains("Address these failures"));
        assert!(p.contains("blocker number 1"));
        let _ = fs::remove_dir_all(ws.root.parent().unwrap().parent().unwrap());
    }

    /// Reviewer always blocks.
    struct AlwaysBlock;
    impl Invoker for AlwaysBlock {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Reviewer {
                return InvocationResult {
                    success: true,
                    message: "still broken\nVERDICT: BLOCK".to_string(),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                };
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn stalls_when_no_progress_is_made() {
        // The reviewer always blocks and the stub implementer repeats the same change, so the
        // second iteration produces no new change → stop early as Stalled, not MaxIters.
        let (run, _ws) = run(&config(3, None), &AlwaysBlock);
        assert_eq!(run.stop_reason, StopReason::Stalled);
        assert!(run.iterations < 3);
        assert!(!run.all_passed());
    }

    /// Reviewer always blocks; the implementer makes a *new* change each iteration, so the
    /// loop genuinely progresses without converging and reaches the iteration cap.
    struct ProgressingBlock {
        n: std::sync::atomic::AtomicUsize,
    }
    impl Invoker for ProgressingBlock {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Reviewer {
                return InvocationResult {
                    success: true,
                    message: "not yet\nVERDICT: BLOCK".to_string(),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                };
            }
            let n = self.n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let _ = std::fs::write(inv.workspace_dir.join(format!("progress-{n}.txt")), n.to_string());
            InvocationResult {
                success: true,
                message: "made a change".to_string(),
                changed_files: vec![format!("progress-{n}.txt")],
                transcript: String::new(),
            }
        }
    }

    #[test]
    fn reaches_max_iters_when_each_iteration_progresses() {
        let invoker = ProgressingBlock { n: std::sync::atomic::AtomicUsize::new(0) };
        let (run, _ws) = run(&config(2, None), &invoker);
        assert_eq!(run.stop_reason, StopReason::MaxIters);
        assert_eq!(run.iterations, 2);
        assert!(!run.all_passed());
    }

    /// Implementer fails immediately.
    struct FailImplement;
    impl Invoker for FailImplement {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Implementer {
                return InvocationResult::failure("implementer crashed");
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn hard_failure_halts_immediately() {
        let (run, _ws) = run(&config(3, None), &FailImplement);
        assert_eq!(run.stop_reason, StopReason::StepFailed);
        assert_eq!(run.iterations, 1);
        assert!(!run.all_passed());
    }

    #[test]
    fn summarize_failures_extracts_pytest_and_cargo_failures() {
        let pytest = "collected 3 items\n\
            tests/test_x.py::test_a PASSED\n\
            tests/test_x.py::test_b FAILED\n\
            =========== FAILURES ===========\n\
            E   AssertionError: expected 0 got nan\n\
            FAILED tests/test_x.py::test_b - AssertionError: expected 0 got nan\n";
        let s = summarize_failures(pytest).unwrap();
        assert!(s.contains("- tests/test_x.py::test_b - AssertionError: expected 0 got nan"));
        assert!(s.contains("- AssertionError: expected 0 got nan"));

        let cargo = "running 2 tests\n\
            test add ... ok\n\
            test saturates ... FAILED\n\
            thread 'saturates' panicked at src/lib.rs:3:5:\n";
        let c = summarize_failures(cargo).unwrap();
        assert!(c.contains("- test saturates"));
        assert!(c.contains("panicked at src/lib.rs:3:5"));

        // Unrecognized output → None (callers fall back to the tail).
        assert_eq!(summarize_failures("everything is fine\nall green"), None);
    }

    #[test]
    fn compose_feedback_leads_with_failing_checks() {
        let fb = compose_feedback(
            &["Codex: missing edge case".to_string()],
            Some("FAILED tests/x.py::test_y - ValueError: boom\nsome trailing log"),
        );
        let lead = fb.find("These checks are failing");
        let tail = fb.find("Verify command output (tail):");
        assert!(lead.is_some() && tail.is_some());
        assert!(lead < tail, "failing-checks block must come before the tail");
        assert!(fb.contains("- tests/x.py::test_y - ValueError: boom"));
        assert!(fb.contains("missing edge case"));
    }

    #[test]
    fn tail_lines_keeps_the_end_and_marks_truncation() {
        // Short text passes through untouched.
        assert_eq!(tail_lines("a\nb\nc", 10, 1000), "a\nb\nc");

        // More lines than the cap keeps the last ones with a marker.
        let many = (1..=100).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        let tail = tail_lines(&many, 3, 1000);
        assert!(tail.starts_with("…(earlier output trimmed)"));
        assert!(tail.ends_with("98\n99\n100"));
        assert!(!tail.contains("\n1\n"));

        // The byte cap also truncates and stays on a char boundary (no panic on UTF-8).
        let wide = "é".repeat(5000);
        let capped = tail_lines(&wide, 10, 100);
        assert!(capped.len() <= 100 + "…(earlier output trimmed)\n".len() + 1);
    }

    #[test]
    fn verify_command_drives_convergence() {
        let (pass, _) = run(&config(2, Some("exit 0")), &StubInvoker);
        assert_eq!(pass.stop_reason, StopReason::Converged);
        assert_eq!(pass.iterations, 1);

        // A verify that always fails the same way with no new change → stalls (no progress),
        // rather than pointlessly running to the iteration cap.
        let (fail, _) = run(&config(3, Some("exit 1")), &StubInvoker);
        assert_eq!(fail.stop_reason, StopReason::Stalled);
        assert!(!fail.all_passed());
    }

    #[test]
    fn design_only_run_writes_contract() {
        let mut cfg = config(0, None);
        cfg.include_design = true;
        let (run, ws) = run(&cfg, &StubInvoker);
        assert_eq!(run.stop_reason, StopReason::DesignOnly);
        assert!(run.all_passed());
        assert!(ws.root.join("design-contract.md").exists());
        assert!(ws.agent_dir(1, Role::Designer, Adapter::Claude).join("result.md").exists());
    }
}
