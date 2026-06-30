//! A render-oriented projection of a step's activity into typed **cells**.
//!
//! Where [`LoopEvent`](super::event::LoopEvent) is the low-level normalized stream, a
//! [`Cell`] is a unit a front-end renders directly: an executed command with its output, a
//! file diff, assistant markdown, reasoning, a tool action, or a notice. The same cell
//! vocabulary drives every surface, and cells serialize to one JSON line each so a recorded
//! run replays identically.

use super::event::{ActionKind, LoopEvent};
use crate::hub::json::{esc, field_i64, field_str};

/// The lifecycle of an executed command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecState {
    Running,
    Done,
    Failed,
}

impl ExecState {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecState::Running => "running",
            ExecState::Done => "done",
            ExecState::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Option<ExecState> {
        Some(match s {
            "running" => ExecState::Running,
            "done" => ExecState::Done,
            "failed" => ExecState::Failed,
            _ => return None,
        })
    }
}

/// How a file changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Add,
    Modify,
    Delete,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Add => "add",
            ChangeKind::Modify => "modify",
            ChangeKind::Delete => "delete",
        }
    }
    pub fn parse(s: &str) -> Option<ChangeKind> {
        Some(match s {
            "add" => ChangeKind::Add,
            "modify" => ChangeKind::Modify,
            "delete" => ChangeKind::Delete,
            _ => return None,
        })
    }
}

/// The severity of a [`Cell::Notice`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeLevel {
    Info,
    Usage,
    Error,
}

impl NoticeLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            NoticeLevel::Info => "info",
            NoticeLevel::Usage => "usage",
            NoticeLevel::Error => "error",
        }
    }
    pub fn parse(s: &str) -> Option<NoticeLevel> {
        Some(match s {
            "info" => NoticeLevel::Info,
            "usage" => NoticeLevel::Usage,
            "error" => NoticeLevel::Error,
            _ => return None,
        })
    }
}

/// One hunk of a unified diff: its `@@ … @@` header and the lines beneath it (each line
/// keeps its leading `+` / `-` / ` ` so a renderer can color it).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<String>,
}

/// A render-ready unit of a step's activity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cell {
    /// A shell command, its (possibly streaming) output, and its outcome.
    Exec {
        command: String,
        output: String,
        exit_code: Option<i32>,
        state: ExecState,
    },
    /// A file edit, carrying the raw unified diff (parse with [`parse_hunks`] to render).
    Diff {
        file: String,
        change: ChangeKind,
        diff: String,
    },
    /// Assistant markdown.
    Markdown { text: String },
    /// Reasoning / "thinking" (folded by default in a UI).
    Reasoning { text: String },
    /// A tool action (read / edit / write / run / search).
    Action { kind: ActionKind, target: String },
    /// A model banner, token usage, or an error.
    Notice { level: NoticeLevel, text: String },
}

impl Cell {
    /// Project a normalized [`LoopEvent`] onto a cell — every event of today's stream maps.
    pub fn from_event(event: &LoopEvent) -> Option<Cell> {
        Some(match event {
            LoopEvent::Model { name } => Cell::Notice {
                level: NoticeLevel::Info,
                text: format!("model: {name}"),
            },
            LoopEvent::Action { kind, target } => Cell::Action {
                kind: *kind,
                target: target.clone(),
            },
            LoopEvent::Message { text } => Cell::Markdown { text: text.clone() },
            LoopEvent::Reasoning { text } => Cell::Reasoning { text: text.clone() },
            LoopEvent::Output { text } => Cell::Exec {
                command: String::new(),
                output: text.clone(),
                exit_code: None,
                state: ExecState::Done,
            },
            LoopEvent::Plan { text } => Cell::Markdown { text: text.clone() },
            LoopEvent::Usage {
                input_tokens,
                output_tokens,
            } => Cell::Notice {
                level: NoticeLevel::Usage,
                text: format!("{input_tokens} in / {output_tokens} out"),
            },
        })
    }

    /// Serialize as one JSON line.
    pub fn to_json_line(&self) -> String {
        match self {
            Cell::Exec {
                command,
                output,
                exit_code,
                state,
            } => {
                let code = exit_code.map(|c| c.to_string()).unwrap_or_else(|| "null".to_string());
                format!(
                    "{{\"cell\":\"exec\",\"command\":\"{}\",\"output\":\"{}\",\"exit_code\":{},\"state\":\"{}\"}}",
                    esc(command),
                    esc(output),
                    code,
                    state.as_str()
                )
            }
            Cell::Diff { file, change, diff } => format!(
                "{{\"cell\":\"diff\",\"file\":\"{}\",\"change\":\"{}\",\"diff\":\"{}\"}}",
                esc(file),
                change.as_str(),
                esc(diff)
            ),
            Cell::Markdown { text } => {
                format!("{{\"cell\":\"markdown\",\"text\":\"{}\"}}", esc(text))
            }
            Cell::Reasoning { text } => {
                format!("{{\"cell\":\"reasoning\",\"text\":\"{}\"}}", esc(text))
            }
            Cell::Action { kind, target } => format!(
                "{{\"cell\":\"action\",\"kind\":\"{}\",\"target\":\"{}\"}}",
                kind.as_str(),
                esc(target)
            ),
            Cell::Notice { level, text } => format!(
                "{{\"cell\":\"notice\",\"level\":\"{}\",\"text\":\"{}\"}}",
                level.as_str(),
                esc(text)
            ),
        }
    }

