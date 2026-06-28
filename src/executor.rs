//! Loop executor: walks a [`LoopPlan`], runs each step through an [`Invoker`],
//! passes artifacts forward, evaluates gates, and persists a run.

use std::io;

use crate::adapter::{AgentInvocation, InvocationResult, Invoker};
use crate::workspace::{RunWorkspace, atomic_write};
use crate::{Adapter, LoopPlan, Role, prompt_for_step};

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
}

/// The result of executing a whole loop.
#[derive(Clone, Debug)]
pub struct LoopRun {
    pub run_id: String,
    pub requirement: String,
    pub outcomes: Vec<StepOutcome>,
    /// True if the loop stopped early on a blocking gate.
    pub halted: bool,
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
        out.push_str(&format!("- Outcome: {outcome}\n\n"));
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
            out.push_str(&format!("   - Gate result: {}\n", o.gate_notes));
            out.push_str(&format!("   - Message: {}\n", first_line(&o.result.message)));
            if !o.result.changed_files.is_empty() {
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
    invoker: &dyn Invoker,
) -> io::Result<LoopRun> {
    atomic_write(&workspace.root.join("plan.md"), &plan.to_markdown())?;

    let mut outcomes = Vec::new();
    let mut halted = false;

    // Artifacts passed forward between steps.
    let mut last_implementer_message: Option<String> = None;
    let mut last_implementer_files: Vec<String> = Vec::new();
    let mut last_review_message: Option<String> = None;

    for step in &plan.steps {
        let prompt = build_prompt(
            step.role,
            &plan.requirement,
            step,
            last_implementer_message.as_deref(),
            &last_implementer_files,
            last_review_message.as_deref(),
        );

        let agent_dir = workspace.agent_dir(step.role, step.adapter);
        let home = workspace.agent_home(step.role, step.adapter)?;
        atomic_write(&agent_dir.join("prompt.md"), &prompt)?;

        let invocation = AgentInvocation {
            adapter: step.adapter,
            role: step.role,
            prompt: prompt.clone(),
            workspace_dir: workspace.workspace_dir.clone(),
            home_dir: home,
            read_only: read_only_for(step.role),
        };
        let result = invoker.invoke(&invocation);

        atomic_write(&agent_dir.join("transcript.jsonl"), &result.transcript)?;
        atomic_write(&agent_dir.join("result.md"), &render_result(&result))?;

        let (gate_passed, gate_notes) = evaluate_gate(step.role, &result);

        // Record artifacts for downstream steps before moving `result`.
        match step.role {
            Role::Implementer => {
                last_implementer_message = Some(result.message.clone());
                if !result.changed_files.is_empty() {
                    last_implementer_files = result.changed_files.clone();
                }
            }
            Role::Reviewer => last_review_message = Some(result.message.clone()),
            _ => {}
        }

        outcomes.push(StepOutcome {
            step_id: step.id,
            role: step.role,
            adapter: step.adapter,
            prompt,
            result,
            gate: step.gate.clone(),
            gate_passed,
            gate_notes,
        });

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
    };

    atomic_write(&workspace.root.join("report.md"), &run.to_report_markdown())?;
    atomic_write(&workspace.root.join("run.json"), &run.to_run_json())?;

    Ok(run)
}

/// Reviewer and verifier never write; designer and implementer may.
fn read_only_for(role: Role) -> bool {
    matches!(role, Role::Reviewer | Role::Verifier)
}

/// Build a step's prompt from its base prompt plus the relevant upstream artifacts.
fn build_prompt(
    role: Role,
    requirement: &str,
    step: &crate::LoopStep,
    last_implementer_message: Option<&str>,
    last_implementer_files: &[String],
    last_review_message: Option<&str>,
) -> String {
    let mut prompt = prompt_for_step(step, requirement);

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

/// Evaluate a step's gate against its real result.
fn evaluate_gate(role: Role, result: &InvocationResult) -> (bool, String) {
    if !result.success {
        return (false, "invocation failed".to_string());
    }
    if result.message.trim().is_empty() {
        return (false, "no artifact produced".to_string());
    }
    match role {
        Role::Implementer => {
            if result.changed_files.is_empty() {
                (false, "implementer reported no change".to_string())
            } else {
                (true, "scoped change produced".to_string())
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
    use std::cell::RefCell;
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

        let run = execute_plan(&plan, &ws, &StubInvoker).unwrap();

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
    fn reviewer_prompt_includes_implementer_artifact() {
        let base = temp_base("artifact");
        let source = temp_base("artifact-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        execute_plan(&plan, &ws, &StubInvoker).unwrap();

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
        seen: RefCell<Vec<Role>>,
    }

    impl Invoker for FailReviewerInvoker {
        fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
            self.seen.borrow_mut().push(inv.role);
            if inv.role == Role::Reviewer {
                return InvocationResult::failure("reviewer could not run");
            }
            StubInvoker.invoke(inv)
        }
    }

    #[test]
    fn blocking_gate_halts_the_loop() {
        let base = temp_base("halt");
        let source = temp_base("halt-src");
        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        let plan = generate_plan("Add login", LoopOptions::default());

        let invoker = FailReviewerInvoker {
            seen: RefCell::new(Vec::new()),
        };
        let run = execute_plan(&plan, &ws, &invoker).unwrap();

        assert!(run.halted);
        assert!(!run.all_passed());
        // implementer + reviewer ran; the loop stopped before the revise step
        let seen = invoker.seen.borrow();
        assert_eq!(seen.as_slice(), &[Role::Implementer, Role::Reviewer]);
        let report = fs::read_to_string(ws.root.join("report.md")).unwrap();
        assert!(report.contains("halted on a blocking gate"));

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }
}
