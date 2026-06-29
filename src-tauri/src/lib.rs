//! Loope Desktop backend.
//!
//! A thin Tauri layer over the std-only `loope::hub` core: every command maps a hub
//! function to an IPC call, converting the core's plain types into serde DTOs (so the
//! `loope` crate stays serde-free). The live run bridge that streams the engine's event
//! stream to the webview is added on top of these read commands.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use loope::adapter::cell::{
    Cell, ChangeKind, ExecState, NoticeLevel, cells_from_events, parse_hunks,
};
use loope::adapter::event::parse_event_line;
use loope::hub::registry::{Capabilities, RealProber};
use loope::hub::{AgentRegistry, Store, discover, search};
use serde::Serialize;

/// Managed application state: the agent registry (with its detection cache).
struct AppState {
    registry: AgentRegistry,
}

impl AppState {
    fn new() -> Self {
        Self {
            registry: AgentRegistry::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AgentDto {
    id: String,
    name: String,
    binary: String,
    available: bool,
    version: Option<String>,
    install_hint: String,
    capabilities: Vec<&'static str>,
}

#[derive(Serialize)]
struct SessionDto {
    id: String,
    dir: String,
    requirement: String,
    converged: bool,
    iterations: usize,
    stop_reason: String,
    has_highlight: bool,
    name: Option<String>,
}

#[derive(Serialize)]
struct ProjectDto {
    path: String,
    name: String,
    run_count: usize,
    last_active: Option<u64>,
    sessions: Vec<SessionDto>,
}

#[derive(Serialize)]
struct HitDto {
    project_path: String,
    session_id: String,
    source: String,
    line: usize,
    preview: String,
}

#[derive(Serialize)]
struct HunkDto {
    header: String,
    lines: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum CellDto {
    Exec {
        command: String,
        output: String,
        exit_code: Option<i32>,
        state: String,
    },
    Diff {
        file: String,
        change: String,
        hunks: Vec<HunkDto>,
    },
    Markdown {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Action {
        action: String,
        target: String,
    },
    Notice {
        level: String,
        text: String,
    },
}

#[derive(Serialize)]
struct StepDto {
    iteration: usize,
    num: usize,
    role: String,
    adapter: String,
    cells: Vec<CellDto>,
}

#[derive(Serialize)]
struct RunDto {
    id: String,
    requirement: String,
    converged: bool,
    iterations: usize,
    stop_reason: String,
    steps: Vec<StepDto>,
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

fn capability_labels(caps: Capabilities) -> Vec<&'static str> {
    let mut out = Vec::new();
    for (flag, label) in [
        (Capabilities::STREAM_TEXT, "text"),
        (Capabilities::STREAM_TOOLS, "tools"),
        (Capabilities::STREAM_REASONING, "reasoning"),
        (Capabilities::IMAGE_INPUT, "image"),
        (Capabilities::RESUME, "resume"),
        (Capabilities::CONFIG, "config"),
    ] {
        if caps.contains(flag) {
            out.push(label);
        }
    }
    out
}

fn epoch_secs(t: Option<SystemTime>) -> Option<u64> {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn cell_to_dto(cell: Cell) -> CellDto {
    match cell {
        Cell::Exec {
            command,
            output,
            exit_code,
            state,
        } => CellDto::Exec {
            command,
            output,
            exit_code,
            state: match state {
                ExecState::Running => "running",
                ExecState::Done => "done",
                ExecState::Failed => "failed",
            }
            .to_string(),
        },
        Cell::Diff { file, change, diff } => CellDto::Diff {
            file,
            change: match change {
                ChangeKind::Add => "add",
                ChangeKind::Modify => "modify",
                ChangeKind::Delete => "delete",
            }
            .to_string(),
            hunks: parse_hunks(&diff)
                .into_iter()
                .map(|h| HunkDto {
                    header: h.header,
                    lines: h.lines,
                })
                .collect(),
        },
        Cell::Markdown { text } => CellDto::Markdown { text },
        Cell::Reasoning { text } => CellDto::Reasoning { text },
        Cell::Action { kind, target } => CellDto::Action {
            action: kind.label().to_string(),
            target,
        },
        Cell::Notice { level, text } => CellDto::Notice {
            level: match level {
                NoticeLevel::Info => "info",
                NoticeLevel::Usage => "usage",
                NoticeLevel::Error => "error",
            }
            .to_string(),
            text,
        },
    }
}

fn session_to_dto(s: &loope::hub::Session) -> SessionDto {
    SessionDto {
        id: s.id.clone(),
        dir: s.dir.to_string_lossy().into_owned(),
        requirement: s.requirement.clone(),
        converged: s.converged,
        iterations: s.iterations,
        stop_reason: s.stop_reason.clone(),
        has_highlight: s.has_highlight,
        name: s.name.clone(),
    }
}

// ---------------------------------------------------------------------------
// Commands (read-only)
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_agents(state: tauri::State<'_, AppState>) -> Vec<AgentDto> {
    let prober = RealProber;
    state
        .registry
        .detect_all(&prober)
        .into_iter()
        .map(|(d, detected)| AgentDto {
            id: d.id().to_string(),
            name: d.display_name().to_string(),
            binary: d.binary().to_string(),
            available: detected.available,
            version: detected.version,
            install_hint: d.install_hint.to_string(),
            capabilities: capability_labels(d.capabilities),
        })
        .collect()
}

#[tauri::command]
fn list_projects() -> Result<Vec<ProjectDto>, String> {
    let store = Store::open().map_err(|e| e.to_string())?;
    let extra: Vec<PathBuf> = std::env::current_dir().ok().into_iter().collect();
    let projects = discover(&store, &extra);
    Ok(projects
        .iter()
        .map(|p| ProjectDto {
            path: p.path.to_string_lossy().into_owned(),
            name: p.name.clone(),
            run_count: p.run_count(),
            last_active: epoch_secs(p.last_active()),
            sessions: p.sessions.iter().map(session_to_dto).collect(),
        })
        .collect())
}

#[tauri::command]
fn read_run(run_dir: String) -> Result<RunDto, String> {
    let dir = PathBuf::from(&run_dir);
    let json = std::fs::read_to_string(dir.join("run.json")).map_err(|e| e.to_string())?;
    let steps = read_steps(&dir);
    Ok(RunDto {
        id: loope::hub::json::field_str(&json, "run_id").unwrap_or_default(),
        requirement: loope::hub::json::field_str(&json, "requirement").unwrap_or_default(),
        converged: loope::hub::json::field_bool(&json, "converged").unwrap_or(false),
        iterations: loope::hub::json::field_u64(&json, "iterations").unwrap_or(0) as usize,
        stop_reason: loope::hub::json::field_str(&json, "stop_reason").unwrap_or_default(),
        steps,
    })
}

/// Read each `agents/<NN>-<role>-<adapter>/` step into a [`StepDto`] of cells (events
/// projected onto cells, plus a diff cell when the step changed files).
fn read_steps(run_dir: &Path) -> Vec<StepDto> {
    let Ok(read) = std::fs::read_dir(run_dir.join("agents")) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();

    let mut steps = Vec::new();
    for dir in dirs {
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let parts: Vec<&str> = name.splitn(3, '-').collect();
        let num = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let role = parts.get(1).copied().unwrap_or("").to_string();
        let adapter = parts.get(2).copied().unwrap_or("").to_string();

        let mut cells = Vec::new();
        if let Ok(events_text) = std::fs::read_to_string(dir.join("events.jsonl")) {
            let events: Vec<_> = events_text.lines().filter_map(parse_event_line).collect();
            cells.extend(cells_from_events(&events).into_iter().map(cell_to_dto));
        }
        if let Ok(diff) = std::fs::read_to_string(dir.join("changes.diff"))
            && !diff.trim().is_empty()
        {
            cells.push(cell_to_dto(Cell::Diff {
                file: "changes".to_string(),
                change: ChangeKind::Modify,
                diff,
            }));
        }

        steps.push(StepDto {
            iteration: 0,
            num,
            role,
            adapter,
            cells,
        });
    }
    steps
}

#[tauri::command]
fn search_runs(query: String) -> Result<Vec<HitDto>, String> {
    let store = Store::open().map_err(|e| e.to_string())?;
    let extra: Vec<PathBuf> = std::env::current_dir().ok().into_iter().collect();
    let projects = discover(&store, &extra);
    let hits = search(&projects, &query);
    Ok(hits
        .into_iter()
        .map(|h| HitDto {
            project_path: h.project_path.to_string_lossy().into_owned(),
            session_id: h.session_id,
            source: h.source.to_string(),
            line: h.line,
            preview: h.preview,
        })
        .collect())
}

#[tauri::command]
fn add_project(path: String) -> Result<(), String> {
    Store::open()
        .and_then(|s| s.add_project(&path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_project(path: String) -> Result<(), String> {
    Store::open()
        .and_then(|s| s.remove_project(&path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_session_name(id: String, name: String) -> Result<(), String> {
    Store::open()
        .and_then(|s| s.set_session_name(&id, &name))
        .map_err(|e| e.to_string())
}

/// Entry point used by `main.rs`.
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            list_agents,
            list_projects,
            read_run,
            search_runs,
            add_project,
            remove_project,
            set_session_name,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Loope Desktop");
}
