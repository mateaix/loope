//! Loop executor: walks a [`LoopPlan`], runs each step through an [`Invoker`],
//! passes artifacts forward, evaluates gates, and persists a run.

use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::adapter::{AgentInvocation, InvocationResult, Invoker};
use crate::event::{LoopEvent, events_to_jsonl};
use crate::review::{ReviewVerdict, parse_review_verdict};
use crate::workspace::{
    FileChange, FileDiff, RunWorkspace, atomic_write, changed_files, compute_file_diffs,
    content_snapshot, render_diffs, snapshot,
};
use crate::{Adapter, LoopPlan, Role, prompt_for_step};

/// Options controlling how a loop executes.
#[derive(Clone, Debug, Default)]
pub struct ExecuteOptions {
    /// When set, the verifier step runs this shell command in the workspace instead
    /// of invoking an agent; its gate passes iff the command exits zero.
    pub verify_command: Option<String>,
}

/// Observes step execution so a frontend can render live progress.
pub trait StepObserver {
    /// Called just before a step's agent (or verify command) runs.
    fn on_step_start(&self, step: &crate::LoopStep);
    /// Called for each normalized event a step emits while it runs. Default no-op.
    fn on_event(&self, _event: &crate::event::LoopEvent) {}
    /// Called once a step's outcome (including its gate result) is known.
    fn on_step_finish(&self, outcome: &StepOutcome);
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
}

/// The result of executing a whole loop.
#[derive(Clone, Debug)]
pub struct LoopRun {
    pub run_id: String,
    pub requirement: String,
    pub outcomes: Vec<StepOutcome>,
    /// True if the loop stopped early on a blocking gate.
    pub halted: bool,
    /// Total wall-clock time of the run, in milliseconds.
    pub total_ms: u64,
}

impl LoopRun {
    /// True only if the loop ran every step and each gate passed.
    pub fn all_passed(&self) -> bool {
        !self.halted && self.outcomes.iter().all(|o| o.gate_passed)
    }

    /// Human-readable final loop report.
    pub fn to_report_markdown(&self) -> String {
        let outcome = if self.all_passed() {
            "all gates passed"
        } else if self.halted {
            "halted on a blocking gate"
        } else {
            "completed with gate failures"
        };

        let mut out = String::new();
        out.push_str("# Loope Run Report\n\n");
        out.push_str(&format!("- Run: {}\n", self.run_id));
        out.push_str(&format!("- Requirement: {}\n", self.requirement));
        out.push_str(&format!("- Outcome: {outcome}\n"));
        out.push_str(&format!("- Took: {}\n\n", fmt_duration(self.total_ms)));
        out.push_str("## Steps\n\n");

        for o in &self.outcomes {
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
            if !o.changes.is_empty() {
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
            } else if !o.result.changed_files.is_empty() {
                out.push_str(&format!(
                    "   - Changed files: {}\n",
                    o.result.changed_files.join(", ")
                ));
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
                    "{{\"id\":{},\"role\":\"{}\",\"adapter\":\"{}\",\"gate_passed\":{},\"success\":{}}}",
                    o.step_id,
                    o.role.as_str(),
                    o.adapter.as_str(),
                    o.gate_passed,
                    o.result.success
                )
            })
            .collect();

        format!(
            "{{\"run_id\":\"{}\",\"requirement\":\"{}\",\"all_passed\":{},\"halted\":{},\"steps\":[{}]}}\n",
            json_escape(&self.run_id),
            json_escape(&self.requirement),
            self.all_passed(),
            self.halted,
            steps.join(",")
        )
    }
}