    /// Parse one JSON line (inverse of [`Cell::to_json_line`]).
    pub fn parse_line(line: &str) -> Option<Cell> {
        Some(match field_str(line, "cell")?.as_str() {
            "exec" => Cell::Exec {
                command: field_str(line, "command")?,
                output: field_str(line, "output").unwrap_or_default(),
                exit_code: field_i64(line, "exit_code").map(|c| c as i32),
                state: ExecState::parse(&field_str(line, "state")?)?,
            },
            "diff" => Cell::Diff {
                file: field_str(line, "file")?,
                change: ChangeKind::parse(&field_str(line, "change")?)?,
                diff: field_str(line, "diff").unwrap_or_default(),
            },
            "markdown" => Cell::Markdown {
                text: field_str(line, "text").unwrap_or_default(),
            },
            "reasoning" => Cell::Reasoning {
                text: field_str(line, "text").unwrap_or_default(),
            },
            "action" => Cell::Action {
                kind: ActionKind::parse(&field_str(line, "kind")?)?,
                target: field_str(line, "target").unwrap_or_default(),
            },
            "notice" => Cell::Notice {
                level: NoticeLevel::parse(&field_str(line, "level")?)?,
                text: field_str(line, "text").unwrap_or_default(),
            },
            _ => return None,
        })
    }
}

/// Project a normalized event stream onto cells.
pub fn cells_from_events(events: &[LoopEvent]) -> Vec<Cell> {
    events.iter().filter_map(Cell::from_event).collect()
}

/// Render a list of cells as JSONL.
pub fn cells_to_jsonl(cells: &[Cell]) -> String {
    let mut out = String::new();
    for cell in cells {
        out.push_str(&cell.to_json_line());
        out.push('\n');
    }
    out
}

/// Split a unified diff into hunks. Lines before the first `@@` header (the file header) are
/// dropped; each hunk keeps its lines verbatim (with `+` / `-` / ` ` prefixes).
pub fn parse_hunks(diff: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<Hunk> = None;
    for line in diff.lines() {
        if line.starts_with("@@") {
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            current = Some(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
        } else if let Some(h) = current.as_mut() {
            h.lines.push(line.to_string());
        }
    }
    if let Some(h) = current.take() {
        hunks.push(h);
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cells() -> Vec<Cell> {
        vec![
            Cell::Exec {
                command: "cargo test".to_string(),
                output: "running 3 tests\nok".to_string(),
                exit_code: Some(0),
                state: ExecState::Done,
            },
            Cell::Exec {
                command: "false".to_string(),
                output: String::new(),
                exit_code: Some(1),
                state: ExecState::Failed,
            },
            Cell::Exec {
                command: "sleep 1".to_string(),
                output: String::new(),
                exit_code: None,
                state: ExecState::Running,
            },
            Cell::Diff {
                file: "src/lib.rs".to_string(),
                change: ChangeKind::Add,
                diff: "@@ -0,0 +1 @@\n+pub fn f() {}".to_string(),
            },
            Cell::Markdown {
                text: "done \"ok\"\nnext".to_string(),
            },
            Cell::Reasoning {
                text: "needs a checked_mul".to_string(),
            },
            Cell::Action {
                kind: ActionKind::Edit,
                target: "src/lib.rs".to_string(),
            },
            Cell::Notice {
                level: NoticeLevel::Error,
                text: "boom".to_string(),
            },
        ]
    }

    #[test]
    fn cells_round_trip_through_jsonl() {
        let cells = sample_cells();
        let jsonl = cells_to_jsonl(&cells);
        let parsed: Vec<Cell> = jsonl.lines().filter_map(Cell::parse_line).collect();
        assert_eq!(parsed, cells);
        assert_eq!(Cell::parse_line("not json"), None);
    }

    #[test]
    fn events_project_onto_cells() {
        let events = vec![
            LoopEvent::Model { name: "claude-x".to_string() },
            LoopEvent::Action {
                kind: ActionKind::Command,
                target: "cargo test".to_string(),
            },
            LoopEvent::Message { text: "fixed it".to_string() },
            LoopEvent::Usage { input_tokens: 1200, output_tokens: 340 },
        ];
        let cells = cells_from_events(&events);
        assert!(matches!(cells[0], Cell::Notice { level: NoticeLevel::Info, .. }));
        assert!(matches!(cells[1], Cell::Action { kind: ActionKind::Command, .. }));
        assert!(matches!(cells[2], Cell::Markdown { .. }));
        assert!(matches!(cells[3], Cell::Notice { level: NoticeLevel::Usage, .. }));
    }

    #[test]
    fn parse_hunks_splits_a_unified_diff() {
        let diff = "--- a/x\n+++ b/x\n@@ -1,2 +1,3 @@\n context\n-old\n+new\n@@ -10,1 +11,1 @@\n+tail";
        let hunks = parse_hunks(diff);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].header, "@@ -1,2 +1,3 @@");
        assert_eq!(hunks[0].lines, vec![" context", "-old", "+new"]);
        assert_eq!(hunks[1].header, "@@ -10,1 +11,1 @@");
        assert_eq!(hunks[1].lines, vec!["+tail"]);
        assert!(parse_hunks("no hunks here").is_empty());
    }
}
