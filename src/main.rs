use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use loope::executor::execute_plan;
use loope::stub::StubInvoker;
use loope::subprocess::SubprocessInvoker;
use loope::workspace::RunWorkspace;
use loope::{LoopOptions, adapter::Invoker, generate_plan, list_adapters};

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print_help();
        return;
    }

    match args.remove(0).as_str() {
        "plan" => cmd_plan(&mut args),
        "run" => cmd_run(&mut args),
        "runs" => cmd_runs(),
        "show" => cmd_show(&args),
        "adapters" => {
            for adapter in list_adapters() {
                println!("{}", adapter.as_str());
            }
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            process::exit(2);
        }
    }
}

fn cmd_plan(args: &mut Vec<String>) {
    let include_design = remove_flag(args, "--design");
    let requirement = args.join(" ");
    if requirement.trim().is_empty() {
        eprintln!("loope plan requires a requirement.");
        process::exit(2);
    }

    let plan = generate_plan(
        &requirement,
        LoopOptions {
            include_design,
            ..LoopOptions::default()
        },
    );
    println!("{}", plan.to_markdown());
}

fn cmd_run(args: &mut Vec<String>) {
    let dry_run = remove_flag(args, "--dry-run");
    let in_place = remove_flag(args, "--in-place");
    let include_design = remove_flag(args, "--design");
    let approve = remove_value(args, "--approve").unwrap_or_else(|| "auto".to_string());
    let workdir = remove_value(args, "--workdir");
    let requirement = args.join(" ");

    if requirement.trim().is_empty() {
        eprintln!("loope run requires a requirement.");
        process::exit(2);
    }
    if approve != "auto" && approve != "manual" {
        eprintln!("--approve must be 'auto' or 'manual'.");
        process::exit(2);
    }

    let cwd = current_dir_or_exit();
    let source = workdir.map(PathBuf::from).unwrap_or_else(|| cwd.clone());
    if !source.is_dir() {
        eprintln!("workdir does not exist: {}", source.display());
        process::exit(2);
    }
    let base = cwd.join(".loope").join("runs");

    let plan = generate_plan(
        &requirement,
        LoopOptions {
            include_design,
            ..LoopOptions::default()
        },
    );

    if approve == "manual" && !confirm_plan(&plan, &source, in_place) {
        eprintln!("aborted before launching any agent.");
        process::exit(1);
    }

    let workspace = match RunWorkspace::create(&base, &source, in_place) {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!("failed to create run workspace: {err}");
            process::exit(1);
        }
    };

    let invoker: Box<dyn Invoker> = if dry_run {
        Box::new(StubInvoker)
    } else {
        Box::new(SubprocessInvoker)
    };

    let run = match execute_plan(&plan, &workspace, invoker.as_ref()) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("run failed: {err}");
            process::exit(1);
        }
    };

    println!("{}", run.to_report_markdown());
    println!("\nRun directory: {}", workspace.root.display());

    if !run.all_passed() {
        process::exit(1);
    }
}

fn cmd_runs() {
    let cwd = current_dir_or_exit();
    let base = cwd.join(".loope").join("runs");
    let mut ids = match list_run_ids(&base) {
        Ok(ids) => ids,
        Err(err) => {
            eprintln!("failed to read runs: {err}");
            process::exit(1);
        }
    };
    if ids.is_empty() {
        println!("no runs yet. Try: loope run --dry-run \"Add login\"");
        return;
    }
    ids.sort();
    for id in ids {
        println!("{id}");
    }
}

fn cmd_show(args: &[String]) {
    let Some(run_id) = args.first() else {
        eprintln!("loope show requires a run id, e.g. loope show run-0001");
        process::exit(2);
    };
    let cwd = current_dir_or_exit();
    let report = cwd
        .join(".loope")
        .join("runs")
        .join(run_id)
        .join("report.md");
    match fs::read_to_string(&report) {
        Ok(contents) => print!("{contents}"),
        Err(_) => {
            eprintln!("no report found for {run_id} (looked at {})", report.display());
            process::exit(1);
        }
    }
}

fn confirm_plan(plan: &loope::LoopPlan, source: &Path, in_place: bool) -> bool {
    println!("About to run {} step(s):", plan.steps.len());
    for step in &plan.steps {
        println!(
            "  {}. {} via {}",
            step.id,
            step.role.as_str(),
            step.adapter.display_name()
        );
    }
    println!(
        "Workspace source: {} ({})",
        source.display(),
        if in_place { "in place" } else { "copied" }
    );
    print!("Proceed? [y/N] ");
    let _ = io::stdout().flush();

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes" | "Yes")
}

fn list_run_ids(base: &Path) -> io::Result<Vec<String>> {
    let mut ids = Vec::new();
    if base.exists() {
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("run-") {
                    ids.push(name);
                }
            }
        }
    }
    Ok(ids)
}

fn current_dir_or_exit() -> PathBuf {
    match env::current_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("cannot determine current directory: {err}");
            process::exit(1);
        }
    }
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

/// Remove `--flag value` and return the value, if present.
fn remove_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.remove(index);
    if index < args.len() {
        Some(args.remove(index))
    } else {
        None
    }
}

fn print_help() {
    println!(
        "Loope - Loop Engineering orchestrator for collaborative coding agents.

Usage:
  loope plan <requirement>
  loope plan --design <requirement>
  loope run [--design] [--dry-run] [--in-place] [--workdir DIR] [--approve auto|manual] <requirement>
  loope runs
  loope show <run-id>
  loope adapters

Default loop:
  Claude implements -> Codex reviews -> Claude revises -> verifier checks

Design-aware loop:
  Design contract -> Claude implements -> Codex reviews -> Claude revises -> verifier checks

run flags:
  --dry-run   Execute with deterministic stub agents (no external CLIs, no network).
  --in-place  Operate on the working directory directly instead of a copied tree.
  --workdir   Source directory to run against (default: current directory).
  --approve   'auto' (default) or 'manual' (confirm before launching agents).

Runs are written to .loope/runs/<run-id>/."
    );
}
