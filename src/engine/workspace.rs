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
    /// Run id, e.g. `0007-add-jwt-auth`.
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
    pub fn create(
        base: &Path,
        source: &Path,
        in_place: bool,
        requirement: &str,
    ) -> io::Result<Self> {
        fs::create_dir_all(base)?;
        // Keep loope's artifacts out of version control (the run dir, and any copied or
        // worktree workspace) so they never show up as unversioned files in the user's repo.
        if let Some(loope_dir) = base.parent()
            && crate::engine::git::is_repo(source)
        {
            let _ = crate::engine::git::ensure_loope_ignored(loope_dir);
        }
        let run_id = next_run_id(base, requirement)?;
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

    /// Directory holding one step's agent files:
    /// `<run>/agents/<NN>-<role>-<adapter>/`. Keyed by the step id so the implement and
    /// revise turns (and every step) keep separate records.
    pub fn agent_dir(&self, step_id: usize, role: Role, adapter: Adapter) -> PathBuf {
        self.root.join("agents").join(format!(
            "{:02}-{}-{}",
            step_id,
            sanitize_component(role.as_str()),
            sanitize_component(adapter.as_str())
        ))
    }

    /// Create and return one step's private home directory.
    pub fn agent_home(&self, step_id: usize, role: Role, adapter: Adapter) -> io::Result<PathBuf> {
        let home = self.agent_dir(step_id, role, adapter).join("home");
        fs::create_dir_all(&home)?;
        Ok(home)
    }
}

/// Allocate the next run id: a zero-padded sequence number plus a slug of the
/// requirement, e.g. `0008-add-jwt-auth`. The number keeps runs ordered and uniquely
/// referenceable; the slug makes the directory self-describing.
pub fn next_run_id(base: &Path, requirement: &str) -> io::Result<String> {
    let mut max = 0usize;
    if base.exists() {
        for entry in fs::read_dir(base)? {
            if let Some(n) = run_number(&entry?.file_name().to_string_lossy()) {
                max = max.max(n);
            }
        }
    }
    let n = max + 1;
    let slug = slugify(requirement);
    Ok(if slug.is_empty() {
        format!("{n:04}")
    } else {
        format!("{n:04}-{slug}")
    })
}

