use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

mod cli;

use cli::{theme, ui};
use loope::adapter::{StubInvoker, SubprocessInvoker};
use loope::engine::workspace::RunWorkspace;
use loope::engine::{LoopConfig, StepObserver, execute_loop};
use loope::{
    Adapter, LoopOptions, Role, adapter::Invoker, generate_plan, list_adapters,
};
use ui::{ColorChoice, PrettyObserver};

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print_help();
        return;
    }

    match args.remove(0).as_str() {
        "plan" => cmd_plan(&mut args),
        "design" => cmd_design(&mut args),
        "run" => cmd_run(&mut args),
        "runs" => cmd_runs(&mut args),
        "show" => cmd_show(&mut args),
        "apply" => cmd_apply(&mut args),
        "tui" => cmd_tui(&mut args),
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

fn cmd_design(args: &mut Vec<String>) {
    let dry_run = remove_flag(args, "--dry-run");
    let in_place = remove_flag(args, "--in-place");
    let isolate_home = remove_flag(args, "--isolate-home");
    let no_progress = remove_flag(args, "--no-progress");
    let color = ColorChoice::parse(&remove_value(args, "--color").unwrap_or_default()).enabled();
    apply_color_level(color);
    let opencode_model = remove_value(args, "--opencode-model")
        .or_else(|| std::env::var("LOOPE_OPENCODE_MODEL").ok())
        .filter(|m| !m.trim().is_empty());
    let timeout = resolve_timeout(remove_value(args, "--timeout"));
    let workdir = remove_value(args, "--workdir");
    let designer = remove_adapter(args, "--designer").unwrap_or(Adapter::Claude);
    let requirement = args.join(" ");

    if requirement.trim().is_empty() {
        eprintln!("loope design requires a requirement.");
        process::exit(2);
    }

    let cwd = current_dir_or_exit();
    let source = workdir.map(PathBuf::from).unwrap_or_else(|| cwd.clone());
    if !source.is_dir() {
        eprintln!("workdir does not exist: {}", source.display());
        process::exit(2);
    }
    let base = cwd.join(".loope").join("runs");

    // A design-only loop: one designer step, no iterations (max_iters = 0).
    let config = LoopConfig {
        requirement: requirement.clone(),
        include_design: true,
        designer,
        implementer: Adapter::Claude,
        reviewers: Vec::new(),
        max_iters: 0,
        verify_command: None,
    };

    if color {
        ui::banner(true);
    }

    let workspace = match RunWorkspace::create(&base, &source, in_place) {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!("failed to create run workspace: {err}");
            process::exit(1);
        }
    };

    let invoker: Box<dyn Invoker + Sync> = if dry_run {
        Box::new(StubInvoker)
    } else {
        Box::new(SubprocessInvoker {
            isolate_home,
            opencode_model,
            timeout,
        })
    };

    let renderer = (color && !no_progress).then(|| ui::LiveRenderer::start(1));
    let live_obs = renderer.as_ref().map(|r| ui::LiveObserver::new(r.sender()));
    let pretty = (color && no_progress).then_some(PrettyObserver { quiet: false });
    let observer: Option<&dyn StepObserver> = if let Some(o) = &live_obs {
        Some(o)
    } else if let Some(p) = &pretty {
        Some(p)
    } else {
        None
    };

    let run = match execute_loop(&config, &workspace, invoker.as_ref(), observer) {
        Ok(run) => run,
        Err(err) => {
            if let Some(r) = renderer {
                r.stop();
            }
            eprintln!("design failed: {err}");
            process::exit(1);
        }
    };
    if let Some(r) = renderer {
        r.stop();
    }

    let contract_path = workspace.root.join("design-contract.md");
    match fs::read_to_string(&contract_path) {
        Ok(contract) => println!("\n{}", contract.trim_end()),
        Err(_) => eprintln!("no design contract was produced"),
    }
    println!("\nContract: {}", contract_path.display());

    if !run.all_passed() {
        process::exit(1);
    }
}

