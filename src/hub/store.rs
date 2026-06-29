//! The local hub metadata store under `~/.loope/`.
//!
//! This holds Loope's own cross-project metadata — app state and friendly session names
//! today, presets and project lists later — kept separate from each project's
//! `.loope/runs/` artifacts. Files are small JSON objects of string→string, written
//! atomically. The JSON is hand-rolled (no serde) to keep the crate std-only.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::json;

/// A handle to the `~/.loope/` metadata directory.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// Open (creating if needed) the store under `$HOME/.loope`.
    pub fn open() -> io::Result<Store> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        Store::at(home.join(".loope"))
    }

    /// Open (creating if needed) the store at an explicit directory — used by tests.
    pub fn at(dir: impl Into<PathBuf>) -> io::Result<Store> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Store { dir })
    }

    /// The store directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Load a string→string map from `name`, or an empty map if it is missing/unreadable.
    pub fn load_map(&self, name: &str) -> BTreeMap<String, String> {
        std::fs::read_to_string(self.dir.join(name))
            .map(|s| json::parse_object(&s))
            .unwrap_or_default()
    }

    /// Atomically write a string→string map to `name`.
    pub fn save_map(&self, name: &str, map: &BTreeMap<String, String>) -> io::Result<()> {
        atomic_write(&self.dir.join(name), &json::write_object(map))
    }

    /// Load a string list from `name`, or an empty list if it is missing/unreadable.
    pub fn load_list(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(name))
            .map(|s| json::parse_array(&s))
            .unwrap_or_default()
    }

    /// Atomically write a string list to `name`.
    pub fn save_list(&self, name: &str, items: &[String]) -> io::Result<()> {
        atomic_write(&self.dir.join(name), &json::write_array(items))
    }

    /// The registered project paths.
    pub fn projects(&self) -> Vec<String> {
        self.load_list(PROJECTS)
    }

    /// Register a project path (idempotent; kept sorted).
    pub fn add_project(&self, path: &str) -> io::Result<()> {
        let mut list = self.projects();
        if list.iter().any(|p| p == path) {
            return Ok(());
        }
        list.push(path.to_string());
        list.sort();
        self.save_list(PROJECTS, &list)
    }

    /// Forget a project path.
    pub fn remove_project(&self, path: &str) -> io::Result<()> {
        let mut list = self.projects();
        let before = list.len();
        list.retain(|p| p != path);
        if list.len() == before {
            return Ok(());
        }
        self.save_list(PROJECTS, &list)
    }

    /// Friendly names users have given to sessions, keyed by session id.
    pub fn session_names(&self) -> BTreeMap<String, String> {
        self.load_map(SESSION_NAMES)
    }

    /// Give a session a friendly name.
    pub fn set_session_name(&self, id: &str, name: &str) -> io::Result<()> {
        let mut map = self.session_names();
        map.insert(id.to_string(), name.to_string());
        self.save_map(SESSION_NAMES, &map)
    }

    /// The persisted app state (active agents, theme, …) as a flat map.
    pub fn app_state(&self) -> BTreeMap<String, String> {
        self.load_map(STATE)
    }

    /// Set one app-state value.
    pub fn set_state(&self, key: &str, value: &str) -> io::Result<()> {
        let mut map = self.app_state();
        map.insert(key.to_string(), value.to_string());
        self.save_map(STATE, &map)
    }
}

const SESSION_NAMES: &str = "session-names.json";
const STATE: &str = "state.json";
const PROJECTS: &str = "projects.json";

/// Write `contents` to `path` atomically (write a sibling temp file, then rename).
fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "loope-store-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn map_round_trips_with_special_characters() {
        let store = Store::at(temp_dir("rt")).unwrap();
        let mut map = BTreeMap::new();
        map.insert("plain".to_string(), "value".to_string());
        map.insert("tricky".to_string(), "has \"quotes\"\nand\ttabs".to_string());
        map.insert("unicode".to_string(), "café ∞".to_string());
        store.save_map("data.json", &map).unwrap();

        let loaded = store.load_map("data.json");
        assert_eq!(loaded, map);
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn missing_file_is_an_empty_map() {
        let store = Store::at(temp_dir("missing")).unwrap();
        assert!(store.load_map("nope.json").is_empty());
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn session_names_persist_across_handles() {
        let dir = temp_dir("sessions");
        {
            let store = Store::at(&dir).unwrap();
            store.set_session_name("0007-add-auth", "the auth run").unwrap();
            store.set_session_name("0008-fix-bug", "bug fix").unwrap();
        }
        let reopened = Store::at(&dir).unwrap();
        let names = reopened.session_names();
        assert_eq!(names.get("0007-add-auth").map(String::as_str), Some("the auth run"));
        assert_eq!(names.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projects_register_dedupe_and_remove() {
        let store = Store::at(temp_dir("projects")).unwrap();
        store.add_project("/a/one").unwrap();
        store.add_project("/a/two").unwrap();
        store.add_project("/a/one").unwrap(); // idempotent
        assert_eq!(store.projects(), vec!["/a/one".to_string(), "/a/two".to_string()]);
        store.remove_project("/a/one").unwrap();
        assert_eq!(store.projects(), vec!["/a/two".to_string()]);
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn app_state_sets_and_reads_back() {
        let store = Store::at(temp_dir("state")).unwrap();
        store.set_state("theme", "dark").unwrap();
        store.set_state("implementer", "claude").unwrap();
        let state = store.app_state();
        assert_eq!(state.get("theme").map(String::as_str), Some("dark"));
        assert_eq!(state.get("implementer").map(String::as_str), Some("claude"));
        let _ = std::fs::remove_dir_all(store.dir());
    }
}
