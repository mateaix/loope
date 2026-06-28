use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A unique temp directory for one test, with no external crates.
fn temp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "loope-it-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn run_dry_run_executes_loop_and_writes_run_directory() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("dryrun");

    let output = Command::new(exe)
        .args(["run", "--dry-run", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");

    assert!(output.status.success(), "expected success exit code");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("# Loope Run Report"));
    assert!(stdout.contains("converged"));
    assert!(stdout.contains("implementer via Claude"));
    assert!(stdout.contains("reviewer via Codex"));

    let run = cwd.join(".loope").join("runs").join("run-0001");
    assert!(run.join("plan.md").exists());
    assert!(run.join("report.md").exists());
    assert!(run.join("run.json").exists());
    assert!(
        run.join("agents")
            .join("01-implementer-claude")
            .join("result.md")
            .exists()
    );
    // the workspace was seeded and the stub implementer wrote into it
    assert!(
        run.join("workspace")
            .join("IMPLEMENTATION_NOTES.md")
            .exists()
    );

    let run_json = fs::read_to_string(run.join("run.json")).unwrap();
    assert!(run_json.contains("\"converged\":true"));

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn design_dry_run_includes_designer_step() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("design");

    let output = Command::new(exe)
        .args(["run", "--design", "--dry-run", "Build dashboard"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("designer via Claude"));

    let run = cwd.join(".loope").join("runs").join("run-0001");
    assert!(
        run.join("agents")
            .join("01-designer-claude")
            .join("result.md")
            .exists()
    );
    // the design contract was persisted as a run artifact
    assert!(run.join("design-contract.md").exists());

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn design_command_produces_a_contract() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("designcmd");

    let output = Command::new(exe)
        .args(["design", "--dry-run", "Build a settings page"])
        .current_dir(&cwd)
        .output()
        .expect("design loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Design contract"));
    assert!(stdout.contains("Contract:"));

    assert!(
        cwd.join(".loope")
            .join("runs")
            .join("run-0001")
            .join("design-contract.md")
            .exists()
    );

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn runs_and_show_report_a_produced_run() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("runsshow");

    // produce a run
    let first = Command::new(exe)
        .args(["run", "--dry-run", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");
    assert!(first.status.success());

    // runs lists it
    let runs = Command::new(exe)
        .arg("runs")
        .current_dir(&cwd)
        .output()
        .expect("runs");
    assert!(runs.status.success());
    let runs_out = String::from_utf8(runs.stdout).unwrap();
    assert!(runs_out.contains("run-0001"));
    // the listing now shows each run's outcome and step count
    assert!(runs_out.contains("converged"));
    assert!(runs_out.contains("steps"));

    // show prints its report
    let show = Command::new(exe)
        .args(["show", "run-0001"])
        .current_dir(&cwd)
        .output()
        .expect("show");
    assert!(show.status.success());
    let show_out = String::from_utf8(show.stdout).unwrap();
    assert!(show_out.contains("# Loope Run Report"));
    assert!(show_out.contains("Add login"));

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn run_ids_increment_across_runs() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("increment");

    for _ in 0..2 {
        let output = Command::new(exe)
            .args(["run", "--dry-run", "Add login"])
            .current_dir(&cwd)
            .output()
            .expect("run loope");
        assert!(output.status.success());
    }

    let runs_dir = cwd.join(".loope").join("runs");
    assert!(runs_dir.join("run-0001").exists());
    assert!(runs_dir.join("run-0002").exists());

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn multiple_reviewers_each_get_a_step() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("multirev");

    let output = Command::new(exe)
        .args(["run", "--dry-run", "--reviewers", "codex,claude", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("reviewer via Codex"));
    assert!(stdout.contains("reviewer via Claude"));

    let agents = cwd
        .join(".loope")
        .join("runs")
        .join("run-0001")
        .join("agents");
    assert!(agents.join("02-reviewer-codex").join("result.md").exists());
    assert!(agents.join("03-reviewer-claude").join("result.md").exists());

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn preset_dual_review_runs_two_reviewers() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("preset");

    let output = Command::new(exe)
        .args(["run", "--dry-run", "--preset", "dual-review", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("reviewer via Codex"));
    assert!(stdout.contains("reviewer via Claude"));

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn run_writes_events_and_show_diff_prints_changes() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("events");

    let run = Command::new(exe)
        .args(["run", "--dry-run", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");
    assert!(run.status.success());

    // the implementer step persisted a normalized event stream
    let events = cwd
        .join(".loope")
        .join("runs")
        .join("run-0001")
        .join("agents")
        .join("01-implementer-claude")
        .join("events.jsonl");
    let events_text = fs::read_to_string(&events).expect("events.jsonl");
    assert!(events_text.contains("\"type\":\"action\""));

    // the report shows change stats
    let report = Command::new(exe)
        .args(["show", "run-0001"])
        .current_dir(&cwd)
        .output()
        .expect("show");
    let report_out = String::from_utf8(report.stdout).unwrap();
    assert!(report_out.contains("IMPLEMENTATION_NOTES.md"));

    // show --diff prints the persisted unified diff
    let diff = Command::new(exe)
        .args(["show", "run-0001", "--diff"])
        .current_dir(&cwd)
        .output()
        .expect("show --diff");
    let diff_out = String::from_utf8(diff.stdout).unwrap();
    assert!(diff_out.contains("# Changes"));
    assert!(diff_out.contains("+# Implementation notes"));

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn dry_run_converges_in_one_iteration() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("turns");

    let output = Command::new(exe)
        .args(["run", "--dry-run", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");
    assert!(output.status.success());

    // The stub reviewer passes, so the loop converges in one iteration: implement (1)
    // and review (2) only, each in its own numbered directory.
    let run = cwd.join(".loope").join("runs").join("run-0001");
    let agents = run.join("agents");
    assert!(agents.join("01-implementer-claude").join("result.md").exists());
    assert!(agents.join("02-reviewer-codex").join("result.md").exists());

    let report = fs::read_to_string(run.join("report.md")).unwrap();
    assert!(report.contains("- Iterations: 1"));
    assert!(report.contains("- Outcome: converged"));

    let run_json = fs::read_to_string(run.join("run.json")).unwrap();
    assert!(run_json.contains("\"converged\":true"));

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn apply_lands_a_dry_run_change_into_a_target() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("applyrun");

    let run = Command::new(exe)
        .args(["run", "--dry-run", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");
    assert!(run.status.success());

    let target = temp_dir("applytgt");
    let apply = Command::new(exe)
        .args(["apply", "run-0001", "--workdir"])
        .arg(&target)
        .current_dir(&cwd)
        .output()
        .expect("apply loope");
    assert!(apply.status.success());
    let out = String::from_utf8(apply.stdout).unwrap();
    assert!(out.contains("IMPLEMENTATION_NOTES.md"));
    assert!(target.join("IMPLEMENTATION_NOTES.md").exists());

    let _ = fs::remove_dir_all(&cwd);
    let _ = fs::remove_dir_all(&target);
}

#[test]
fn run_show_diff_prints_cumulative_changes() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("showdiff");

    let run = Command::new(exe)
        .args(["run", "--dry-run", "--show-diff", "Add login"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    assert!(out.contains("# Changes"));
    assert!(out.contains("IMPLEMENTATION_NOTES.md"));

    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn run_requires_a_requirement() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let cwd = temp_dir("norq");

    let output = Command::new(exe)
        .args(["run", "--dry-run"])
        .current_dir(&cwd)
        .output()
        .expect("run loope");

    assert!(!output.status.success());

    let _ = fs::remove_dir_all(&cwd);
}