fn cmd_run(args: &mut Vec<String>) {
    let dry_run = remove_flag(args, "--dry-run");
    let in_place = remove_flag(args, "--in-place");
    let include_design = remove_flag(args, "--design");
    let isolate_home = remove_flag(args, "--isolate-home");
    let quiet = remove_flag(args, "--quiet");
    let no_progress = remove_flag(args, "--no-progress");
    let color = ColorChoice::parse(&remove_value(args, "--color").unwrap_or_default()).enabled();
    apply_color_level(color);
    let approve = remove_value(args, "--approve").unwrap_or_else(|| "auto".to_string());
    let workdir = remove_value(args, "--workdir");
    let verify_command = remove_value(args, "--verify-cmd");
    let opencode_model = remove_value(args, "--opencode-model")
        .or_else(|| std::env::var("LOOPE_OPENCODE_MODEL").ok())
        .filter(|m| !m.trim().is_empty());
    let timeout = resolve_timeout(remove_value(args, "--timeout"));
    let max_iters = remove_value(args, "--max-iters")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    let show_diff = remove_flag(args, "--show-diff");
    let tui = remove_flag(args, "--tui");
    let preset = remove_value(args, "--preset");
    let implementer = remove_adapter(args, "--implementer");
    let reviewer = remove_adapter(args, "--reviewer");
    let reviewers_list = remove_value(args, "--reviewers");
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
    if tui {
        #[cfg(not(feature = "tui"))]
        {
            eprintln!("{TUI_HINT}");
            process::exit(2);
        }
        #[cfg(feature = "tui")]
        if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            eprintln!("--tui needs an interactive terminal.");
            process::exit(2);
        }
    }

    let cwd = current_dir_or_exit();
    let source = workdir.map(PathBuf::from).unwrap_or_else(|| cwd.clone());
    if !source.is_dir() {
        eprintln!("workdir does not exist: {}", source.display());
        process::exit(2);
    }
    let base = cwd.join(".loope").join("runs");

    // Start from the preset (or defaults), then let explicit flags override it.
    let base_options = preset_options(preset.as_deref());
    let reviewers = if let Some(list) = reviewers_list {
        parse_reviewers(&list)
    } else if let Some(single) = reviewer {
        vec![single]
    } else {
        base_options.reviewers
    };
    let config = LoopConfig {
        requirement: requirement.clone(),
        include_design,
        designer: designer.unwrap_or(base_options.designer),
        implementer: implementer.unwrap_or(base_options.implementer),
        reviewers: reviewers.clone(),
        max_iters,
        verify_command,
    };
    // A representative plan (one iteration) for the confirmation prompt and pipeline banner.
    let plan = generate_plan(
        &requirement,
        LoopOptions {
            include_design,
            implementer: config.implementer,
            reviewers,
            designer: config.designer,
            verifier: base_options.verifier,
        },
    );

    if !tui && approve == "manual" && !confirm_plan(&plan, &source, in_place) {
        eprintln!("aborted before launching any agent.");
        process::exit(1);
    }

    if color && !tui {
        ui::banner(true);
        // One iteration's roles, in order; the ↻ marks that it repeats to convergence.
        let mut loop_steps: Vec<(Role, Adapter)> = Vec::new();
        if config.include_design {
            loop_steps.push((Role::Designer, config.designer));
        }
        loop_steps.push((Role::Implementer, config.implementer));
        for reviewer in &config.reviewers {
            loop_steps.push((Role::Reviewer, *reviewer));
        }
        if config.verify_command.is_some() {
            loop_steps.push((Role::Verifier, Adapter::Generic));
        }
        ui::pipeline(&loop_steps, config.max_iters > 1, true);
    }

    let workspace = match RunWorkspace::create(&base, &source, in_place) {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!("failed to create run workspace: {err}");
            process::exit(1);
        }
    };
    let _ = fs::write(workspace.root.join("plan.md"), plan.to_markdown());

    let invoker: Box<dyn Invoker + Send + Sync> = if dry_run {
        Box::new(StubInvoker)
    } else {
        Box::new(SubprocessInvoker {
            isolate_home,
            opencode_model: opencode_model.clone(),
            timeout,
        })
    };

    // Full-screen live dashboard: hand the loop to the TUI and return when it quits.
    #[cfg(feature = "tui")]
    if tui {
        match cli::tui::run_live(config, workspace, invoker) {
            Ok(()) => return,
            Err(err) => {
                eprintln!("tui error: {err}");
                process::exit(1);
            }
        }
    }

    // Live mode (TTY color, progress on) animates a pinned status line; otherwise the
    // committed-only PrettyObserver, or nothing in plain/piped output.
    let per_iter = 1 + config.reviewers.len() + usize::from(config.verify_command.is_some());
    let est_total = usize::from(config.include_design) + config.max_iters * per_iter;
    let renderer = (color && !no_progress).then(|| ui::LiveRenderer::start(est_total));
    let live_obs = renderer.as_ref().map(|r| ui::LiveObserver::new(r.sender()));
    let pretty = (color && no_progress).then_some(PrettyObserver { quiet });
    let observer: Option<&dyn StepObserver> = if let Some(o) = &live_obs {
        Some(o)
    } else if let Some(p) = &pretty {
        Some(p)
    } else {
        None
    };

    let run = match execute_loop(&config, &workspace, invoker.as_ref(), observer) {
        Ok(run) => run,
        Err(err) => {
            if let Some(r) = renderer {
                r.stop();
            }
            eprintln!("run failed: {err}");
            process::exit(1);
        }
    };

    if let Some(r) = renderer {
        r.stop();
    }

    ui::print_report(&run.to_report_markdown(), Some(&workspace.root), color);

    if show_diff {
        let diffs = collect_run_diffs(&workspace.root);
        if !diffs.trim().is_empty() {
            println!("\n# Changes\n");
            ui::print_diff(&diffs, color);
        }
    }

    if !run.all_passed() {
        process::exit(1);
    }
}

