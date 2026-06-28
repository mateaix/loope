//! Normalized agent events.
//!
//! Both the Claude and Codex CLIs stream rich JSONL event data; their adapter parsers
//! map each line onto this small shared vocabulary so the UI and persistence never
//! depend on a specific CLI's schema.

/// The kind of action an agent took.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionKind {
    Read,
    Edit,
    Write,
    Command,
    Search,
    Other,
}

impl ActionKind {
    /// Short verb for display.
    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Read => "read",
            ActionKind::Edit => "edit",
            ActionKind::Write => "write",
            ActionKind::Command => "run",
            ActionKind::Search => "search",
            ActionKind::Other => "do",
        }
    }

    /// Stable lowercase id for persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            ActionKind::Read => "read",
            ActionKind::Edit => "edit",
            ActionKind::Write => "write",
            ActionKind::Command => "command",
            ActionKind::Search => "search",
            ActionKind::Other => "other",
        }
    }
}

/// A single normalized event from an agent's run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoopEvent {
    /// The model a step is using.
    Model { name: String },
    /// A tool action: file read/edit/write, command, search.
    Action { kind: ActionKind, target: String },
    /// Assistant text (kept short for display; full text stays in the transcript).
    Message { text: String },
    /// Token usage, when the CLI reports it.
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
}

impl LoopEvent {
    /// Serialize as one JSON line for `events.jsonl` (hand-rolled; no serde).
    pub fn to_json_line(&self) -> String {
        match self {
            LoopEvent::Model { name } => {
                format!("{{\"type\":\"model\",\"name\":\"{}\"}}", esc(name))
            }
            LoopEvent::Action { kind, target } => format!(
                "{{\"type\":\"action\",\"kind\":\"{}\",\"target\":\"{}\"}}",
                kind.as_str(),
                esc(target)
            ),
            LoopEvent::Message { text } => {
                format!("{{\"type\":\"message\",\"text\":\"{}\"}}", esc(text))
            }
            LoopEvent::Usage {
                input_tokens,
                output_tokens,
            } => format!(
                "{{\"type\":\"usage\",\"input_tokens\":{input_tokens},\"output_tokens\":{output_tokens}}}"
            ),
        }
    }
}

/// Render a list of events as JSONL.
pub fn events_to_jsonl(events: &[LoopEvent]) -> String {
    let mut out = String::new();
    for event in events {
        out.push_str(&event.to_json_line());
        out.push('\n');
    }
    out
}

/// Minimal JSON string escaping.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_kinds_have_labels_and_ids() {
        assert_eq!(ActionKind::Edit.label(), "edit");
        assert_eq!(ActionKind::Command.label(), "run");
        assert_eq!(ActionKind::Command.as_str(), "command");
    }

    #[test]
    fn events_serialize_to_jsonl() {
        let events = vec![
            LoopEvent::Action {
                kind: ActionKind::Edit,
                target: "src/lib.rs".to_string(),
            },
            LoopEvent::Message {
                text: "done \"ok\"".to_string(),
            },
        ];
        let jsonl = events_to_jsonl(&events);
        assert!(jsonl.contains("\"type\":\"action\",\"kind\":\"edit\",\"target\":\"src/lib.rs\""));
        assert!(jsonl.contains("\\\"ok\\\""));
        assert_eq!(jsonl.lines().count(), 2);
    }
}
