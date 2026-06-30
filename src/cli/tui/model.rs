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
    /// A relative "when it ran" hint, e.g. `2h ago`.
    pub age: String,
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
    /// The "caught & fixed" highlight, when the review earned its keep.
    pub highlight: Option<loope::engine::Highlight>,
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
    // Any directory holding a run.json is a run, regardless of its name (so the new
    // `NNNN-slug` ids and the legacy `run-NNNN` both load); sort newest-first by name.
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs.iter().filter_map(|d| load_entry(d)).collect()
}

fn load_entry(dir: &Path) -> Option<RunEntry> {
    let id = dir.file_name()?.to_str()?.to_string();
    let path = dir.join("run.json");
    let json = fs::read_to_string(&path).ok()?;
    Some(RunEntry {
        id,
        converged: json.contains("\"converged\":true"),
        stop_reason: json_str(&json, "stop_reason").unwrap_or_default(),
        iterations: json_num(&json, "iterations").unwrap_or(0),
        steps: json.matches("\"role\":").count(),
        age: file_age(&path),
    })
}

/// A relative age for a file (how long ago it was written), or `""` if unavailable.
fn file_age(path: &Path) -> String {
    let Some(secs) = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs())
    else {
        return String::new();
    };
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Load a run's full detail by parsing its `report.md`, plus the highlight if present.
pub fn load_run(dir: &Path) -> Option<RunDetail> {
    let md = fs::read_to_string(dir.join("report.md")).ok()?;
    let mut detail = parse_report(&md, dir);
    // report.md stores a one-line message summary; replace it with the full `## Message`
    // section from each step's result.md so the preview shows the complete review findings.
    for step in &mut detail.steps {
        let result_md = dir.join("agents").join(step_dir_name(step)).join("result.md");
        if let Some(full) = read_result_message(&result_md) {
            step.message = full;
        }
    }
    detail.highlight = fs::read_to_string(dir.join("highlight"))
        .ok()
        .and_then(|text| loope::engine::Highlight::from_storage(&text));
    Some(detail)
}

/// A step's `agents/` subdirectory name, e.g. `02-reviewer-codex`.
fn step_dir_name(step: &StepView) -> String {
    format!("{:02}-{}-{}", step.num, step.role, step.adapter.to_ascii_lowercase())
}

/// Read the full `## Message` section from a step's `result.md` (everything up to the next
/// `## ` header). Returns `None` when the file or section is missing/empty.
fn read_result_message(path: &Path) -> Option<String> {
    let md = fs::read_to_string(path).ok()?;
    let after = md.split_once("## Message")?.1;
    let mut body = String::new();
    for line in after.lines().skip(1) {
        if line.starts_with("## ") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    let trimmed = body.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
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
    fn reads_full_message_section_from_result_md() {
        let dir = std::env::temp_dir().join(format!("loope-msg-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("result.md");
        fs::write(
            &path,
            "# Result\n\n- Success: true\n\n## Message\n\n**Blocking Findings**\n\n1. fix the guard\n2. add a test\n\nVERDICT: BLOCK\n\n## Changed Files\n\n- src/x.rs\n",
        )
        .unwrap();
        let msg = read_result_message(&path).unwrap();
        assert!(msg.starts_with("**Blocking Findings**"));
        assert!(msg.contains("1. fix the guard"));
        assert!(msg.contains("VERDICT: BLOCK"));
        // Stops at the next section header.
        assert!(!msg.contains("Changed Files"));
        assert!(read_result_message(&dir.join("missing.md")).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn step_dir_name_matches_layout() {
        let step = StepView {
            num: 2,
            role: "reviewer".to_string(),
            adapter: "Codex".to_string(),
            ..Default::default()
        };
        assert_eq!(step_dir_name(&step), "02-reviewer-codex");
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