fn cmd_runs(args: &mut Vec<String>) {
    let color = ColorChoice::parse(&remove_value(args, "--color").unwrap_or_default()).enabled();
    apply_color_level(color);
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
    let items: Vec<(String, Option<ui::RunSummary>)> = ids
        .into_iter()
        .map(|id| {
            let summary = read_run_summary(&base.join(&id));
            (id, summary)
        })
        .collect();
    ui::runs_list(&items, color);
}

/// Read a run's `run.json` for its at-a-glance outcome. Returns `None` when the file
/// is missing or unreadable.
fn read_run_summary(run_dir: &Path) -> Option<ui::RunSummary> {
    let json = fs::read_to_string(run_dir.join("run.json")).ok()?;
    Some(ui::RunSummary {
        passed: json.contains("\"converged\":true"),
        halted: json.contains("\"stop_reason\":\"step_failed\""),
        steps: json.matches("\"gate_passed\":").count(),
    })
}

fn cmd_show(args: &mut Vec<String>) {
    let color = ColorChoice::parse(&remove_value(args, "--color").unwrap_or_default()).enabled();
    apply_color_level(color);
    let show_diff = remove_flag(args, "--diff");
    let Some(run_id) = args.first() else {
        eprintln!("loope show requires a run id, e.g. loope show run-0001");
        process::exit(2);
    };
    let cwd = current_dir_or_exit();
    let run_dir = cwd.join(".loope").join("runs").join(run_id);
    let report = run_dir.join("report.md");
    match fs::read_to_string(&report) {
        Ok(contents) => ui::print_report(&contents, Some(&run_dir), color),
        Err(_) => {
            eprintln!("no report found for {run_id} (looked at {})", report.display());
            process::exit(1);
        }
    }

    if show_diff {
        let diffs = collect_run_diffs(&run_dir);
        if diffs.trim().is_empty() {
            println!("\n(no recorded changes for {run_id})");
        } else {
            println!("\n# Changes\n");
            ui::print_diff(&diffs, color);
        }
    }
}

