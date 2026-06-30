//! Minimal git integration, driven by shelling out to the `git` binary (keeps `deps = 1`,
//! like the agent CLIs).
//!
//! v1 scope: keep loope's run artifacts out of version control. Later tasks add worktree
//! branches so a run's results land as a first-class git object.

use std::path::Path;
use std::process::Command;

/// Whether `dir` is inside a git work tree.
pub fn is_repo(dir: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success() && o.stdout.starts_with(b"true"))
        .unwrap_or(false)
}

/// Ensure loope's artifact directory is git-ignored, so neither the run artifacts nor a
/// copied/worktree workspace ever show up as unversioned files. Writes
/// `<loope_dir>/.gitignore` containing `*` (ignore the whole directory's contents, including
/// itself) when it is absent — leaving the user's root `.gitignore` untouched. No-op when the
/// file already exists, so a user's customization is preserved.
pub fn ensure_loope_ignored(loope_dir: &Path) -> std::io::Result<()> {
    let gitignore = loope_dir.join(".gitignore");
    if gitignore.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(loope_dir)?;
    std::fs::write(gitignore, "*\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_loope_ignored_writes_star_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("loope-gi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        ensure_loope_ignored(&dir).unwrap();
        let gi = dir.join(".gitignore");
        assert_eq!(std::fs::read_to_string(&gi).unwrap(), "*\n");
        // An existing .gitignore is left untouched (a user's customization survives).
        std::fs::write(&gi, "custom\n").unwrap();
        ensure_loope_ignored(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&gi).unwrap(), "custom\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_repo_false_outside_git() {
        let dir = std::env::temp_dir().join(format!("loope-norepo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(!is_repo(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
