//! Per-run workspace management.
//!
//! A [`RunWorkspace`] owns one `.loope/runs/<run-id>/` directory. All agents in a
//! run share one `workspace/` working tree (so the reviewer sees the implementer's
//! changes), while each agent gets its own private `home/` so their session state
//! never collides. Any id used in a path is sanitized so it cannot escape the run
//! root, and all state files are written atomically.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::{Adapter, Role};

/// A lightweight fingerprint of a directory tree: relative path -> (mtime, len).
pub type FsSnapshot = BTreeMap<String, (u64, u64)>;

/// Directory names skipped when seeding a workspace from a source tree.
const SKIP_DIRS: &[&str] = &[".git", ".loope", ".claude", "target", "node_modules"];

/// File names skipped when seeding a workspace and when snapshotting for change
/// detection (editor/OS cruft that an agent should never see or "change").
const SKIP_FILES: &[&str] = &[".DS_Store"];

/// One run's directory tree.
#[derive(Clone, Debug)]
pub struct RunWorkspace {
    /// Run id, e.g. `run-0001`.
    pub run_id: String,
    /// Run root: `<base>/<run-id>`.
    pub root: PathBuf,
    /// Working tree the agents read and edit.
    pub workspace_dir: PathBuf,
    /// Whether the working tree is the caller's real directory (true) or a copy.
    pub in_place: bool,
}

impl RunWorkspace {
    /// Create the next run directory under `base`, seeding the working tree from
    /// `source`. When `in_place` is true the working tree IS `source` (no copy);
    /// otherwise `source` is copied into `<run>/workspace/`.
    pub fn create(base: &Path, source: &Path, in_place: bool) -> io::Result<Self> {
        fs::create_dir_all(base)?;
        let run_id = next_run_id(base)?;
        let root = base.join(&run_id);
        fs::create_dir_all(&root)?;

        let workspace_dir = if in_place {
            source.to_path_buf()
        } else {
            let ws = root.join("workspace");
            fs::create_dir_all(&ws)?;
            copy_tree(source, &ws)?;
            ws
        };
        fs::create_dir_all(root.join("agents"))?;

        Ok(Self {
            run_id,
            root,
            workspace_dir,
            in_place,
        })
    }

    /// Directory holding one agent's files: `<run>/agents/<role>-<adapter>/`.
    pub fn agent_dir(&self, role: Role, adapter: Adapter) -> PathBuf {
        self.root.join("agents").join(format!(
            "{}-{}",
            sanitize_component(role.as_str()),
            sanitize_component(adapter.as_str())
        ))
    }

    /// Create and return one agent's private home directory.
    pub fn agent_home(&self, role: Role, adapter: Adapter) -> io::Result<PathBuf> {
        let home = self.agent_dir(role, adapter).join("home");
        fs::create_dir_all(&home)?;
        Ok(home)
    }
}

/// Allocate the next `run-NNNN` id by scanning existing run directories.
pub fn next_run_id(base: &Path) -> io::Result<String> {
    let mut max = 0usize;
    if base.exists() {
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(suffix) = name.strip_prefix("run-")
                && let Ok(n) = suffix.parse::<usize>()
            {
                max = max.max(n);
            }
        }
    }
    Ok(format!("run-{:04}", max + 1))
}