/// The leading sequence number of a run directory name, tolerating the legacy `run-NNNN`
/// scheme as well as the current `NNNN-slug`.
fn run_number(name: &str) -> Option<usize> {
    let digits: String = name
        .strip_prefix("run-")
        .unwrap_or(name)
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Turn a requirement into a short, filesystem-safe slug: lowercase ASCII words joined by
/// `-`, capped at six words / 40 chars. Returns `""` when nothing usable remains.
pub fn slugify(requirement: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    for c in requirement.chars() {
        if c.is_ascii_alphanumeric() {
            word.extend(c.to_lowercase());
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut slug = words.into_iter().take(6).collect::<Vec<_>>().join("-");
    slug.truncate(40);
    slug.trim_matches('-').to_string()
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

/// Largest file (bytes) whose text content is captured for diffing.
const MAX_DIFF_FILE_BYTES: u64 = 512 * 1024;

/// A file's change summary for the report and run record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    /// Changed but not diffable as text (binary or too large).
    pub binary: bool,
}

/// A file change plus its unified diff text (persisted to `changes.diff`).
#[derive(Clone, Debug)]
pub struct FileDiff {
    pub change: FileChange,
    pub unified: String,
}

/// Capture the UTF-8 text content of every (small enough) file under `dir`, keyed by
/// relative path. Binary and oversized files are omitted.
pub fn content_snapshot(dir: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let _ = collect_content(dir, dir, &mut map);
    map
}

fn collect_content(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> io::Result<()> {
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
            collect_content(base, &entry.path(), out)?;
        } else if file_type.is_file() {
            if SKIP_FILES.contains(&name.as_ref()) {
                continue;
            }
            let path = entry.path();
            if entry.metadata()?.len() > MAX_DIFF_FILE_BYTES {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                out.insert(rel, text);
            }
        }
    }
    Ok(())
}

/// Build a [`FileDiff`] for every changed path, using the captured before-content and
/// the file's current (after) content.
pub fn compute_file_diffs(
    workspace_dir: &Path,
    changed: &[String],
    before_content: &BTreeMap<String, String>,
) -> Vec<FileDiff> {
    let mut diffs = Vec::new();
    for path in changed {
        let before = before_content.get(path).map(|s| s.as_str());
        let after = fs::read_to_string(workspace_dir.join(path)).ok();
        let diff = match (before, after.as_deref()) {
            (Some(b), Some(a)) => {
                let (added, removed, unified) = diff_lines(b, a);
                FileDiff {
                    change: FileChange {
                        path: path.clone(),
                        added,
                        removed,
                        binary: false,
                    },
                    unified,
                }
            }
            (None, Some(a)) => {
                let count = a.lines().count();
                let mut unified = format!("@@ -0,0 +1,{count} @@\n");
                for line in a.lines() {
                    unified.push_str(&format!("+{line}\n"));
                }
                FileDiff {
                    change: FileChange {
                        path: path.clone(),
                        added: count,
                        removed: 0,
                        binary: false,
                    },
                    unified,
                }
            }
            (Some(b), None) => {
                let count = b.lines().count();
                let mut unified = format!("@@ -1,{count} +0,0 @@\n");
                for line in b.lines() {
                    unified.push_str(&format!("-{line}\n"));
                }
                FileDiff {
                    change: FileChange {
                        path: path.clone(),
                        added: 0,
                        removed: count,
                        binary: false,
                    },
                    unified,
                }
            }
            (None, None) => FileDiff {
                change: FileChange {
                    path: path.clone(),
                    added: 0,
                    removed: 0,
                    binary: true,
                },
                unified: String::new(),
            },
        };
        diffs.push(diff);
    }
    diffs
}

/// Render a set of diffs as a single `changes.diff` document.
pub fn render_diffs(diffs: &[FileDiff]) -> String {
    let mut out = String::new();
    for diff in diffs {
        out.push_str(&format!("diff a/{0} b/{0}\n", diff.change.path));
        if diff.change.binary {
            out.push_str("Binary file changed\n\n");
        } else {
            out.push_str(&format!("--- a/{0}\n+++ b/{0}\n", diff.change.path));
            out.push_str(&diff.unified);
            if !diff.unified.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
    }
    out
}

/// One line of a diff: tag (' ' context, '-' removed, '+' added) and 1-based line
/// numbers in the old and new files.
struct DiffOp {
    tag: char,
    text: String,
    old_no: usize,
    new_no: usize,
}

/// Lines of unchanged context kept around each change in a hunk.
const DIFF_CONTEXT: usize = 3;
/// Max body lines emitted per file before collapsing the tail.
const DIFF_MAX_LINES: usize = 400;

/// Line-based diff returning (added, removed, unified-with-hunks). Uses an LCS for
/// normal files and a cheap multiset count for very large ones.
pub fn diff_lines(before: &str, after: &str) -> (usize, usize, String) {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();

    if (a.len() + 1).saturating_mul(b.len() + 1) > 4_000_000 {
        return cheap_diff(&a, &b);
    }

    let ops = lcs_ops(&a, &b);
    let added = ops.iter().filter(|o| o.tag == '+').count();
    let removed = ops.iter().filter(|o| o.tag == '-').count();
    (added, removed, format_hunks(&ops))
}

/// Backtrack an LCS into an ordered list of context/removed/added lines.
fn lcs_ops(a: &[&str], b: &[&str]) -> Vec<DiffOp> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(DiffOp { tag: ' ', text: a[i].to_string(), old_no: i + 1, new_no: j + 1 });
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            ops.push(DiffOp { tag: '-', text: a[i].to_string(), old_no: i + 1, new_no: 0 });
            i += 1;
        } else {
            ops.push(DiffOp { tag: '+', text: b[j].to_string(), old_no: 0, new_no: j + 1 });
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOp { tag: '-', text: a[i].to_string(), old_no: i + 1, new_no: 0 });
        i += 1;
    }
    while j < m {
        ops.push(DiffOp { tag: '+', text: b[j].to_string(), old_no: 0, new_no: j + 1 });
        j += 1;
    }
    ops
}

/// Group ops into `@@`-delimited hunks with surrounding context, collapsing the tail
/// if the diff is very large.
fn format_hunks(ops: &[DiffOp]) -> String {
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| o.tag != ' ')
        .map(|(i, _)| i)
        .collect();
    if changed.is_empty() {
        return String::new();
    }

    // Build the inclusive [start, end] op ranges each hunk covers, merging overlaps.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &c in &changed {
        let lo = c.saturating_sub(DIFF_CONTEXT);
        let hi = (c + DIFF_CONTEXT).min(ops.len() - 1);
        match ranges.last_mut() {
            Some(last) if lo <= last.1 + 1 => last.1 = last.1.max(hi),
            _ => ranges.push((lo, hi)),
        }
    }

    let mut out = String::new();
    let mut emitted = 0usize;
    for (lo, hi) in ranges {
        let slice = &ops[lo..=hi];
        let old_start = slice.iter().find(|o| o.old_no != 0).map(|o| o.old_no).unwrap_or(0);
        let new_start = slice.iter().find(|o| o.new_no != 0).map(|o| o.new_no).unwrap_or(0);
        let old_count = slice.iter().filter(|o| o.tag != '+').count();
        let new_count = slice.iter().filter(|o| o.tag != '-').count();
        out.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));
        for op in slice {
            if emitted >= DIFF_MAX_LINES {
                let remaining = ops.len() - lo;
                out.push_str(&format!("… +{remaining} more lines\n"));
                return out;
            }
            out.push_str(&format!("{}{}\n", op.tag, op.text));
            emitted += 1;
        }
    }
    out
}

