//! Subprocess invoker that drives a real coding-agent CLI.
//!
//! Each agent is launched in headless / non-interactive mode with the run workspace
//! as its working directory and a private home directory passed via environment, so
//! agents never share session state. The prompt is delivered on stdin and the output
//! stream is captured as the step transcript.
//!
//! This path requires the real `claude` / `codex` binaries and is therefore exercised
//! by the manual checklist in the README rather than by automated tests.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::Adapter;
use crate::adapter::{AgentInvocation, InvocationResult, Invoker, resolve_program, spec_for};

/// An [`Invoker`] that runs the adapter's real CLI as a subprocess.
#[derive(Clone, Copy, Debug, Default)]
pub struct SubprocessInvoker {
    /// When true, point each agent's CLI config/home at its private run directory.
    /// Default (false) reuses the user's normal login so authentication works.
    pub isolate_home: bool,
}

impl Invoker for SubprocessInvoker {
    fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
        let spec = spec_for(inv.adapter);
        let Some(program) = resolve_program(&spec) else {
            return InvocationResult::failure(format!(
                "no program configured for adapter '{}' (set {})",
                inv.adapter.as_str(),
                spec.env_override
            ));
        };

        let mut cmd = Command::new(&program);
        cmd.current_dir(&inv.workspace_dir);
        configure_command(&mut cmd, inv, self.isolate_home);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                return InvocationResult::failure(format!("failed to launch '{program}': {err}"));
            }
        };

        // Deliver the prompt on stdin, then close it so the CLI can start working.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(inv.prompt.as_bytes());
        }

        // `wait_with_output` drains stdout/stderr while waiting, avoiding pipe deadlock.
        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(err) => return InvocationResult::failure(format!("capturing output failed: {err}")),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let success = output.status.success();

        let message = if success {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                "(no output)".to_string()
            } else {
                trimmed.to_string()
            }
        } else {
            let detail = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            format!(
                "exit {}: {detail}",
                output.status.code().unwrap_or(-1)
            )
        };

        InvocationResult {
            success,
            message,
            changed_files: Vec::new(),
            transcript: format!("--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n"),
        }
    }
}

/// Apply per-adapter headless flags. The prompt is delivered on stdin for every
/// adapter. When `isolate_home` is set, the agent's CLI config/home is redirected to
/// its private run directory; otherwise the user's normal login is reused so the CLI
/// authenticates.
fn configure_command(cmd: &mut Command, inv: &AgentInvocation, isolate_home: bool) {
    match inv.adapter {
        Adapter::Claude => {
            // Print (headless) mode.
            cmd.arg("-p");
            // Read-only steps plan only; write-capable steps may edit the workspace.
            if inv.read_only {
                cmd.args(["--permission-mode", "plan"]);
            } else {
                cmd.args(["--permission-mode", "acceptEdits"]);
            }
            if isolate_home {
                cmd.env("CLAUDE_CONFIG_DIR", &inv.home_dir);
            }
        }
        Adapter::Codex => {
            // Non-interactive exec mode.
            cmd.arg("exec");
            if inv.read_only {
                cmd.args(["--sandbox", "read-only"]);
            }
            if isolate_home {
                cmd.env("CODEX_HOME", &inv.home_dir);
            }
        }
        Adapter::OpenCode => {
            cmd.arg("run");
        }
        Adapter::Generic => {}
    }
}