/// Execute `plan` step by step in `workspace`, driving each step through `invoker`.
/// Persists per-step prompt/transcript/result plus `report.md` and `run.json`, and
/// stops at the first blocking gate.
pub fn execute_plan(
    plan: &LoopPlan,
    workspace: &RunWorkspace,
    invoker: &(dyn Invoker + Sync),
    options: &ExecuteOptions,
    observer: Option<&dyn StepObserver>,
) -> io::Result<LoopRun> {
    atomic_write(&workspace.root.join("plan.md"), &plan.to_markdown())?;

    let run_started = Instant::now();
    let mut outcomes = Vec::new();
    let mut halted = false;

    // Artifacts passed forward between steps.
    let mut last_implementer_message: Option<String> = None;
    let mut last_implementer_files: Vec<String> = Vec::new();
    let mut last_review_message: Option<String> = None;
    let mut review_blockers_pending = false;
    let mut design_contract: Option<String> = None;

    let steps = &plan.steps;
    let mut i = 0;
    while i < steps.len() {
        // A maximal run of consecutive reviewer steps forms one review phase that runs
        // its reviewers concurrently and aggregates their verdicts.
        if steps[i].role == Role::Reviewer {
            let start = i;
            while i < steps.len() && steps[i].role == Role::Reviewer {
                i += 1;
            }
            let group = &steps[start..i];

            let prompts: Vec<String> = group
                .iter()
                .map(|s| {
                    build_prompt(
                        s.role,
                        &plan.requirement,
                        s,
                        last_implementer_message.as_deref(),
                        &last_implementer_files,
                        last_review_message.as_deref(),
                        design_contract.as_deref(),
                    )
                })
                .collect();
            for (s, prompt) in group.iter().zip(&prompts) {
                workspace.agent_home(s.role, s.adapter)?;
                atomic_write(&workspace.agent_dir(s.role, s.adapter).join("prompt.md"), prompt)?;
            }

            let group_outcomes = if group.len() == 1 {
                if let Some(observer) = observer {
                    observer.on_step_start(&group[0]);
                }
                vec![run_reviewer(workspace, &group[0], &prompts[0], invoker)?]
            } else {
                run_reviewers_parallel(workspace, group, &prompts, invoker)?
            };

            // Aggregate: any reviewer with blockers means blockers are pending.
            review_blockers_pending = false;
            let mut summaries = Vec::new();
            let mut group_halt = false;
            for outcome in group_outcomes {
                if let Some(verdict) = &outcome.review_verdict {
                    if verdict.has_blockers {
                        review_blockers_pending = true;
                    }
                    summaries.push(format!(
                        "{} ({}): {}",
                        outcome.adapter.display_name(),
                        verdict.label(),
                        verdict.summary
                    ));
                }
                if !outcome.gate_passed {
                    group_halt = true;
                }
                if let Some(observer) = observer {
                    observer.on_step_finish(&outcome);
                }
                outcomes.push(outcome);
            }
            last_review_message = Some(format!("Reviews:\n{}", summaries.join("\n")));
            if group_halt {
                halted = true;
                break;
            }
            continue;
        }

        let step = &steps[i];
        i += 1;

        let prompt = build_prompt(
            step.role,
            &plan.requirement,
            step,
            last_implementer_message.as_deref(),
            &last_implementer_files,
            last_review_message.as_deref(),
            design_contract.as_deref(),
        );

        let agent_dir = workspace.agent_dir(step.role, step.adapter);
        let home = workspace.agent_home(step.role, step.adapter)?;
        atomic_write(&agent_dir.join("prompt.md"), &prompt)?;

        if let Some(observer) = observer {
            observer.on_step_start(step);
        }

        let read_only = read_only_for(step.role);
        let is_command_verify = step.role == Role::Verifier && options.verify_command.is_some();
        // For write-capable agent steps, capture content up front so we can diff.
        let before_content = (!read_only && !is_command_verify)
            .then(|| content_snapshot(&workspace.workspace_dir));

        let step_started = Instant::now();
        let result = if is_command_verify {
            // A real check command stands in for the verifier agent.
            run_verify_command(
                options.verify_command.as_deref().unwrap(),
                &workspace.workspace_dir,
            )
        } else {
            let invocation = AgentInvocation {
                adapter: step.adapter,
                role: step.role,
                prompt: prompt.clone(),
                workspace_dir: workspace.workspace_dir.clone(),
                home_dir: home,
                read_only,
            };
            // For write-capable steps, detect file changes by diffing the workspace,
            // since the CLI cannot reliably self-report what it changed.
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

        atomic_write(&agent_dir.join("transcript.jsonl"), &result.transcript)?;
        atomic_write(&agent_dir.join("result.md"), &render_result(&result))?;

        // Turn the workspace changes into per-file diffs for the report and `show`.
        // Keep only files whose content actually changed, so a no-op revise turn
        // doesn't overwrite an earlier turn's diff with an empty one.
        let changes = if let Some(before_content) = &before_content {
            let real: Vec<FileDiff> = compute_file_diffs(
                &workspace.workspace_dir,
                &result.changed_files,
                before_content,
            )
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

        // A successful design step yields the Design Contract: persist it as a run
        // artifact and inside the workspace, and feed it to downstream prompts.
        if step.role == Role::Designer && result.success && !result.message.trim().is_empty() {
            let contract = result.message.clone();
            atomic_write(&workspace.root.join("design-contract.md"), &contract)?;
            atomic_write(
                &workspace.workspace_dir.join("DESIGN_CONTRACT.md"),
                &contract,
            )?;
            design_contract = Some(contract);
        }

        // A second implementer turn that follows a review is a revision. If the review
        // raised no blockers it may legitimately make no change; if it raised blockers,
        // a no-op revise turn means they were not addressed.
        let is_revision = step.role == Role::Implementer && last_review_message.is_some();
        let (gate_passed, gate_notes) =
            evaluate_gate(step.role, &result, is_revision, review_blockers_pending);

        // Record artifacts for downstream steps before moving `result`.
        if step.role == Role::Implementer {
            last_implementer_message = Some(result.message.clone());
            if !result.changed_files.is_empty() {
                last_implementer_files = result.changed_files.clone();
                if is_revision {
                    // Blockers addressed once a revise turn produced a change.
                    review_blockers_pending = false;
                }
            }
        }

        let outcome = StepOutcome {
            step_id: step.id,
            role: step.role,
            adapter: step.adapter,
            prompt,
            result,
            gate: step.gate.clone(),
            gate_passed,
            gate_notes,
            review_verdict: None,
            changes,
            duration_ms: step_started.elapsed().as_millis() as u64,
        };
        if let Some(observer) = observer {
            observer.on_step_finish(&outcome);
        }
        outcomes.push(outcome);

        if !gate_passed {
            halted = true;
            break;
        }
    }

    let run = LoopRun {
        run_id: workspace.run_id.clone(),
        requirement: plan.requirement.clone(),
        outcomes,
        halted,
        total_ms: run_started.elapsed().as_millis() as u64,
    };

    atomic_write(&workspace.root.join("report.md"), &run.to_report_markdown())?;
    atomic_write(&workspace.root.join("run.json"), &run.to_run_json())?;

    Ok(run)
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
    invoker: &dyn Invoker,
) -> io::Result<StepOutcome> {
    let home = workspace.agent_home(step.role, step.adapter)?;
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

    let agent_dir = workspace.agent_dir(step.role, step.adapter);
    atomic_write(&agent_dir.join("events.jsonl"), &events_to_jsonl(&events))?;
    atomic_write(&agent_dir.join("transcript.jsonl"), &result.transcript)?;
    atomic_write(&agent_dir.join("result.md"), &render_result(&result))?;

    let verdict = parse_review_verdict(&result.message);
    let (gate_passed, gate_notes) = evaluate_gate(Role::Reviewer, &result, false, false);

    Ok(StepOutcome {
        step_id: step.id,
        role: step.role,
        adapter: step.adapter,
        prompt: prompt.to_string(),
        result,
        gate: step.gate.clone(),
        gate_passed,
        gate_notes,
        review_verdict: Some(verdict),
        changes: Vec::new(),
        duration_ms,
    })
}

/// Run several reviewers concurrently, one scoped thread each. Reviewers are
/// read-only and write to separate per-adapter directories, so there is no contention.
fn run_reviewers_parallel(
    workspace: &RunWorkspace,
    group: &[crate::LoopStep],
    prompts: &[String],
    invoker: &(dyn Invoker + Sync),
) -> io::Result<Vec<StepOutcome>> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = group
            .iter()
            .zip(prompts)
            .map(|(step, prompt)| scope.spawn(move || run_reviewer(workspace, step, prompt, invoker)))
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

/// Build a step's prompt from its base prompt plus the relevant upstream artifacts.
#[allow(clippy::too_many_arguments)]
fn build_prompt(
    role: Role,
    requirement: &str,
    step: &crate::LoopStep,
    last_implementer_message: Option<&str>,
    last_implementer_files: &[String],
    last_review_message: Option<&str>,
    design_contract: Option<&str>,
) -> String {
    let mut prompt = prompt_for_step(step, requirement);

    // The implementer and reviewer work against the design contract when present.
    if matches!(role, Role::Implementer | Role::Reviewer)
        && let Some(contract) = design_contract
    {
        prompt.push_str("\n\n## Design contract\n\n");
        prompt.push_str(contract);
        prompt.push('\n');
        prompt.push_str(if role == Role::Reviewer {
            "\nCheck the implementation for consistency with this design contract.\n"
        } else {
            "\nImplement against this design contract.\n"
        });
    }

    match role {
        Role::Reviewer => {
            if let Some(msg) = last_implementer_message {
                prompt.push_str("\n\n## Implementer result to review\n\n");
                prompt.push_str(msg);
                prompt.push('\n');
                if !last_implementer_files.is_empty() {
                    prompt.push_str("\nChanged files:\n");
                    for file in last_implementer_files {
                        prompt.push_str(&format!("- {file}\n"));
                    }
                }
            }
        }
        Role::Implementer => {
            if let Some(review) = last_review_message {
                prompt.push_str("\n\n## Review findings to address\n\n");
                prompt.push_str(review);
                prompt.push('\n');
            }
        }
        Role::Verifier => {
            if let Some(msg) = last_implementer_message {
                prompt.push_str("\n\n## Final implementation summary\n\n");
                prompt.push_str(msg);
                prompt.push('\n');
            }
        }
        Role::Designer => {}
    }

    prompt
}

/// Evaluate a step's gate against its real result. `is_revision` marks an implementer
/// turn that follows a review; `blockers_pending` is true when the review found
/// blocking issues that the revise turn is expected to address.
fn evaluate_gate(
    role: Role,
    result: &InvocationResult,
    is_revision: bool,
    blockers_pending: bool,
) -> (bool, String) {
    if !result.success {
        return (false, "invocation failed".to_string());
    }
    if result.message.trim().is_empty() {
        return (false, "no artifact produced".to_string());
    }
    match role {
        Role::Implementer => {
            if !result.changed_files.is_empty() {
                let notes = if is_revision {
                    "revision addressed review"
                } else {
                    "scoped change produced"
                };
                (true, notes.to_string())
            } else if is_revision {
                if blockers_pending {
                    (false, "blocking review findings not addressed".to_string())
                } else {
                    (true, "no revision needed".to_string())
                }
            } else {
                (false, "implementer reported no change".to_string())
            }
        }
        Role::Designer => (true, "design contract produced".to_string()),
        Role::Reviewer => (true, "review produced".to_string()),
        Role::Verifier => (true, "verification reported".to_string()),
    }
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
    use crate::stub::StubInvoker;
    use crate::workspace::RunWorkspace;
    use crate::{LoopOptions, generate_plan};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::fs;
    use std::path::PathBuf;

    fn temp_base(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("loope-exec-{}-{}-{}", tag, std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp base");
        dir
    }

    #[test]
    fn full_loop_passes_and_persists_artifacts() {
        let base = temp_base("full");
        let source = temp_base("full-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let run = execute_plan(&plan, &ws, &StubInvoker, &ExecuteOptions::default(), None).unwrap();

        assert!(run.all_passed());
        assert!(!run.halted);
        assert_eq!(run.outcomes.len(), plan.steps.len());
        // run artifacts on disk
        assert!(ws.root.join("plan.md").exists());
        assert!(ws.root.join("report.md").exists());
        assert!(ws.root.join("run.json").exists());
        let report = fs::read_to_string(ws.root.join("report.md")).unwrap();
        assert!(report.contains("all gates passed"));
        // per-agent files exist
        let impl_dir = ws.agent_dir(Role::Implementer, Adapter::Claude);
        assert!(impl_dir.join("prompt.md").exists());
        assert!(impl_dir.join("result.md").exists());

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn design_run_writes_contract_and_feeds_implementer() {
        let base = temp_base("design");
        let source = temp_base("design-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan(
            "Build dashboard",
            LoopOptions {
                include_design: true,
                ..LoopOptions::default()
            },
        );

        let run = execute_plan(&plan, &ws, &StubInvoker, &ExecuteOptions::default(), None).unwrap();
        assert!(run.all_passed());

        // contract persisted as a run artifact and inside the workspace
        assert!(ws.root.join("design-contract.md").exists());
        assert!(ws.workspace_dir.join("DESIGN_CONTRACT.md").exists());

        // the implementer's prompt was given the contract
        let impl_prompt =
            fs::read_to_string(ws.agent_dir(Role::Implementer, Adapter::Claude).join("prompt.md"))
                .unwrap();
        assert!(impl_prompt.contains("## Design contract"));
        assert!(impl_prompt.contains("Implement against this design contract"));

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn reviewer_prompt_includes_implementer_artifact() {
        let base = temp_base("artifact");
        let source = temp_base("artifact-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        execute_plan(&plan, &ws, &StubInvoker, &ExecuteOptions::default(), None).unwrap();

        let reviewer_prompt =
            fs::read_to_string(ws.agent_dir(Role::Reviewer, Adapter::Codex).join("prompt.md"))
                .unwrap();
        assert!(reviewer_prompt.contains("Implementer result to review"));
        assert!(reviewer_prompt.contains("IMPLEMENTATION_NOTES.md"));

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    /// An invoker that fails the reviewer step to exercise the blocking-gate halt.
    struct FailReviewerInvoker {
        seen: Mutex<Vec<Role>>,
    }

    impl Invoker for FailReviewerInvoker {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            self.seen.lock().unwrap().push(inv.role);
            if inv.role == Role::Reviewer {
                return InvocationResult::failure("reviewer could not run");
            }
            StubInvoker.invoke(inv)
        }
    }

    /// Reviewer flags blockers; the implementer's revise turn makes no change.
    struct BlockNoFixInvoker {
        implementer_calls: AtomicUsize,
    }

    impl Invoker for BlockNoFixInvoker {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            match inv.role {
                Role::Reviewer => InvocationResult {
                    success: true,
                    message: "A blocking issue exists.\nVERDICT: BLOCK".to_string(),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                },
                Role::Implementer => {
                    let calls = self.implementer_calls.fetch_add(1, Ordering::Relaxed);
                    if calls == 0 {
                        StubInvoker.invoke(inv)
                    } else {
                        InvocationResult {
                            success: true,
                            message: "I did not change anything.".to_string(),
                            changed_files: Vec::new(),
                            transcript: String::new(),
                        }
                    }
                }
                _ => StubInvoker.invoke(inv),
            }
        }
    }

    #[test]
    fn revise_blocks_when_blockers_not_addressed() {
        let base = temp_base("block-nofix");
        let source = temp_base("block-nofix-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let invoker = BlockNoFixInvoker {
            implementer_calls: AtomicUsize::new(0),
        };
        let run = execute_plan(&plan, &ws, &invoker, &ExecuteOptions::default(), None).unwrap();

        // reviewer recorded blockers
        let reviewer = &run.outcomes[1];
        assert_eq!(reviewer.role, Role::Reviewer);
        assert!(reviewer.review_verdict.as_ref().unwrap().has_blockers);
        // revise turn made no change while blockers were pending -> blocked
        assert!(run.halted);
        let revise = &run.outcomes[2];
        assert!(!revise.gate_passed);
        assert_eq!(revise.gate_notes, "blocking review findings not addressed");

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    /// Reviewer flags blockers; the implementer's revise turn makes a change (stub
    /// implementer always writes a file), so the loop proceeds.
    struct BlockThenFixInvoker;

    impl Invoker for BlockThenFixInvoker {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Reviewer {
                return InvocationResult {
                    success: true,
                    message: "Fix needed.\nVERDICT: BLOCK".to_string(),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                };
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn revise_passes_when_blockers_addressed() {
        let base = temp_base("block-fix");
        let source = temp_base("block-fix-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let run =
            execute_plan(&plan, &ws, &BlockThenFixInvoker, &ExecuteOptions::default(), None).unwrap();

        assert!(run.all_passed());
        let revise = &run.outcomes[2];
        assert!(revise.gate_passed);
        assert_eq!(revise.gate_notes, "revision addressed review");

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    /// Reviewer that emits BLOCK for Codex and PASS for Claude, to check aggregation.
    struct PerAdapterReviewInvoker;

    impl Invoker for PerAdapterReviewInvoker {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Reviewer {
                let verdict = if inv.adapter == Adapter::Codex {
                    "VERDICT: BLOCK"
                } else {
                    "VERDICT: PASS"
                };
                return InvocationResult {
                    success: true,
                    message: format!("Review from {}.\n{verdict}", inv.adapter.display_name()),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                };
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn two_reviewers_run_and_aggregate() {
        let base = temp_base("multi");
        let source = temp_base("multi-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan(
            "Add login",
            LoopOptions {
                reviewers: vec![Adapter::Codex, Adapter::Claude],
                ..LoopOptions::default()
            },
        );

        let run = execute_plan(
            &plan,
            &ws,
            &PerAdapterReviewInvoker,
            &ExecuteOptions::default(),
            None,
        )
        .unwrap();

        // both reviewers produced artifacts and verdicts
        let reviewers: Vec<&StepOutcome> = run
            .outcomes
            .iter()
            .filter(|o| o.role == Role::Reviewer)
            .collect();
        assert_eq!(reviewers.len(), 2);
        assert!(
            ws.agent_dir(Role::Reviewer, Adapter::Codex)
                .join("result.md")
                .exists()
        );
        assert!(
            ws.agent_dir(Role::Reviewer, Adapter::Claude)
                .join("result.md")
                .exists()
        );
        // Codex blocked; aggregation marks blockers pending, so the stub revise turn
        // (which writes a file) addresses them and the loop still passes.
        let codex = reviewers.iter().find(|o| o.adapter == Adapter::Codex).unwrap();
        assert!(codex.review_verdict.as_ref().unwrap().has_blockers);
        assert!(run.all_passed());

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn reviewer_outcome_carries_pass_verdict() {
        let base = temp_base("verdict");
        let source = temp_base("verdict-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let run = execute_plan(&plan, &ws, &StubInvoker, &ExecuteOptions::default(), None).unwrap();
        let reviewer = &run.outcomes[1];
        let verdict = reviewer.review_verdict.as_ref().unwrap();
        assert!(!verdict.has_blockers);
        assert_eq!(verdict.label(), "PASS");

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn command_verifier_passes_when_command_succeeds() {
        let base = temp_base("verify-ok");
        let source = temp_base("verify-ok-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let options = ExecuteOptions {
            verify_command: Some("exit 0".to_string()),
        };
        let run = execute_plan(&plan, &ws, &StubInvoker, &options, None).unwrap();

        assert!(run.all_passed());
        let verifier = run.outcomes.last().unwrap();
        assert_eq!(verifier.role, Role::Verifier);
        assert!(verifier.gate_passed);
        assert!(verifier.result.message.contains("exited 0"));

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn command_verifier_blocks_when_command_fails() {
        let base = temp_base("verify-fail");
        let source = temp_base("verify-fail-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let options = ExecuteOptions {
            verify_command: Some("exit 1".to_string()),
        };
        let run = execute_plan(&plan, &ws, &StubInvoker, &options, None).unwrap();

        assert!(!run.all_passed());
        assert!(run.halted);
        let verifier = run.outcomes.last().unwrap();
        assert_eq!(verifier.role, Role::Verifier);
        assert!(!verifier.gate_passed);

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    /// Implementer changes a file on the first turn but makes no change on the
    /// revise turn (nothing to fix). The reviewer/verifier defer to the stub.
    struct NoRevisionInvoker {
        implementer_calls: AtomicUsize,
    }

    impl Invoker for NoRevisionInvoker {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            if inv.role == Role::Implementer {
                let calls = self.implementer_calls.fetch_add(1, Ordering::Relaxed);
                if calls == 0 {
                    return StubInvoker.invoke(inv); // first turn writes a file
                }
                // revise turn: nothing to change
                return InvocationResult {
                    success: true,
                    message: "No changes needed; the review raised no blockers.".to_string(),
                    changed_files: Vec::new(),
                    transcript: String::new(),
                };
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn revise_turn_passes_when_no_change_is_needed() {
        let base = temp_base("norev");
        let source = temp_base("norev-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let invoker = NoRevisionInvoker {
            implementer_calls: AtomicUsize::new(0),
        };
        let run = execute_plan(&plan, &ws, &invoker, &ExecuteOptions::default(), None).unwrap();

        assert!(run.all_passed(), "revise turn with no change should not block");
        let revise = &run.outcomes[2];
        assert_eq!(revise.role, Role::Implementer);
        assert!(revise.gate_passed);
        assert_eq!(revise.gate_notes, "no revision needed");

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn blocking_gate_halts_the_loop() {
        let base = temp_base("halt");
        let source = temp_base("halt-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let invoker = FailReviewerInvoker {
            seen: Mutex::new(Vec::new()),
        };
        let run = execute_plan(&plan, &ws, &invoker, &ExecuteOptions::default(), None).unwrap();

        assert!(run.halted);
        assert!(!run.all_passed());
        // implementer + reviewer ran; the loop stopped before the revise step
        let seen = invoker.seen.lock().unwrap();
        assert_eq!(seen.as_slice(), &[Role::Implementer, Role::Reviewer]);
        let report = fs::read_to_string(ws.root.join("report.md")).unwrap();
        assert!(report.contains("halted on a blocking gate"));

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }
}