/// Count-only diff for files too large for the LCS table.
fn cheap_diff(a: &[&str], b: &[&str]) -> (usize, usize, String) {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for line in a {
        *counts.entry(line).or_default() += 1;
    }
    for line in b {
        *counts.entry(line).or_default() -= 1;
    }
    let removed = counts.values().filter(|v| **v > 0).map(|v| *v as usize).sum();
    let added = counts
        .values()
        .filter(|v| **v < 0)
        .map(|v| (-*v) as usize)
        .sum();
    (added, removed, "(diff too large to display)\n".to_string())
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
    fn run_ids_increment_with_slug() {
        let base = temp_base("ids");
        assert_eq!(next_run_id(&base, "Add JWT auth").unwrap(), "0001-add-jwt-auth");
        // The number advances past existing runs (current and legacy schemes alike).
        fs::create_dir_all(base.join("0001-add-jwt-auth")).unwrap();
        fs::create_dir_all(base.join("run-0007")).unwrap();
        assert_eq!(next_run_id(&base, "Fix bug").unwrap(), "0008-fix-bug");
        // A requirement with nothing slug-able falls back to just the number.
        assert_eq!(next_run_id(&base, "🎉🎉").unwrap(), "0008");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn slugify_is_short_and_safe() {
        assert_eq!(slugify("Add a multiply(a, b) function!"), "add-a-multiply-a-b-function");
        assert_eq!(slugify("  trailing -- dashes  "), "trailing-dashes");
        assert_eq!(slugify(""), "");
        assert!(!slugify("a/../b").contains('/'));
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

        let ws = RunWorkspace::create(&base.join("runs"), &source, false, "demo run").unwrap();
        assert_eq!(ws.run_id, "0001-demo-run");
        // copied the real file but skipped target/, .claude/, and .DS_Store
        assert!(ws.workspace_dir.join("a.txt").exists());
        assert!(!ws.workspace_dir.join("target").exists());
        assert!(!ws.workspace_dir.join(".claude").exists());
        assert!(!ws.workspace_dir.join(".DS_Store").exists());

        let h1 = ws.agent_home(1, Role::Implementer, Adapter::Claude).unwrap();
        let h2 = ws.agent_home(2, Role::Reviewer, Adapter::Codex).unwrap();
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
    fn diff_counts_modifications() {
        let before = "a\nb\nc\n";
        let after = "a\nB\nc\nd\n";
        let (added, removed, unified) = diff_lines(before, after);
        assert_eq!(removed, 1); // b
        assert_eq!(added, 2); // B and d
        assert!(unified.contains("@@")); // hunk header
        assert!(unified.contains("-b"));
        assert!(unified.contains("+B"));
        assert!(unified.contains("+d"));
        assert!(unified.contains(" a")); // context kept
    }

    #[test]
    fn diff_added_and_deleted_files() {
        let (added, removed, _) = diff_lines("", "x\ny\n");
        assert_eq!((added, removed), (2, 0));
        let (added, removed, _) = diff_lines("x\ny\n", "");
        assert_eq!((added, removed), (0, 2));
    }

    #[test]
    fn compute_file_diffs_reports_per_file_stats() {
        let base = temp_base("diffs");
        // before: a.txt = "1\n2\n"; new.txt absent
        let before = {
            let mut m = std::collections::BTreeMap::new();
            m.insert("a.txt".to_string(), "1\n2\n".to_string());
            m
        };
        // after on disk: a.txt = "1\n2\n3\n"; new.txt = "hi\n"
        fs::write(base.join("a.txt"), "1\n2\n3\n").unwrap();
        fs::write(base.join("new.txt"), "hi\n").unwrap();

        let diffs = compute_file_diffs(
            &base,
            &["a.txt".to_string(), "new.txt".to_string()],
            &before,
        );
        let a = diffs.iter().find(|d| d.change.path == "a.txt").unwrap();
        assert_eq!((a.change.added, a.change.removed), (1, 0));
        let new = diffs.iter().find(|d| d.change.path == "new.txt").unwrap();
        assert_eq!((new.change.added, new.change.removed), (1, 0));
        assert!(render_diffs(&diffs).contains("+3"));

        let _ = fs::remove_dir_all(&base);
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
