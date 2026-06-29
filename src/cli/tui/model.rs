//! The TUI's view model, loaded from a run directory on disk. The same [`RunDetail`]
//! shape is reused by live mode (accumulated from `StepOutcome`s), so the views never
//! care where the data came from.

use std::fs;
use std::path::{Path, PathBuf};

/// One row in the run list.
#[derive(Clone, Debug)]
pub struct RunEntry {
    pub id: String,
    pub converged: bool,
    pub stop_reason: String,
    pub iterations: usize,
    pub steps: usize,
}

/// A whole run, as shown in the detail pane.
#[derive(Clone, Debug, Default)]
pub struct RunDetail {
    pub id: String,
    pub requirement: String,
    pub outcome: String,
    pub iterations: usize,
    pub changed: String,
    pub took: String,
    pub steps: Vec<StepView>,
    pub dir: PathBuf,
}

/// One executed step within a run.
#[derive(Clone, Debug, Default)]
pub struct StepView {
    /// Iteration this step ran in; `0` is the design step.
    pub iteration: usize,
    pub num: usize,
    pub role: String,
    pub adapter: String,
    pub passed: bool,
    pub gate_result: String,
    pub verdict: Option<String>,
    pub message: String,
    pub changes: Vec<String>,
}

/// Load every run under `base`, newest first.
pub fn load_runs(base: &Path) -> Vec<RunEntry> {
    let Ok(read) = fs::read_dir(base) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("run-"))
        })
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs.iter().filter_map(|d| load_entry(d)).collect()
}

fn load_entry(dir: &Path) -> Option<RunEntry> {
    let id = dir.file_name()?.to_str()?.to_string();
    let json = fs::read_to_string(dir.join("run.json")).ok()?;
    Some(RunEntry {
        id,
        converged: json.contains("\"converged\":true"),
        stop_reason: json_str(&json, "stop_reason").unwrap_or_default(),
        iterations: json_num(&json, "iterations").unwrap_or(0),
        steps: json.matches("\"role\":").count(),
    })
}

/// Load a run's full detail by parsing its `report.md`.
pub fn load_run(dir: &Path) -> Option<RunDetail> {
    let md = fs::read_to_string(dir.join("report.md")).ok()?;
    Some(parse_report(&md, dir))
}

/// Parse `report.md` (the format produced by `LoopRun::to_report_markdown`).
fn parse_report(md: &str, dir: &Path) -> RunDetail {
    let mut detail = RunDetail {
        dir: dir.to_path_buf(),
        ..Default::default()
    };
    let mut iteration = 0usize;
    let mut in_steps = false;

    for line in md.lines() {
        if let Some(v) = line.strip_prefix("- Run: ") {
            detail.id = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("- Requirement: ") {
            detail.requirement = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("- Outcome: ") {
            detail.outcome = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("- Iterations: ") {
            detail.iterations = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("- Changed: ") {
            detail.changed = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("- Took: ") {
            detail.took = v.trim().to_string();
        } else if line.starts_with("## Steps") {
            in_steps = true;
        } else if let Some(rest) = line.strip_prefix("### ") {
            iteration = if rest.starts_with("Design") {
                0
            } else {
                rest.trim_start_matches("Iteration ").trim().parse().unwrap_or(iteration)
            };
        } else if in_steps && let Some(step) = parse_step_header(line, iteration) {
            detail.steps.push(step);
        } else if let Some(step) = detail.steps.last_mut() {
            apply_step_field(step, line);
        }
    }
    detail
}

/// Parse a step header like `1. **implementer via Claude** — PASS`.
fn parse_step_header(line: &str, iteration: usize) -> Option<StepView> {
    let (num, rest) = line.split_once(". **")?;
    let num: usize = num.trim().parse().ok()?;
    let (inner, status) = rest.split_once("** — ")?;
    let (role, adapter) = inner.split_once(" via ")?;
    Some(StepView {
        iteration,
        num,
        role: role.trim().to_string(),
        adapter: adapter.trim().to_string(),
        passed: status.trim() == "PASS",
        ..Default::default()
    })
}

/// Fold a `   - Field: value` bullet into the current step.
fn apply_step_field(step: &mut StepView, line: &str) {
    let line = line.trim_start();
    if let Some(v) = line.strip_prefix("- Gate result: ") {
        step.gate_result = v.trim().to_string();
    } else if let Some(v) = line.strip_prefix("- Verdict: ") {
        step.verdict = Some(v.trim().to_string());
    } else if let Some(v) = line.strip_prefix("- Message: ") {
        step.message = v.trim().to_string();
    } else if let Some(v) = line.strip_prefix("- Changed: ") {
        step.changes.push(v.trim().to_string());
    }
}

/// Extract a `"key":"value"` string field from compact JSON.
fn json_str(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = json.find(&needle)? + needle.len();
    let end = json[start..].find('"')? + start;
    Some(json[start..end].to_string())
}

/// Extract a `"key":<number>` field from compact JSON.
fn json_num(json: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\":");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "\
# Loope Run Report

- Run: run-0007
- Requirement: Add login
- Outcome: stopped at the iteration cap
- Iterations: 2
- Changed: 1 file(s) +10 -0
- Took: 1s

## Steps

### Iteration 1

1. **implementer via Claude** — PASS
   - Gate result: change produced
   - Message: did the thing
   - Changed: src/lib.rs +10 -0
2. **reviewer via Codex** — BLOCK
   - Gate result: review produced
   - Verdict: blocked (needs work)
   - Message: fix it

### Iteration 2

3. **implementer via Claude** — PASS
   - Message: fixed
";

    #[test]
    fn parses_report_into_grouped_steps() {
        let detail = parse_report(REPORT, std::path::Path::new("/x"));
        assert_eq!(detail.id, "run-0007");
        assert_eq!(detail.iterations, 2);
        assert_eq!(detail.outcome, "stopped at the iteration cap");
        assert_eq!(detail.steps.len(), 3);

        let first = &detail.steps[0];
        assert_eq!(first.iteration, 1);
        assert_eq!(first.role, "implementer");
        assert_eq!(first.adapter, "Claude");
        assert!(first.passed);
        assert_eq!(first.changes, vec!["src/lib.rs +10 -0".to_string()]);

        let reviewer = &detail.steps[1];
        assert!(!reviewer.passed);
        assert_eq!(reviewer.verdict.as_deref(), Some("blocked (needs work)"));

        assert_eq!(detail.steps[2].iteration, 2);
    }

    #[test]
    fn extracts_json_fields() {
        let json = "{\"converged\":true,\"iterations\":3,\"stop_reason\":\"max_iters\",\"steps\":[{\"role\":\"x\"},{\"role\":\"y\"}]}";
        assert_eq!(json_num(json, "iterations"), Some(3));
        assert_eq!(json_str(json, "stop_reason").as_deref(), Some("max_iters"));
        assert!(json.contains("\"converged\":true"));
        assert_eq!(json.matches("\"role\":").count(), 2);
    }
}
