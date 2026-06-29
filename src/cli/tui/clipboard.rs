//! Read the system clipboard for Ctrl+V paste — an image when one is present, else text.
//!
//! macOS only for now (via `osascript` / `pbpaste`); other platforms return `None`, so
//! Ctrl+V is simply a no-op there.

use std::path::PathBuf;
use std::process::Command;

/// Grab an image from the clipboard, saving it to a temp PNG and returning its path.
/// Returns `None` when the clipboard holds no image.
pub fn grab_image() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "loope-paste-{}-{}.png",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        // Write the clipboard's PNG representation to the file, or do nothing if there
        // isn't one.
        let script = format!(
            "try\n\
               set f to (open for access (POSIX file \"{}\") with write permission)\n\
               write (the clipboard as «class PNGf») to f\n\
               close access f\n\
               return \"ok\"\n\
             on error\n\
               try\n close access f\n end try\n\
               return \"none\"\n\
             end try",
            path.display()
        );
        let output = Command::new("osascript").arg("-e").arg(&script).output().ok()?;
        let ok = String::from_utf8_lossy(&output.stdout).trim() == "ok";
        if ok && path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return Some(path);
        }
        let _ = std::fs::remove_file(&path);
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Grab plain text from the clipboard.
pub fn grab_text() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pbpaste").output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        (!text.is_empty()).then_some(text)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
