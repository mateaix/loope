//! Tiny std-only JSON helpers shared across the hub (no serde).
//!
//! Just enough to read the run artifacts Loope writes and to persist the hub's own flat
//! metadata: string→string objects, string arrays, and single-field extraction from a
//! one-line JSON object.

use std::collections::BTreeMap;
use std::iter::Peekable;
use std::str::Chars;

/// Serialize a string→string map as a JSON object.
pub fn write_object(map: &BTreeMap<String, String>) -> String {
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
pub fn parse_object(input: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut chars = input.chars().peekable();
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

/// Serialize a list of strings as a JSON array.
pub fn write_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&esc(item));
        out.push('"');
    }
    out.push(']');
    out
}

/// Parse a JSON array of strings (lenient).
pub fn parse_array(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = input.chars().peekable();
    for c in chars.by_ref() {
        if c == '[' {
            break;
        }
    }
    loop {
        skip_ws(&mut chars);
        match chars.peek() {
            Some('"') => {
                if let Some(s) = read_string(&mut chars) {
                    out.push(s);
                }
            }
            Some(',') => {
                chars.next();
            }
            Some(']') | None => break,
            _ => {
                chars.next();
            }
        }
    }
    out
}

/// Extract a `"key":"value"` string field, reversing [`esc`].
pub fn field_str(input: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = input.find(&needle)? + needle.len();
    let mut chars = input[start..].chars().peekable();
    // `read_string` expects the opening quote; we are already past it, so feed it back.
    let mut buf = String::from('"');
    buf.extend(chars.by_ref());
    read_string(&mut buf.chars().peekable())
}

/// Extract a `"key":<number>` unsigned field.
pub fn field_u64(input: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let start = input.find(&needle)? + needle.len();
    let rest = input[start..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract a `"key":true|false` boolean field.
pub fn field_bool(input: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\":");
    let start = input.find(&needle)? + needle.len();
    let rest = input[start..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
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
pub fn esc(s: &str) -> String {
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

    #[test]
    fn object_round_trips() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), "1".to_string());
        map.insert("b".to_string(), "has \"quotes\"\nand tabs\t".to_string());
        assert_eq!(parse_object(&write_object(&map)), map);
    }

    #[test]
    fn array_round_trips() {
        let items = vec!["/a/b".to_string(), "with \"q\"".to_string(), "café".to_string()];
        assert_eq!(parse_array(&write_array(&items)), items);
    }

    #[test]
    fn fields_extract() {
        let line = "{\"run_id\":\"0008-x\",\"requirement\":\"add \\\"auth\\\"\",\"converged\":true,\"iterations\":3}";
        assert_eq!(field_str(line, "run_id").as_deref(), Some("0008-x"));
        assert_eq!(field_str(line, "requirement").as_deref(), Some("add \"auth\""));
        assert_eq!(field_bool(line, "converged"), Some(true));
        assert_eq!(field_u64(line, "iterations"), Some(3));
        assert_eq!(field_str(line, "missing"), None);
    }
}
