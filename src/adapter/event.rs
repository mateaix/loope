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

    /// Parse a persisted action id (inverse of [`ActionKind::as_str`]).
    pub fn parse(id: &str) -> Option<ActionKind> {
        Some(match id {
            "read" => ActionKind::Read,
            "edit" => ActionKind::Edit,
            "write" => ActionKind::Write,
            "command" => ActionKind::Command,
            "search" => ActionKind::Search,
            "other" => ActionKind::Other,
            _ => return None,
        })
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

/// Parse one `events.jsonl` line back into a [`LoopEvent`] (inverse of
/// [`LoopEvent::to_json_line`]), so a recorded run's data stream can be replayed.
pub fn parse_event_line(line: &str) -> Option<LoopEvent> {
    match json_str(line, "type")?.as_str() {
        "model" => Some(LoopEvent::Model {
            name: json_str(line, "name")?,
        }),
        "action" => Some(LoopEvent::Action {
            kind: ActionKind::parse(&json_str(line, "kind")?)?,
            target: json_str(line, "target").unwrap_or_default(),
        }),
        "message" => Some(LoopEvent::Message {
            text: json_str(line, "text").unwrap_or_default(),
        }),
        "usage" => Some(LoopEvent::Usage {
            input_tokens: json_u64(line, "input_tokens").unwrap_or(0),
            output_tokens: json_u64(line, "output_tokens").unwrap_or(0),
        }),
        _ => None,
    }
}

/// Extract a `"key":"value"` string, reversing [`esc`].
fn json_str(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let mut out = String::new();
    let mut chars = line[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Extract a `"key":<number>` field.
fn json_u64(line: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
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

    #[test]
    fn events_round_trip_through_jsonl() {
        let events = vec![
            LoopEvent::Model { name: "claude-x".to_string() },
            LoopEvent::Action { kind: ActionKind::Command, target: "cargo test".to_string() },
            LoopEvent::Message { text: "fixed\n\"the\" bug\ttabs".to_string() },
            LoopEvent::Usage { input_tokens: 1200, output_tokens: 340 },
        ];
        let jsonl = events_to_jsonl(&events);
        let parsed: Vec<LoopEvent> = jsonl.lines().filter_map(parse_event_line).collect();
        assert_eq!(parsed, events);
        assert_eq!(parse_event_line("not json"), None);
    }
}