/// Reduce an id to a safe single path component: ASCII alphanumerics, `-`, and `_`
/// survive; everything else (including `/`, `.`, and the `..` traversal) becomes
/// `-`. Never returns an empty string.
pub fn sanitize_component(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "x".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Write `contents` to `path` atomically: write a sibling temp file, then rename.
pub fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("state");
    let tmp = parent.join(format!(".{file_name}.tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Fingerprint every file under `dir` (skipping the same heavy directories as the
/// copy), so changes can be detected by comparing two snapshots.
pub fn snapshot(dir: &Path) -> FsSnapshot {
    let mut map = FsSnapshot::new();
    let _ = collect_snapshot(dir, dir, &mut map);
    map
}

fn collect_snapshot(base: &Path, dir: &Path, out: &mut FsSnapshot) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_snapshot(base, &entry.path(), out)?;
        } else if file_type.is_file() {
            if SKIP_FILES.contains(&name.as_ref()) {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let meta = entry.metadata()?;
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.insert(rel, (mtime, meta.len()));
        }
    }
    Ok(())
}

/// Files that were added or modified between two snapshots, sorted.
pub fn changed_files(before: &FsSnapshot, after: &FsSnapshot) -> Vec<String> {
    let mut changed: Vec<String> = after
        .iter()
        .filter(|(path, sig)| before.get(*path) != Some(*sig))
        .map(|(path, _)| path.clone())
        .collect();
    changed.sort();
    changed
}

/// Recursively copy `src` into `dst`, skipping heavy/irrelevant directories and
/// symlinks.
fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            let target = dst.join(&file_name);
            fs::create_dir_all(&target)?;
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            if SKIP_FILES.contains(&name.as_ref()) {
                continue;
            }
            fs::copy(entry.path(), dst.join(&file_name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temp directory for one test, no external crates.
    fn temp_base(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "loope-ws-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp base");
        dir
    }

    #[test]
    fn run_ids_increment() {
        let base = temp_base("ids");
        assert_eq!(next_run_id(&base).unwrap(), "run-0001");
        fs::create_dir_all(base.join("run-0001")).unwrap();
        fs::create_dir_all(base.join("run-0007")).unwrap();
        assert_eq!(next_run_id(&base).unwrap(), "run-0008");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sanitize_blocks_traversal_and_separators() {
        for input in ["../etc", "a/b", "..", "x/../y", ""] {
            let out = sanitize_component(input);
            assert!(!out.contains('/'), "{input:?} -> {out:?}");
            assert!(!out.contains(".."), "{input:?} -> {out:?}");
            assert!(!out.is_empty());
        }
        assert_eq!(sanitize_component("implementer"), "implementer");
    }

    #[test]
    fn create_copies_tree_and_isolates_agent_homes() {
        let base = temp_base("create");
        let source = temp_base("src");
        fs::write(source.join("a.txt"), "hello").unwrap();
        fs::write(source.join(".DS_Store"), "junk").unwrap();
        fs::create_dir_all(source.join("target")).unwrap();
        fs::write(source.join("target").join("big.bin"), "skip me").unwrap();
        fs::create_dir_all(source.join(".claude")).unwrap();
        fs::write(source.join(".claude").join("settings.json"), "{}").unwrap();

        let ws = RunWorkspace::create(&base.join("runs"), &source, false).unwrap();
        assert_eq!(ws.run_id, "run-0001");
        // copied the real file but skipped target/, .claude/, and .DS_Store
        assert!(ws.workspace_dir.join("a.txt").exists());
        assert!(!ws.workspace_dir.join("target").exists());
        assert!(!ws.workspace_dir.join(".claude").exists());
        assert!(!ws.workspace_dir.join(".DS_Store").exists());

        let h1 = ws.agent_home(Role::Implementer, Adapter::Claude).unwrap();
        let h2 = ws.agent_home(Role::Reviewer, Adapter::Codex).unwrap();
        assert!(h1.exists() && h2.exists());
        assert_ne!(h1, h2);

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&source);
    }

    #[test]
    fn snapshot_detects_added_and_modified_files() {
        let dir = temp_base("snap");
        fs::write(dir.join("keep.txt"), "same").unwrap();
        fs::write(dir.join("edit.txt"), "v1").unwrap();
        let before = snapshot(&dir);

        // add a file and grow an existing one
        fs::write(dir.join("new.txt"), "fresh").unwrap();
        fs::write(dir.join("edit.txt"), "v1-extended").unwrap();
        let after = snapshot(&dir);

        let changed = changed_files(&before, &after);
        assert!(changed.contains(&"new.txt".to_string()));
        assert!(changed.contains(&"edit.txt".to_string()));
        assert!(!changed.contains(&"keep.txt".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_replaces_contents() {
        let base = temp_base("atomic");
        let target = base.join("nested").join("state.json");
        atomic_write(&target, "{\"a\":1}").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"a\":1}");
        atomic_write(&target, "{\"a\":2}").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"a\":2}");
        // no leftover temp file
        let leftover = base.join("nested").join(".state.json.tmp");
        assert!(!leftover.exists());
        let _ = fs::remove_dir_all(&base);
    }
}
