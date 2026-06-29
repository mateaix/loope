//! A project is a source directory Loope has run against — i.e. one that holds a
//! `.loope/runs/` directory. The hub discovers projects from the registered list plus any
//! extra roots, and aggregates each one's runs into [`Session`]s.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::session::{Session, load_session};
use super::store::Store;

/// A project and its runs (newest first).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub sessions: Vec<Session>,
}

impl Project {
    /// How many runs the project has.
    pub fn run_count(&self) -> usize {
        self.sessions.len()
    }

    /// The most recent run's modification time, if any.
    pub fn last_active(&self) -> Option<SystemTime> {
        self.sessions.iter().filter_map(|s| s.modified).max()
    }
}

/// The runs directory for a project path.
pub fn runs_dir(project_path: &Path) -> PathBuf {
    project_path.join(".loope").join("runs")
}

/// Discover one project at `path`, or `None` if it has no `.loope/runs/` directory.
/// Friendly session names are read from `store`.
pub fn discover_project(path: &Path, store: &Store) -> Option<Project> {
    let runs = runs_dir(path);
    let read = std::fs::read_dir(&runs).ok()?;
    let names = store.session_names();

    let mut sessions: Vec<Session> = read
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| load_session(&e.path()))
        .map(|mut s| {
            s.name = names.get(&s.id).cloned();
            s
        })
        .collect();
    // Newest first: ids are zero-padded `NNNN-slug`, so a reverse string sort works and is
    // stable when two runs share a modification time.
    sessions.sort_by(|a, b| b.id.cmp(&a.id));

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    Some(Project {
        path: path.to_path_buf(),
        name,
        sessions,
    })
}

/// Discover all known projects: the registered list in `store` plus `extra_roots`,
/// de-duplicated by path and sorted by recency (most recently active first).
pub fn discover(store: &Store, extra_roots: &[PathBuf]) -> Vec<Project> {
    let mut seen = BTreeSet::new();
    let mut projects = Vec::new();

    let roots = store
        .projects()
        .into_iter()
        .map(PathBuf::from)
        .chain(extra_roots.iter().cloned());

    for root in roots {
        let key = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        if !seen.insert(key) {
            continue;
        }
        if let Some(project) = discover_project(&root, store) {
            projects.push(project);
        }
    }

    projects.sort_by_key(|p| std::cmp::Reverse(p.last_active()));
    projects
}

#[cfg(test)]
mod tests {
    use super::super::session::tests::{temp_dir, write_run};
    use super::*;

    fn run_json(id: &str, converged: bool) -> String {
        format!(
            "{{\"run_id\":\"{id}\",\"requirement\":\"do {id}\",\"converged\":{converged},\"highlight\":false,\"iterations\":1,\"stop_reason\":\"converged\",\"steps\":[]}}"
        )
    }

    #[test]
    fn discovers_project_runs_newest_first() {
        let project = temp_dir("proj");
        let runs = runs_dir(&project);
        std::fs::create_dir_all(&runs).unwrap();
        write_run(&runs, "0001-first", &run_json("0001-first", true));
        write_run(&runs, "0002-second", &run_json("0002-second", false));
        std::fs::create_dir_all(runs.join("not-a-run")).unwrap(); // no run.json → skipped

        let store = Store::at(temp_dir("store")).unwrap();
        store.set_session_name("0002-second", "the latest").unwrap();

        let p = discover_project(&project, &store).unwrap();
        assert_eq!(p.run_count(), 2);
        assert_eq!(p.sessions[0].id, "0002-second");
        assert_eq!(p.sessions[0].name.as_deref(), Some("the latest"));
        assert_eq!(p.sessions[0].label(), "the latest");
        assert_eq!(p.sessions[1].id, "0001-first");

        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn no_runs_dir_is_none() {
        let project = temp_dir("bare");
        std::fs::create_dir_all(&project).unwrap();
        let store = Store::at(temp_dir("store2")).unwrap();
        assert!(discover_project(&project, &store).is_none());
        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn discover_merges_registered_and_extra_roots() {
        let a = temp_dir("a");
        let b = temp_dir("b");
        for (p, id) in [(&a, "0001-a"), (&b, "0001-b")] {
            let runs = runs_dir(p);
            std::fs::create_dir_all(&runs).unwrap();
            write_run(&runs, id, &run_json(id, true));
        }
        let store = Store::at(temp_dir("store3")).unwrap();
        store.add_project(&a.to_string_lossy()).unwrap();

        // `a` comes from the store, `b` from the extra roots; `a` passed twice de-dupes.
        let projects = discover(&store, &[a.clone(), b.clone()]);
        assert_eq!(projects.len(), 2);
        let names: BTreeSet<_> = projects.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains(&a.file_name().unwrap().to_string_lossy().into_owned()));
        assert!(names.contains(&b.file_name().unwrap().to_string_lossy().into_owned()));

        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        let _ = std::fs::remove_dir_all(store.dir());
    }
}
