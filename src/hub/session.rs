//! A session is one Loope run: a `.loope/runs/<id>/` directory, summarized from its
//! `run.json`. The hub reads it without depending on any front-end.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::json;

/// One run, as the hub presents it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    /// The run id (the `NNNN-slug` directory name).
    pub id: String,
    /// The run directory.
    pub dir: PathBuf,
    /// The requirement the run was given.
    pub requirement: String,
    /// Whether the loop converged.
    pub converged: bool,
    /// How many iterations it took.
    pub iterations: usize,
    /// Why the run stopped (`converged`, `max_iters`, …).
    pub stop_reason: String,
    /// Whether a "caught & fixed" highlight was recorded.
    pub has_highlight: bool,
    /// When the run directory was last modified (a proxy for recency).
    pub modified: Option<SystemTime>,
    /// A friendly name the user gave this session, if any.
    pub name: Option<String>,
}

impl Session {
    /// The label to show: the friendly name if set, else the id.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

/// Load a session from its run directory, or `None` if it has no `run.json`.
pub fn load_session(dir: &Path) -> Option<Session> {
    let json = std::fs::read_to_string(dir.join("run.json")).ok()?;
    let id = json::field_str(&json, "run_id")
        .filter(|s| !s.is_empty())
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let modified = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    Some(Session {
        id,
        dir: dir.to_path_buf(),
        requirement: json::field_str(&json, "requirement").unwrap_or_default(),
        converged: json::field_bool(&json, "converged").unwrap_or(false),
        iterations: json::field_u64(&json, "iterations").unwrap_or(0) as usize,
        stop_reason: json::field_str(&json, "stop_reason").unwrap_or_default(),
        has_highlight: json::field_bool(&json, "highlight").unwrap_or(false),
        modified,
        name: None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "loope-session-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// Write a minimal run directory with a `run.json`.
    pub(crate) fn write_run(parent: &Path, id: &str, json: &str) -> PathBuf {
        let dir = parent.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("run.json"), json).unwrap();
        dir
    }

    #[test]
    fn loads_a_run_summary() {
        let base = temp_dir("load");
        let dir = write_run(
            &base,
            "0008-add-auth",
            "{\"run_id\":\"0008-add-auth\",\"requirement\":\"add auth\",\"converged\":true,\"highlight\":true,\"iterations\":2,\"stop_reason\":\"converged\",\"steps\":[]}",
        );
        let s = load_session(&dir).unwrap();
        assert_eq!(s.id, "0008-add-auth");
        assert_eq!(s.requirement, "add auth");
        assert!(s.converged);
        assert!(s.has_highlight);
        assert_eq!(s.iterations, 2);
        assert_eq!(s.stop_reason, "converged");
        assert_eq!(s.label(), "0008-add-auth");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_run_json_is_none() {
        let base = temp_dir("none");
        std::fs::create_dir_all(base.join("empty")).unwrap();
        assert!(load_session(&base.join("empty")).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }
}
