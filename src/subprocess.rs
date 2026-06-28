//! Subprocess invoker that drives a real coding-agent CLI.
//!
//! Each agent is launched in headless / non-interactive mode with the run workspace
//! as its working directory and a private home directory passed via environment, so
//! agents never share session state. The prompt is delivered on stdin and the output
//! stream is captured as the step transcript.
//!
//! This path requires the real `claude` / `codex` binaries and is therefore exercised
//! by the manual checklist in the README rather than by automated tests.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::thread;

use crate::Adapter;
use crate::adapter::{AgentInvocation, InvocationResult, Invoker, resolve_program, spec_for};
use crate::event::{ActionKind, LoopEvent};

/// An [`Invoker`] that runs the adapter's real CLI as a subprocess.
#[derive(Clone, Copy, Debug, Default)]
pub struct SubprocessInvoker {
    /// When true, point each agent's CLI config/home at its private run directory.
    /// Default (false) reuses the user's normal login so authentication works.
    pub isolate_home: bool,
}

impl Invoker for SubprocessInvoker {
    fn invoke(&self, inv: &AgentInvocation) -> InvocationResult {
        // The non-streaming path just discards events.
        self.invoke_streaming(inv, &mut |_| {})
    }

    fn invoke_streaming(
        &self,
        inv: &AgentInvocation,
        sink: &mut dyn FnMut(LoopEvent),
    ) -> InvocationResult {
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

        // Drain stderr on a thread so a chatty child can't deadlock the stdout reader.
        let stderr_handle = child.stderr.take().map(|mut err| {
            thread::spawn(move || {
                let mut buf = String::new();
                let _ = err.read_to_string(&mut buf);
                buf
            })
        });

        // Read stdout line by line, parsing each into events as it arrives.
        let mut stdout = String::new();
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                for event in parse_event(inv.adapter, &line) {
                    sink(event);
                }
                stdout.push_str(&line);
                stdout.push('\n');
            }
        }

        let status = child.wait();
        let stderr = stderr_handle
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        let success = status.as_ref().map(|s| s.success()).unwrap_or(false);
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);

        let message = if success {
            adapter_message(inv.adapter, codex_output.as_deref(), &stdout).unwrap_or_else(|| {
                let trimmed = stdout.trim();
                if trimmed.is_empty() {
                    "(no output)".to_string()
                } else {
                    trimmed.to_string()
                }
            })
        } else {
            let detail = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            format!("exit {code}: {detail}")
        };

        InvocationResult {
            success,
            message,
            changed_files: Vec::new(),
            transcript: format!("--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n"),
        }
    }
}

/// Derive a step's final message from its captured stream, per adapter.
fn adapter_message(adapter: Adapter, codex_output: Option<&std::path::Path>, stdout: &str) -> Option<String> {
    match adapter {
        Adapter::Codex => codex_output
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| extract_last_agent_message(stdout)),
        Adapter::Claude => extract_claude_result(stdout),
        _ => {
            let trimmed = stdout.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
    }
}

/// Extract Claude's final `result` text from a `stream-json` stdout.
fn extract_claude_result(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if line.contains("\"type\":\"result\"")
            && let Some(text) = extract_json_string_field(line, "result")
            && !text.trim().is_empty()
        {
            return Some(text.trim().to_string());
        }
    }
    None
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
            // Print (headless) mode with a structured event stream we can parse live.
            cmd.arg("-p");
            cmd.args(["--output-format", "stream-json", "--verbose"]);
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

/// Parse one Claude `--output-format stream-json` line into normalized events.
/// Content blocks are bounded by splitting on the `{"type":"` marker.
fn parse_claude_event(line: &str) -> Vec<LoopEvent> {
    let mut events = Vec::new();
    for seg in line.split("{\"type\":\"") {
        if let Some(rest) = seg.strip_prefix("tool_use\"") {
            if let Some(name) = extract_json_string_field(rest, "name") {
                let target = extract_json_string_field(rest, "file_path")
                    .or_else(|| extract_json_string_field(rest, "command"))
                    .or_else(|| extract_json_string_field(rest, "pattern"))
                    .or_else(|| extract_json_string_field(rest, "path"))
                    .unwrap_or_else(|| name.clone());
                events.push(LoopEvent::Action {
                    kind: map_claude_tool(&name),
                    target: shorten_target(&target),
                });
            }
        } else if let Some(rest) = seg.strip_prefix("text\"") {
            if let Some(text) = extract_json_string_field(rest, "text")
                && !text.trim().is_empty()
            {
                events.push(LoopEvent::Message {
                    text: shorten_message(text.trim()),
                });
            }
        } else if seg.starts_with("result\"")
            && let (Some(input), Some(output)) = (
                extract_json_u64(line, "input_tokens"),
                extract_json_u64(line, "output_tokens"),
            )
        {
            events.push(LoopEvent::Usage {
                input_tokens: input,
                output_tokens: output,
            });
        }
    }
    events
}

