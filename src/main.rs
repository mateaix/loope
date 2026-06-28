use std::env;
use std::process;

use loope::{LoopOptions, generate_plan, list_adapters};

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print_help();
        return;
    }

    match args.remove(0).as_str() {
        "plan" => {
            let include_design = remove_flag(&mut args, "--design");
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

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn print_help() {
    println!(
        "Loope - Loop Engineering orchestrator for collaborative coding agents.

Usage:
  loope plan <requirement>
  loope plan --design <requirement>
  loope adapters

Default loop:
  Claude implements -> Codex reviews -> Claude revises -> verifier checks

Design-aware loop:
  Design contract -> Claude implements -> Codex reviews -> Claude revises -> verifier checks"
    );
}
