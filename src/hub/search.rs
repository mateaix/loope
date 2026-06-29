//! Full-text search across the persisted run artifacts of discovered projects.
//!
//! Scans each session's `report.md` and its per-step `prompt.md` / `events.jsonl` /
//! `transcript.jsonl`, returning case-insensitive substring matches with a short preview.
//! Pure std; the CLI or a GUI can present the hits.

use std::path::{Path, PathBuf};

use super::project::Project;

/// One match found while searching.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub project_path: PathBuf,
    pub session_id: String,
    /// Which artifact matched: `report`, `prompt`, `events`, or `transcript`.
    pub source: &'static str,
    /// 1-based line number within the artifact.
    pub line: usize,
    /// A trimmed, length-capped preview of the matching line.
    pub preview: String,
}

/// At most this many matches per artifact file, to keep results bounded.
const MAX_PER_FILE: usize = 5;
const PREVIEW_CHARS: usize = 160;

/// Search every session of every project for `query` (case-insensitive). Empty queries
/// return nothing. Results are ordered project → session → artifact → line.
pub fn search(projects: &[Project], query: &str) -> Vec<SearchHit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for project in projects {
        for session in &project.sessions {
            let dir = &session.dir;
            scan(&dir.join("report.md"), "report", &needle, project, session, &mut hits);

            // Per-step artifacts live under `agents/<step>/`.
            if let Ok(steps) = std::fs::read_dir(dir.join("agents")) {
                let mut step_dirs: Vec<PathBuf> =
                    steps.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
                step_dirs.sort();
                for step in step_dirs {
                    scan(&step.join("prompt.md"), "prompt", &needle, project, session, &mut hits);
                    scan(&step.join("events.jsonl"), "events", &needle, project, session, &mut hits);
                    scan(
                        &step.join("transcript.jsonl"),
                        "transcript",
                        &needle,
                        project,
                        session,
                        &mut hits,
                    );
                }
            }
        }
    }
    hits
}

fn scan(
    path: &Path,
    source: &'static str,
    needle: &str,
    project: &Project,
    session: &super::session::Session,
    hits: &mut Vec<SearchHit>,
) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let mut found = 0;
    for (i, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(needle) {
            hits.push(SearchHit {
                project_path: project.path.clone(),
                session_id: session.id.clone(),
                source,
                line: i + 1,
                preview: preview(line),
            });
            found += 1;
            if found >= MAX_PER_FILE {
                break;
            }
        }
    }
}

/// Trim and cap a line for display.
fn preview(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= PREVIEW_CHARS {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(PREVIEW_CHARS).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::super::project::{discover_project, runs_dir};
    use super::super::session::tests::{temp_dir, write_run};
    use super::super::store::Store;
    use super::*;

    fn run_json(id: &str) -> String {
        format!(
            "{{\"run_id\":\"{id}\",\"requirement\":\"r\",\"converged\":true,\"highlight\":false,\"iterations\":1,\"stop_reason\":\"converged\",\"steps\":[]}}"
        )
    }

    #[test]
    fn finds_matches_across_artifacts() {
        let project = temp_dir("search");
        let runs = runs_dir(&project);
        std::fs::create_dir_all(&runs).unwrap();
        let dir = write_run(&runs, "0001-x", &run_json("0001-x"));
        std::fs::write(dir.join("report.md"), "# Report\nadded the multiply function\n").unwrap();
        let step = dir.join("agents").join("01-implementer-claude");
        std::fs::create_dir_all(&step).unwrap();
        std::fs::write(
            step.join("transcript.jsonl"),
            "{\"text\":\"I will MULTIPLY two numbers\"}\nunrelated line\n",
        )
        .unwrap();

        let store = Store::at(temp_dir("store")).unwrap();
        let p = discover_project(&project, &store).unwrap();

        let hits = search(&[p], "multiply");
        assert_eq!(hits.len(), 2, "report.md + transcript.jsonl");
        assert!(hits.iter().any(|h| h.source == "report" && h.line == 2));
        assert!(hits.iter().any(|h| h.source == "transcript" && h.line == 1));
        assert!(hits.iter().all(|h| h.session_id == "0001-x"));

        // Empty / non-matching queries.
        assert!(search(&[discover_project(&project, &store).unwrap()], "  ").is_empty());
        assert!(search(&[discover_project(&project, &store).unwrap()], "zzz").is_empty());

        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(store.dir());
    }
}