/// Parse one Codex `--json` line into normalized events.
fn parse_codex_event(line: &str) -> Vec<LoopEvent> {
    let mut events = Vec::new();
    if line.contains("\"item.started\"")
        && line.contains("\"command_execution\"")
        && let Some(command) = extract_json_string_field(line, "command")
    {
        events.push(LoopEvent::Action {
            kind: ActionKind::Command,
            target: shorten_target(&command),
        });
    }
    if line.contains("\"item.completed\"")
        && line.contains("\"agent_message\"")
        && let Some(text) = extract_json_string_field(line, "text")
        && !text.trim().is_empty()
    {
        events.push(LoopEvent::Message {
            text: shorten_message(text.trim()),
        });
    }
    if line.contains("\"turn.completed\"")
        && let (Some(input), Some(output)) = (
            extract_json_u64(line, "input_tokens"),
            extract_json_u64(line, "output_tokens"),
        )
    {
        events.push(LoopEvent::Usage {
            input_tokens: input,
            output_tokens: output,
        });
    }
    events
}

/// Dispatch to the adapter's event parser.
fn parse_event(adapter: Adapter, line: &str) -> Vec<LoopEvent> {
    match adapter {
        Adapter::Claude => parse_claude_event(line),
        Adapter::Codex => parse_codex_event(line),
        _ => Vec::new(),
    }
}

/// Map a Claude tool name to an action kind.
fn map_claude_tool(name: &str) -> ActionKind {
    match name {
        "Write" => ActionKind::Write,
        "Edit" | "MultiEdit" | "NotebookEdit" => ActionKind::Edit,
        "Read" | "NotebookRead" => ActionKind::Read,
        "Bash" | "BashOutput" | "KillBash" => ActionKind::Command,
        "Grep" | "Glob" | "WebSearch" | "WebFetch" => ActionKind::Search,
        _ => ActionKind::Other,
    }
}

/// Extract an unsigned integer JSON field value from a line.
fn extract_json_u64(line: &str, field: &str) -> Option<u64> {
    let key = format!("\"{field}\":");
    let start = line.find(&key)? + key.len();
    let digits: String = line[start..]
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Trim an action target (path or command) for display: keep the tail of long paths.
fn shorten_target(target: &str) -> String {
    let target = target.trim();
    if target.contains('/') && target.len() > 48 {
        let tail: Vec<&str> = target.rsplit('/').take(2).collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("/");
        format!("…/{tail}")
    } else if target.len() > 64 {
        format!("{}…", &target[..63])
    } else {
        target.to_string()
    }
}

/// Collapse an assistant message to a single short line.
fn shorten_message(text: &str) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    if first.chars().count() > 80 {
        let truncated: String = first.chars().take(79).collect();
        format!("{truncated}…")
    } else {
        first.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_tool_use_and_text() {
        let line = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"text\",\"text\":\"I'll create that file.\"},\
            {\"type\":\"tool_use\",\"id\":\"x\",\"name\":\"Write\",\"input\":{\"file_path\":\"/tmp/ws/notes.txt\"}}]}}";
        let events = parse_claude_event(line);
        assert_eq!(
            events,
            vec![
                LoopEvent::Message {
                    text: "I'll create that file.".to_string()
                },
                LoopEvent::Action {
                    kind: ActionKind::Write,
                    target: "/tmp/ws/notes.txt".to_string()
                },
            ]
        );
    }

    #[test]
    fn parses_claude_bash_and_result_usage() {
        let bash = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"tool_use\",\"name\":\"Bash\",\"input\":{\"command\":\"cargo test\"}}]}}";
        assert_eq!(
            parse_claude_event(bash),
            vec![LoopEvent::Action {
                kind: ActionKind::Command,
                target: "cargo test".to_string()
            }]
        );
        let result = "{\"type\":\"result\",\"subtype\":\"success\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3}}";
        assert_eq!(
            parse_claude_event(result),
            vec![LoopEvent::Usage {
                input_tokens: 12,
                output_tokens: 3
            }]
        );
    }

    #[test]
    fn parses_codex_command_and_message() {
        let started = "{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\",\"command\":\"ls -la\",\"status\":\"in_progress\"}}";
        assert_eq!(
            parse_codex_event(started),
            vec![LoopEvent::Action {
                kind: ActionKind::Command,
                target: "ls -la".to_string()
            }]
        );
        let msg = "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"VERDICT: PASS\"}}";
        assert_eq!(
            parse_codex_event(msg),
            vec![LoopEvent::Message {
                text: "VERDICT: PASS".to_string()
            }]
        );
    }

    #[test]
    fn shortens_long_paths() {
        let long = "/private/tmp/claude/very/deep/workspace/src/lib.rs";
        assert_eq!(shorten_target(long), "…/src/lib.rs");
        assert_eq!(shorten_target("src/lib.rs"), "src/lib.rs");
    }

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

