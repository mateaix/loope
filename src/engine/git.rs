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

/// The short HEAD sha of the repo at `dir`, if resolvable (the point a worktree branches
/// from — recorded so the end-of-run summary can print `git diff <base>..<branch>`).
pub fn head_sha(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Sanitize a string into a valid git ref body (the branch suffix). Keeps `[A-Za-z0-9_/-]`,
/// turns everything else (including `.`, which git ref rules restrict) into `-`, and trims
/// separators git dislikes at the ends. Never returns empty.
pub fn sanitize_ref(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '/' => out.push(c),
            _ => out.push('-'),
        }
    }
    let trimmed = out.trim_matches(|c| c == '-' || c == '/');
    if trimmed.is_empty() {
        "run".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Add a git worktree at `path` on a new branch `branch`, based on the source's `HEAD`.
/// `path` must not yet exist (git creates it). Returns the git error text on failure.
pub fn worktree_add(source: &Path, branch: &str, path: &Path) -> std::io::Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["worktree", "add", "-b", branch])
        .arg(path)
        .arg("HEAD")
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

/// Whether the work tree at `dir` has uncommitted changes (tracked or untracked).
pub fn is_dirty(dir: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Stage everything in the worktree and commit. `--no-verify` skips the repo's pre-commit
/// hooks (loope's result commit must not be blocked by — or wait on — user hooks). Returns
/// `Ok(true)` on a new commit, `Ok(false)` when there was nothing to commit.
pub fn commit_all(worktree: &Path, message: &str) -> std::io::Result<bool> {
    let add = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["add", "-A"])
        .output()?;
    if !add.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&add.stderr).trim().to_string(),
        ));
    }
    // Is anything staged? `git diff --cached --quiet` exits 0 when there's nothing to commit,
    // 1 when there is — locale-independent, unlike parsing the commit message.
    let staged = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["diff", "--cached", "--quiet"])
        .status()?;
    if staged.success() {
        return Ok(false); // nothing to commit
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["commit", "--no-verify", "-m", message])
        .output()?;
    if out.status.success() {
        Ok(true)
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
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

    #[test]
    fn is_dirty_and_commit_all_roundtrip() {
        let dir = std::env::temp_dir().join(format!("loope-commit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success(),
                "git {args:?}"
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "1\n").unwrap();
        assert!(is_dirty(&dir)); // untracked file
        assert!(commit_all(&dir, "first").unwrap()); // makes a commit
        assert!(!is_dirty(&dir)); // clean now
        assert!(!commit_all(&dir, "noop").unwrap()); // nothing to commit → Ok(false)
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_ref_keeps_safe_chars() {
        assert_eq!(sanitize_ref("0001-review-rfc-076"), "0001-review-rfc-076");
        assert_eq!(sanitize_ref("feat/x y..z"), "feat/x-y--z");
        assert_eq!(sanitize_ref("--/--"), "run");
        assert_eq!(sanitize_ref("ünïcode"), "n-code");
    }
}
