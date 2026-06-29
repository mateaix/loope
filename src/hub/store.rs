//! The local hub metadata store under `~/.loope/`.
//!
//! This holds Loope's own cross-project metadata — app state and friendly session names
//! today, presets and project lists later — kept separate from each project's
//! `.loope/runs/` artifacts. Files are small JSON objects of string→string, written
//! atomically. The JSON is hand-rolled (no serde) to keep the crate std-only.

use std::collections::BTreeMap;
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::Chars;

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
            .map(|s| parse_string_map(&s))
            .unwrap_or_default()
    }

    /// Atomically write a string→string map to `name`.
    pub fn save_map(&self, name: &str, map: &BTreeMap<String, String>) -> io::Result<()> {
        atomic_write(&self.dir.join(name), &write_string_map(map))
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

/// Write `contents` to `path` atomically (write a sibling temp file, then rename).
fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Serialize a string→string map as a JSON object.
fn write_string_map(map: &BTreeMap<String, String>) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&esc(key));
        out.push_str("\":\"");
        out.push_str(&esc(value));
        out.push('"');
    }
    out.push('}');
    out
}

/// Parse a flat JSON object of `"key":"value"` pairs (lenient — ignores anything else).
fn parse_string_map(input: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut chars = input.chars().peekable();
    // Advance past the opening brace.
    for c in chars.by_ref() {
        if c == '{' {
            break;
        }
    }
    loop {
        skip_ws(&mut chars);
        match chars.peek() {
            Some('"') => {
                let key = read_string(&mut chars);
                skip_ws(&mut chars);
                if chars.peek() == Some(&':') {
                    chars.next();
                }
                skip_ws(&mut chars);
                let value = if chars.peek() == Some(&'"') {
                    read_string(&mut chars)
                } else {
                    None
                };
                if let (Some(k), Some(v)) = (key, value) {
                    map.insert(k, v);
                }
            }
            Some(',') => {
                chars.next();
            }
            Some('}') | None => break,
            _ => {
                chars.next();
            }
        }
    }
    map
}

/// Read a JSON string starting at the opening quote, reversing [`esc`].
fn read_string(chars: &mut Peekable<Chars>) -> Option<String> {
    if chars.next() != Some('"') {
        return None;
    }
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn skip_ws(chars: &mut Peekable<Chars>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

/// Minimal JSON string escaping.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
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
