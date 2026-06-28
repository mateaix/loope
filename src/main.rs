use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

mod ui;

use loope::executor::{ExecuteOptions, StepObserver, execute_plan};
use loope::stub::StubInvoker;
use loope::subprocess::SubprocessInvoker;
use loope::workspace::RunWorkspace;
use loope::{Adapter, LoopOptions, adapter::Invoker, generate_plan, list_adapters};
use ui::{ColorChoice, PrettyObserver};

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
    let isolate_home = remove_flag(args, "--isolate-home");
    let color = ColorChoice::parse(&remove_value(args, "--color").unwrap_or_default()).enabled();
    let approve = remove_value(args, "--approve").unwrap_or_else(|| "auto".to_string());
    let workdir = remove_value(args, "--workdir");
    let verify_command = remove_value(args, "--verify-cmd");
    let implementer = remove_adapter(args, "--implementer");
    let reviewer = remove_adapter(args, "--reviewer");
    let designer = remove_adapter(args, "--designer");
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

    let defaults = LoopOptions::default();
    let plan = generate_plan(
        &requirement,
        LoopOptions {
            include_design,
            implementer: implementer.unwrap_or(defaults.implementer),
            reviewer: reviewer.unwrap_or(defaults.reviewer),
            designer: designer.unwrap_or(defaults.designer),
            verifier: defaults.verifier,
        },
    );

    if approve == "manual" && !confirm_plan(&plan, &source, in_place) {
        eprintln!("aborted before launching any agent.");
        process::exit(1);
    }

    if color {
        ui::banner(true);
        ui::pipeline(&plan, true);
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
        Box::new(SubprocessInvoker { isolate_home })
    };
    let options = ExecuteOptions { verify_command };
    let observer: Option<&dyn StepObserver> = if color { Some(&PrettyObserver) } else { None };

    let run = match execute_plan(&plan, &workspace, invoker.as_ref(), &options, observer) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("run failed: {err}");
            process::exit(1);
        }
    };

    ui::summary(&run, &workspace.root, color);

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
    ui::runs_list(&ids, ColorChoice::Auto.enabled());
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

/// Remove `--flag <adapter>` and parse it, exiting on an unknown adapter name.
fn remove_adapter(args: &mut Vec<String>, flag: &str) -> Option<Adapter> {
    let value = remove_value(args, flag)?;
    match Adapter::parse(&value) {
        Some(adapter) => Some(adapter),
        None => {
            eprintln!("unknown adapter for {flag}: {value} (try claude, codex, opencode, generic)");
            process::exit(2);
        }
    }
}

fn print_help() {
    ui::banner(ColorChoice::Auto.enabled());
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
  --dry-run       Execute with deterministic stub agents (no external CLIs, no network).
  --in-place      Operate on the working directory directly instead of a copied tree.
  --workdir DIR   Source directory to run against (default: current directory).
  --approve MODE  'auto' (default) or 'manual' (confirm before launching agents).
  --implementer A Override the implementer adapter (claude|codex|opencode|generic).
  --reviewer A    Override the reviewer adapter.
  --designer A    Override the designer adapter (with --design).
  --verify-cmd C  Run shell command C as the verifier (gate passes iff it exits 0).
  --isolate-home  Give each agent a private CLI config dir (default: reuse your login).
  --color WHEN    'auto' (default), 'always', or 'never' for terminal coloring.

Runs are written to .loope/runs/<run-id>/."
    );
}
