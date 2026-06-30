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
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::Adapter;
use crate::adapter::{AgentInvocation, InvocationResult, Invoker, resolve_program, spec_for};
use crate::adapter::event::{ActionKind, LoopEvent};

/// An [`Invoker`] that runs the adapter's real CLI as a subprocess.
#[derive(Clone, Debug, Default)]
pub struct SubprocessInvoker {
    /// When true, point each agent's CLI config/home at its private run directory.
    /// Default (false) reuses the user's normal login so authentication works.
    pub isolate_home: bool,
    /// Optional `provider/model` passed to OpenCode as `-m` when its default provider
    /// is not usable. Resolved by the CLI from `--opencode-model` / `LOOPE_OPENCODE_MODEL`.
    pub opencode_model: Option<String>,
    /// Per-step wall-clock bound. `None` disables it. On timeout the child is killed
    /// and the step fails with a "timed out" message.
    pub timeout: Option<Duration>,
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
        configure_command(
            &mut cmd,
            inv,
            self.isolate_home,
            codex_output.as_deref(),
            self.opencode_model.as_deref(),
        );
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                return InvocationResult::failure(format!("failed to launch '{program}': {err}"));
            }
        };

        // Deliver the prompt: Claude/Codex read it on stdin; OpenCode takes it as a
        // `run` argument (set in configure_command). Always close stdin so the CLI starts.
        if let Some(mut stdin) = child.stdin.take()
            && matches!(inv.adapter, Adapter::Claude | Adapter::Codex)
        {
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

        // Read stdout on a thread (sending lines back) so the main thread can enforce a
        // deadline: a hung agent that never writes or exits would otherwise block forever.
        let (tx, rx) = mpsc::channel::<String>();
        let stdout_handle = child.stdout.take().map(|out| {
            thread::spawn(move || {
                for line in BufReader::new(out).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
        });

        let started = Instant::now();
        let mut stdout = String::new();
        let mut timed_out = false;
        loop {
            let remaining = match self.timeout {
                Some(limit) => match limit.checked_sub(started.elapsed()) {
                    Some(left) if !left.is_zero() => left,
                    _ => {
                        timed_out = true;
                        let _ = child.kill();
                        break;
                    }
                },
                None => Duration::from_secs(3600),
            };
            match rx.recv_timeout(remaining) {
                Ok(line) => {
                    for event in parse_event(inv.adapter, &line) {
                        sink(event);
                    }
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.timeout.is_some() {
                        timed_out = true;
                        let _ = child.kill();
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let status = child.wait();
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);

        if timed_out {
            // Do not join the reader/stderr threads: a grandchild the agent spawned may
            // still hold those pipes open, which would block. Detach them (they end when
            // the pipes close or at process exit) so the loop itself never hangs.
            drop(stdout_handle);
            drop(stderr_handle);
            let secs = self.timeout.map(|d| d.as_secs()).unwrap_or(0);
            return InvocationResult {
                success: false,
                message: format!("timed out after {secs}s"),
                changed_files: Vec::new(),
                transcript: format!("--- stdout ---\n{stdout}\n--- (timed out; streams detached) ---\n"),
            };
        }

        if let Some(handle) = stdout_handle {
            let _ = handle.join();
        }
        let stderr = stderr_handle
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        let exit_ok = code == 0;

        // OpenCode reports provider/auth failures as a `type:"error"` event even when it
        // exits cleanly; treat that as a failed step carrying the error message.
        let opencode_error = (inv.adapter == Adapter::OpenCode)
            .then(|| extract_opencode_error(&stdout))
            .flatten();
        let success = exit_ok && opencode_error.is_none();

        let message = if let Some(err) = opencode_error {
            err
        } else if success {
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
        Adapter::OpenCode => extract_opencode_message(stdout),
        _ => {
            let trimmed = stdout.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
    }
}

/// Extract OpenCode's error message from a `--format json` stream, if it reported one.
fn extract_opencode_error(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if line.contains("\"type\":\"error\"")
            && let Some(message) = extract_json_string_field(line, "message")
            && !message.trim().is_empty()
        {
            return Some(message.trim().to_string());
        }
    }
    None
}

/// Extract OpenCode's final assistant message: the last assistant `text` in the stream.
fn extract_opencode_message(stdout: &str) -> Option<String> {
    let mut last = None;
    for line in stdout.lines() {
        for event in parse_opencode_event(line) {
            if let LoopEvent::Message { text } = event {
                last = Some(text);
            }
        }
    }
    last
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
    opencode_model: Option<&str>,
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
            // Non-interactive run with a structured event stream; the prompt is the
            // `run` message argument, and the workspace is the working directory.
            cmd.arg("run");
            cmd.args(["--format", "json"]);
            cmd.arg("--dir").arg(&inv.workspace_dir);
            if let Some(model) = opencode_model {
                cmd.args(["-m", model]);
            }
            // OpenCode has no read-only run flag; the role's read-only intent is carried
            // in the prompt. The prompt is delivered as the message argument (last).
            cmd.arg(&inv.prompt);
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

/// Collect every `"key":"value"` string in a line, in order (reversing the common escapes).
/// Used to pull a `TodoWrite` plan's parallel `content` / `status` arrays without a JSON
/// parser.
fn collect_string_fields(line: &str, field: &str) -> Vec<String> {
    let key = format!("\"{field}\":\"");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = line[from..].find(&key) {
        let start = from + rel + key.len();
        let mut val = String::new();
        let mut escaped = false;
        let mut end = start;
        for c in line[start..].chars() {
            end += c.len_utf8();
            if escaped {
                match c {
                    'n' => val.push('\n'),
                    't' => val.push('\t'),
                    'r' => val.push('\r'),
                    o => val.push(o),
                }
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                break;
            } else {
                val.push(c);
            }
        }
        out.push(val);
        from = end;
    }
    out
}

/// Bound a reasoning/thinking snippet to a short, single-ish line for the feed.
fn shorten_reasoning(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > 200 {
        let t: String = joined.chars().take(199).collect();
        format!("{t}…")
    } else {
        joined
    }
}

/// Bound a tool result / command output: keep the last ~12 non-blank lines (errors live at
/// the tail), each capped, total capped — so a full `cargo test` log stays glanceable.
fn shorten_output(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail: &[&str] = if lines.len() > 12 {
        &lines[lines.len() - 12..]
    } else {
        &lines
    };
    let mut out = String::new();
    for line in tail {
        let capped: String = line.chars().take(160).collect();
        out.push_str(capped.trim_end());
        out.push('\n');
    }
    let out = out.trim_end().to_string();
    if out.chars().count() > 800 {
        let t: String = out.chars().take(799).collect();
        format!("{t}…")
    } else {
        out
    }
}

/// Format a `TodoWrite` plan as a checklist from its parallel content/status arrays.
fn format_plan(contents: &[String], statuses: &[String]) -> String {
    let mut out = String::new();
    for (i, content) in contents.iter().enumerate() {
        let mark = match statuses.get(i).map(String::as_str) {
            Some("completed") => "[x]",
            Some("in_progress") => "[~]",
            _ => "[ ]",
        };
        out.push_str(&format!("- {mark} {}\n", content.trim()));
    }
    out.trim_end().to_string()
}

/// Parse one Claude `--output-format stream-json` line into normalized events.
/// Content blocks are bounded by splitting on the `{"type":"` marker.
fn parse_claude_event(line: &str) -> Vec<LoopEvent> {
    let mut events = Vec::new();
    for seg in line.split("{\"type\":\"") {
        if let Some(rest) = seg.strip_prefix("tool_use\"") {
            if let Some(name) = extract_json_string_field(rest, "name") {
                if name == "TodoWrite" {
                    // A plan update: render the todos as a checklist rather than a bare action.
                    let contents = collect_string_fields(rest, "content");
                    if !contents.is_empty() {
                        let statuses = collect_string_fields(rest, "status");
                        events.push(LoopEvent::Plan {
                            text: format_plan(&contents, &statuses),
                        });
                    }
                } else {
                    let target = extract_json_string_field(rest, "file_path")
                        .or_else(|| extract_json_string_field(rest, "command"))
                        .or_else(|| extract_json_string_field(rest, "url"))
                        .or_else(|| extract_json_string_field(rest, "pattern"))
                        .or_else(|| extract_json_string_field(rest, "path"))
                        .or_else(|| extract_json_string_field(rest, "description"))
                        .unwrap_or_else(|| name.clone());
                    events.push(LoopEvent::Action {
                        kind: map_claude_tool(&name),
                        target: shorten_target(&target),
                    });
                }
            }
        } else if let Some(rest) = seg.strip_prefix("thinking\"") {
            if let Some(text) = extract_json_string_field(rest, "thinking")
                && !text.trim().is_empty()
            {
                events.push(LoopEvent::Reasoning {
                    text: shorten_reasoning(text.trim()),
                });
            }
        } else if let Some(rest) = seg.strip_prefix("tool_result\"") {
            // A tool's result (e.g. a Bash command's output). String content is the common
            // shape; array content falls back to its first text part.
            if let Some(body) = extract_json_string_field(rest, "content")
                .or_else(|| extract_json_string_field(rest, "text"))
            {
                let bounded = shorten_output(&body);
                if !bounded.trim().is_empty() {
                    events.push(LoopEvent::Output { text: bounded });
                }
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
    // Reasoning items (`reasoning` / `agent_reasoning`).
    if line.contains("\"reasoning\"")
        && let Some(text) = extract_json_string_field(line, "text")
        && !text.trim().is_empty()
    {
        events.push(LoopEvent::Reasoning {
            text: shorten_reasoning(text.trim()),
        });
    }
    // A completed command's aggregated output.
    if line.contains("\"item.completed\"")
        && line.contains("\"command_execution\"")
        && let Some(out) = extract_json_string_field(line, "aggregated_output")
        && !out.trim().is_empty()
    {
        events.push(LoopEvent::Output {
            text: shorten_output(&out),
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
        Adapter::OpenCode => parse_opencode_event(line),
        _ => Vec::new(),
    }
}

/// Parse one OpenCode `--format json` line into normalized events.
///
/// OpenCode emits JSONL events keyed by `type`. Assistant `text` parts become messages,
/// tool/file parts become actions; `error` events yield nothing here (the failure is
/// surfaced from the invocation result). Field handling is intentionally lenient.
fn parse_opencode_event(line: &str) -> Vec<LoopEvent> {
    let mut events = Vec::new();
    if line.contains("\"type\":\"error\"") {
        return events;
    }
    // A tool/file part: prefer an explicit tool name, then a path/file target.
    if (line.contains("\"tool\"") || line.contains("\"type\":\"tool\""))
        && let Some(tool) = extract_json_string_field(line, "tool")
    {
        let target = extract_json_string_field(line, "path")
            .or_else(|| extract_json_string_field(line, "file"))
            .or_else(|| extract_json_string_field(line, "filePath"))
            .or_else(|| extract_json_string_field(line, "command"))
            .unwrap_or_else(|| tool.clone());
        events.push(LoopEvent::Action {
            kind: map_opencode_tool(&tool),
            target: shorten_target(&target),
        });
    }
    // An assistant text part.
    if line.contains("\"type\":\"text\"")
        && let Some(text) = extract_json_string_field(line, "text")
        && !text.trim().is_empty()
    {
        events.push(LoopEvent::Message {
            text: shorten_message(text.trim()),
        });
    }
    // A reasoning part.
    if line.contains("\"type\":\"reasoning\"")
        && let Some(text) = extract_json_string_field(line, "text")
        && !text.trim().is_empty()
    {
        events.push(LoopEvent::Reasoning {
            text: shorten_reasoning(text.trim()),
        });
    }
    // A tool's output, when the part carries one.
    if (line.contains("\"tool\"") || line.contains("\"step-finish\""))
        && let Some(out) =
            extract_json_string_field(line, "output").or_else(|| extract_json_string_field(line, "stdout"))
        && !out.trim().is_empty()
    {
        events.push(LoopEvent::Output {
            text: shorten_output(&out),
        });
    }
    events
}

/// Map an OpenCode tool name to an action kind.
fn map_opencode_tool(name: &str) -> ActionKind {
    match name.to_ascii_lowercase().as_str() {
        "write" => ActionKind::Write,
        "edit" | "patch" | "multiedit" => ActionKind::Edit,
        "read" | "view" | "cat" => ActionKind::Read,
        "bash" | "shell" | "run" | "exec" => ActionKind::Command,
        "grep" | "glob" | "search" | "list" | "ls" => ActionKind::Search,
        "webfetch" | "fetch" | "web" => ActionKind::Fetch,
        "task" | "agent" | "subagent" => ActionKind::Task,
        _ => ActionKind::Other,
    }
}

/// Map a Claude tool name to an action kind.
fn map_claude_tool(name: &str) -> ActionKind {
    match name {
        "Write" => ActionKind::Write,
        "Edit" | "MultiEdit" | "NotebookEdit" => ActionKind::Edit,
        "Read" | "NotebookRead" => ActionKind::Read,
        "Bash" | "BashOutput" | "KillBash" => ActionKind::Command,
        "Grep" | "Glob" | "WebSearch" => ActionKind::Search,
        "WebFetch" => ActionKind::Fetch,
        "Task" => ActionKind::Task,
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
    } else if target.chars().count() > 64 {
        // Truncate by characters, not bytes — a byte slice would panic mid-UTF-8.
        let truncated: String = target.chars().take(63).collect();
        format!("{truncated}…")
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
    fn parses_opencode_text_and_tool() {
        let text = "{\"type\":\"text\",\"text\":\"Implemented slugify.\"}";
        assert_eq!(
            parse_opencode_event(text),
            vec![LoopEvent::Message {
                text: "Implemented slugify.".to_string()
            }]
        );
        let tool = "{\"type\":\"tool\",\"tool\":\"edit\",\"path\":\"src/lib.rs\"}";
        assert_eq!(
            parse_opencode_event(tool),
            vec![LoopEvent::Action {
                kind: ActionKind::Edit,
                target: "src/lib.rs".to_string()
            }]
        );
    }

    #[test]
    fn opencode_error_line_yields_no_event_but_extracts_message() {
        let err = "{\"type\":\"error\",\"sessionID\":\"x\",\"error\":{\"name\":\"APIError\",\"data\":{\"message\":\"Forbidden: unauthorized: not licensed to use Copilot\",\"statusCode\":403}}}";
        assert!(parse_opencode_event(err).is_empty());
        assert_eq!(
            extract_opencode_error(err).as_deref(),
            Some("Forbidden: unauthorized: not licensed to use Copilot")
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
    fn parses_claude_thinking_as_reasoning() {
        let line = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"thinking\",\"thinking\":\"The   bug is an overflow;\\nuse checked_mul.\"}]}}";
        assert_eq!(
            parse_claude_event(line),
            vec![LoopEvent::Reasoning {
                text: "The bug is an overflow; use checked_mul.".to_string()
            }]
        );
    }

    #[test]
    fn parses_claude_tool_result_as_output() {
        let line = "{\"type\":\"user\",\"message\":{\"content\":[\
            {\"type\":\"tool_result\",\"tool_use_id\":\"x\",\"content\":\"running 1 test\\ntest tests::ok ... ok\"}]}}";
        assert_eq!(
            parse_claude_event(line),
            vec![LoopEvent::Output {
                text: "running 1 test\ntest tests::ok ... ok".to_string()
            }]
        );
    }

    #[test]
    fn parses_claude_todowrite_as_plan() {
        let line = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"tool_use\",\"name\":\"TodoWrite\",\"input\":{\"todos\":[\
            {\"content\":\"Read the file\",\"status\":\"completed\",\"activeForm\":\"Reading\"},\
            {\"content\":\"Fix overflow\",\"status\":\"in_progress\",\"activeForm\":\"Fixing\"},\
            {\"content\":\"Add a test\",\"status\":\"pending\",\"activeForm\":\"Testing\"}]}}]}}";
        assert_eq!(
            parse_claude_event(line),
            vec![LoopEvent::Plan {
                text: "- [x] Read the file\n- [~] Fix overflow\n- [ ] Add a test".to_string()
            }]
        );
    }

    #[test]
    fn maps_claude_web_and_task_tools() {
        let fetch = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"tool_use\",\"name\":\"WebFetch\",\"input\":{\"url\":\"https://example.com/x\"}}]}}";
        assert_eq!(
            parse_claude_event(fetch),
            vec![LoopEvent::Action {
                kind: ActionKind::Fetch,
                target: "https://example.com/x".to_string()
            }]
        );
        let task = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"tool_use\",\"name\":\"Task\",\"input\":{\"description\":\"audit deps\"}}]}}";
        assert_eq!(
            parse_claude_event(task),
            vec![LoopEvent::Action {
                kind: ActionKind::Task,
                target: "audit deps".to_string()
            }]
        );
    }

    #[test]
    fn parses_codex_reasoning_and_command_output() {
        let reasoning = "{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\",\"text\":\"Plan: patch the guard.\"}}";
        assert_eq!(
            parse_codex_event(reasoning),
            vec![LoopEvent::Reasoning {
                text: "Plan: patch the guard.".to_string()
            }]
        );
        let done = "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"cargo test\",\"aggregated_output\":\"test result: ok. 3 passed\",\"exit_code\":0}}";
        assert_eq!(
            parse_codex_event(done),
            vec![LoopEvent::Output {
                text: "test result: ok. 3 passed".to_string()
            }]
        );
    }

    #[test]
    fn parses_opencode_reasoning() {
        let line = "{\"type\":\"reasoning\",\"text\":\"Considering the edge case.\"}";
        assert_eq!(
            parse_opencode_event(line),
            vec![LoopEvent::Reasoning {
                text: "Considering the edge case.".to_string()
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

    #[test]
    fn shorten_target_is_utf8_safe() {
        // Regression: a long non-ASCII target used to panic on a byte-index slice.
        let command = "é".repeat(100);
        let shortened = shorten_target(&command);
        assert!(shortened.ends_with('…'));
        assert!(shortened.chars().count() <= 64);

        // Long paths still keep their tail; short targets pass through.
        let path = format!("/a/very/long/{}/src/lib.rs", "x".repeat(60));
        assert!(shorten_target(&path).starts_with("…/"));
        assert_eq!(shorten_target("cargo test"), "cargo test");
    }
}