/// Copy a run's changed/added files from its workspace back into the working directory.
fn cmd_apply(args: &mut Vec<String>) {
    let workdir = remove_value(args, "--workdir");
    let Some(run_id) = args.first().cloned() else {
        eprintln!("loope apply requires a run id, e.g. loope apply run-0001");
        process::exit(2);
    };
    let cwd = current_dir_or_exit();
    let run_dir = cwd.join(".loope").join("runs").join(&run_id);
    let workspace = run_dir.join("workspace");
    if !workspace.is_dir() {
        eprintln!("no run workspace found for {run_id} (looked at {})", workspace.display());
        process::exit(1);
    }
    let target = workdir.map(PathBuf::from).unwrap_or(cwd);
    if !target.is_dir() {
        eprintln!("target directory does not exist: {}", target.display());
        process::exit(2);
    }

    let listing = match fs::read_to_string(run_dir.join("changed-files.txt")) {
        Ok(text) => text,
        Err(_) => {
            eprintln!("no changed-files.txt for {run_id}; nothing to apply");
            process::exit(1);
        }
    };
    let files: Vec<&str> = listing.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if files.is_empty() {
        println!("{run_id} changed no files; nothing to apply.");
        return;
    }

    let mut applied = 0usize;
    for rel in files {
        let from = workspace.join(rel);
        let to = target.join(rel);
        if !from.is_file() {
            continue;
        }
        if let Some(parent) = to.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            eprintln!("failed to create {}: {err}", parent.display());
            process::exit(1);
        }
        match fs::copy(&from, &to) {
            Ok(_) => {
                println!("  applied {rel}");
                applied += 1;
            }
            Err(err) => {
                eprintln!("failed to apply {rel}: {err}");
                process::exit(1);
            }
        }
    }
    println!("Applied {applied} file(s) from {run_id} into {}.", target.display());
}

/// Open the interactive TUI browser over `.loope/runs/` (requires `--features tui`).
fn cmd_tui(args: &mut Vec<String>) {
    let _ = remove_value(args, "--color"); // the TUI manages its own color
    let cwd = current_dir_or_exit();
    let runs_dir = cwd.join(".loope").join("runs");

    #[cfg(feature = "tui")]
    {
        if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            eprintln!("the TUI needs an interactive terminal.");
            process::exit(2);
        }
        if let Err(err) = cli::tui::run_browser(&runs_dir) {
            eprintln!("tui error: {err}");
            process::exit(1);
        }
    }
    #[cfg(not(feature = "tui"))]
    {
        let _ = runs_dir;
        eprintln!("{TUI_HINT}");
        process::exit(2);
    }
}

/// Shown when a TUI command is invoked on a build without the `tui` feature.
#[cfg(not(feature = "tui"))]
const TUI_HINT: &str =
    "the TUI requires a build with the `tui` feature: `cargo install --path . --features tui`";

/// Concatenate every step's `changes.diff` for a run, in step order.
fn collect_run_diffs(run_dir: &Path) -> String {
    // Prefer the run-level cumulative diff; fall back to concatenating per-step diffs
    // (for runs produced before the cumulative diff existed).
    if let Ok(diff) = fs::read_to_string(run_dir.join("changes.diff"))
        && !diff.trim().is_empty()
    {
        return diff;
    }
    let agents = run_dir.join("agents");
    let mut dirs: Vec<PathBuf> = match fs::read_dir(&agents) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => return String::new(),
    };
    dirs.sort();
    let mut out = String::new();
    for dir in dirs {
        if let Ok(diff) = fs::read_to_string(dir.join("changes.diff")) {
            out.push_str(&diff);
        }
    }
    out
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

/// Resolve the per-step timeout from `--timeout` (seconds) over `LOOPE_TIMEOUT` over a
/// 600s default. `0` disables the bound.
fn resolve_timeout(flag: Option<String>) -> Option<std::time::Duration> {
    timeout_from(flag.or_else(|| std::env::var("LOOPE_TIMEOUT").ok()))
}

/// Pure timeout parse: a seconds string (or none) into a duration; `0` disables.
fn timeout_from(raw: Option<String>) -> Option<std::time::Duration> {
    let secs: u64 = raw.and_then(|s| s.trim().parse().ok()).unwrap_or(600);
    (secs != 0).then(|| std::time::Duration::from_secs(secs))
}

