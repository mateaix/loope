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

        // Codex can write its final message verbatim to a file; we read it back for
        // a reliable, structured message rather than scraping mixed stdout.
        let codex_output = if inv.adapter == Adapter::Codex {
            Some(inv.home_dir.join("loope-last-message.txt"))
        } else {
            None
        };

        let mut cmd = Command::new(&program);
        cmd.current_dir(&inv.workspace_dir);
        configure_command(&mut cmd, inv, self.isolate_home, codex_output.as_deref());
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
            // Prefer Codex's verbatim last-message file; else the last agent message
            // from its JSONL event stream; else trimmed stdout.
            let codex_message = codex_output
                .as_deref()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| extract_last_agent_message(&stdout));
            match codex_message {
                Some(msg) => msg,
                None => {
                    let trimmed = stdout.trim();
                    if trimmed.is_empty() {
                        "(no output)".to_string()
                    } else {
                        trimmed.to_string()
                    }
                }
            }
        } else {
            let detail = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            format!("exit {}: {detail}", output.status.code().unwrap_or(-1))
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
fn configure_command(
    cmd: &mut Command,
    inv: &AgentInvocation,
    isolate_home: bool,
    codex_output: Option<&std::path::Path>,
) {
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
            // Non-interactive exec mode; the prompt is read from stdin.
            cmd.arg("exec");
            // The run workspace is a copied tree, not a git repo.
            cmd.arg("--skip-git-repo-check");
            // Structured event stream, plus the final message written verbatim.
            cmd.arg("--json");
            if let Some(path) = codex_output {
                cmd.arg("-o").arg(path);
            }
            // Read-only steps (reviewer/verifier) cannot write; others may edit.
            if inv.read_only {
                cmd.args(["--sandbox", "read-only"]);
            } else {
                cmd.args(["--sandbox", "workspace-write"]);
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

/// Extract the last agent message from a Codex `--json` (JSONL) event stream.
/// Each agent message arrives as an object containing `"type":"agent_message"` and a
/// `"text"` field. Returns the text of the last such message, if any.
fn extract_last_agent_message(jsonl: &str) -> Option<String> {
    let mut last = None;
    for line in jsonl.lines() {
        if line.contains("\"type\":\"agent_message\"")
            && let Some(text) = extract_json_string_field(line, "text")
        {
            last = Some(text);
        }
    }
    last
}

/// Extract a JSON string field value from a single JSON line, handling the common
/// escape sequences. Minimal on purpose (no serde dependency).
fn extract_json_string_field(line: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\":\"");
    let start = line.find(&key)? + key.len();
    let mut out = String::new();
    let mut escaped = false;
    for c in line[start..].chars() {
        if escaped {
            match c {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => out.push(other),
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_last_agent_message_from_jsonl() {
        let jsonl = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"x\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"aggregated_output\":\"noise\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_1\",\"type\":\"agent_message\",\"text\":\"VERDICT: PASS\"}}\n",
            "{\"type\":\"turn.completed\"}\n"
        );
        assert_eq!(
            extract_last_agent_message(jsonl).as_deref(),
            Some("VERDICT: PASS")
        );
    }

    #[test]
    fn handles_escaped_text() {
        let line = "{\"type\":\"agent_message\",\"text\":\"line1\\nline2 \\\"quoted\\\"\"}";
        assert_eq!(
            extract_last_agent_message(line).as_deref(),
            Some("line1\nline2 \"quoted\"")
        );
    }

    #[test]
    fn none_when_no_agent_message() {
        assert_eq!(extract_last_agent_message("{\"type\":\"turn.started\"}"), None);
    }
}