/// Resolve and store the process-wide color level from the `color` decision.
fn apply_color_level(color: bool) {
    let level = if color {
        theme::detect_enabled_level()
    } else {
        theme::ColorLevel::None
    };
    theme::set_level(level);
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

/// Expand a `--preset` name to base options, exiting on an unknown name.
fn preset_options(preset: Option<&str>) -> LoopOptions {
    let Some(name) = preset else {
        return LoopOptions::default();
    };
    let reviewers = |adapters: &[Adapter]| adapters.to_vec();
    match name.trim().to_ascii_lowercase().as_str() {
        "claude-codex" => LoopOptions {
            implementer: Adapter::Claude,
            reviewers: reviewers(&[Adapter::Codex]),
            ..LoopOptions::default()
        },
        "codex-claude" => LoopOptions {
            implementer: Adapter::Codex,
            reviewers: reviewers(&[Adapter::Claude]),
            ..LoopOptions::default()
        },
        "claude-solo" => LoopOptions {
            implementer: Adapter::Claude,
            reviewers: reviewers(&[Adapter::Claude]),
            ..LoopOptions::default()
        },
        "dual-review" => LoopOptions {
            implementer: Adapter::Claude,
            reviewers: reviewers(&[Adapter::Codex, Adapter::Claude]),
            ..LoopOptions::default()
        },
        "opencode-codex" => LoopOptions {
            implementer: Adapter::OpenCode,
            reviewers: reviewers(&[Adapter::Codex]),
            ..LoopOptions::default()
        },
        other => {
            eprintln!(
                "unknown preset: {other} (try claude-codex, codex-claude, claude-solo, dual-review, opencode-codex)"
            );
            process::exit(2);
        }
    }
}

/// Parse a comma-separated reviewer list, exiting on an unknown or empty entry.
fn parse_reviewers(list: &str) -> Vec<Adapter> {
    let reviewers: Vec<Adapter> = list
        .split(',')
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| match Adapter::parse(item) {
            Some(adapter) => adapter,
            None => {
                eprintln!("unknown reviewer adapter: {item}");
                process::exit(2);
            }
        })
        .collect();
    if reviewers.is_empty() {
        eprintln!("--reviewers needs at least one adapter, e.g. --reviewers codex,claude");
        process::exit(2);
    }
    reviewers
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
  loope design [--designer A] <requirement>
  loope run [--design] [--dry-run] [--max-iters N] [--show-diff] [--workdir DIR] <requirement>
  loope runs
  loope show <run-id> [--diff]
  loope apply <run-id> [--workdir DIR]
  loope tui                                  (build with --features tui)
  loope adapters

The loop iterates: an optional design step, then implement -> review -> verify repeated
with feedback until it converges (verification passes, no reviewer blocks) or --max-iters.

run flags:
  --dry-run       Execute with deterministic stub agents (no external CLIs, no network).
  --max-iters N   Cap the implement -> review -> verify iterations (default 3; 1 = single pass).
  --show-diff     After the run, print the cumulative diff of everything that changed.
  --tui           Watch the run in a full-screen dashboard (build with --features tui).
  --in-place      Operate on the working directory directly instead of a copied tree.
  --workdir DIR   Source directory to run against (default: current directory).
  --approve MODE  'auto' (default) or 'manual' (confirm before launching agents).
  --preset NAME   claude-codex | codex-claude | claude-solo | dual-review | opencode-codex.
  --implementer A Override the implementer adapter (claude|codex|opencode|generic).
  --reviewer A    Override the reviewer adapter (single).
  --reviewers A,B Run several reviewers in parallel and aggregate verdicts.
  --designer A    Override the designer adapter (with --design).
  --opencode-model M  Model 'provider/model' for OpenCode (or LOOPE_OPENCODE_MODEL).
  --timeout SECS  Per-step timeout (default 600; 0 disables; or LOOPE_TIMEOUT).
  --verify-cmd C  Run shell command C as the verifier (gate passes iff it exits 0).
  --isolate-home  Give each agent a private CLI config dir (default: reuse your login).
  --quiet         Suppress the live activity feed; keep step results and summary.
  --no-progress   Disable the animated status line (keep committed step lines).
  --color WHEN    'auto' (default), 'always', or 'never' for terminal coloring.

show flags:
  --diff          Also print the run's cumulative diff.

apply copies a run's changed files from its workspace into your tree (never deletes).
Runs are written to .loope/runs/<run-id>/."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timeout_parsing() {
        assert_eq!(timeout_from(None), Some(Duration::from_secs(600)));
        assert_eq!(timeout_from(Some("120".into())), Some(Duration::from_secs(120)));
        assert_eq!(timeout_from(Some(" 30 ".into())), Some(Duration::from_secs(30)));
        assert_eq!(timeout_from(Some("0".into())), None); // disabled
        assert_eq!(timeout_from(Some("nonsense".into())), Some(Duration::from_secs(600)));
    }
}
