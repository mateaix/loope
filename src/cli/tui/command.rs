//! The slash-command vocabulary for the home prompt. Parsing is decoupled from the app:
//! [`parse`] turns a typed line into a [`Command`]; [`matches`] powers the palette.

use loope::Adapter;

/// A parsed slash command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Iters(usize),
    Preset(String),
    Implementer(Adapter),
    Reviewers(Vec<Adapter>),
    /// `Some(cmd)` sets the verifier command; `None` clears it.
    Verify(Option<String>),
    ToggleDesign,
    ToggleDry,
    Apply,
    Browse,
    Doctor,
    Help,
    Quit,
}

/// A command's name, argument hint, and one-line help, for the palette.
pub struct Spec {
    pub name: &'static str,
    pub args: &'static str,
    pub help: &'static str,
}

pub const SPECS: &[Spec] = &[
    Spec { name: "iters", args: "N", help: "set the iteration cap" },
    Spec { name: "preset", args: "NAME", help: "claude-codex | dual-review | …" },
    Spec { name: "implementer", args: "A", help: "set the implementer adapter" },
    Spec { name: "reviewers", args: "A[,B]", help: "set the reviewer adapter(s)" },
    Spec { name: "verify", args: "[CMD]", help: "verifier command (empty clears)" },
    Spec { name: "design", args: "", help: "toggle the design step" },
    Spec { name: "dry", args: "", help: "toggle stub agents" },
    Spec { name: "apply", args: "", help: "apply the selected run's changes" },
    Spec { name: "browse", args: "", help: "open the run browser" },
    Spec { name: "doctor", args: "", help: "re-check the local agent CLIs" },
    Spec { name: "help", args: "", help: "show keys & commands" },
    Spec { name: "quit", args: "", help: "quit" },
];

/// The commands whose name starts with the first token of `input` (which begins `/`).
pub fn matches(input: &str) -> Vec<&'static Spec> {
    let token = input
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or("");
    SPECS.iter().filter(|s| s.name.starts_with(token)).collect()
}

/// Parse a typed line (with or without the leading `/`) into a [`Command`].
pub fn parse(line: &str) -> Result<Command, String> {
    let line = line.trim_start_matches('/').trim();
    let (name, rest) = match line.split_once(char::is_whitespace) {
        Some((n, r)) => (n, r.trim()),
        None => (line, ""),
    };
    match name.to_ascii_lowercase().as_str() {
        "iters" | "max-iters" => {
            let n: usize = rest.parse().map_err(|_| "usage: /iters N".to_string())?;
            if n == 0 {
                return Err("iterations must be ≥ 1".to_string());
            }
            Ok(Command::Iters(n))
        }
        "preset" => {
            if rest.is_empty() {
                Err("usage: /preset NAME".to_string())
            } else {
                Ok(Command::Preset(rest.to_ascii_lowercase()))
            }
        }
        "implementer" => Ok(Command::Implementer(adapter(rest)?)),
        "reviewers" | "reviewer" => {
            let mut adapters = Vec::new();
            for token in rest.split(',') {
                let token = token.trim();
                if !token.is_empty() {
                    adapters.push(adapter(token)?);
                }
            }
            if adapters.is_empty() {
                return Err("usage: /reviewers A[,B]".to_string());
            }
            Ok(Command::Reviewers(adapters))
        }
        "verify" => Ok(Command::Verify((!rest.is_empty()).then(|| rest.to_string()))),
        "design" => Ok(Command::ToggleDesign),
        "dry" | "dry-run" => Ok(Command::ToggleDry),
        "apply" => Ok(Command::Apply),
        "browse" | "runs" => Ok(Command::Browse),
        "doctor" | "check" => Ok(Command::Doctor),
        "help" => Ok(Command::Help),
        "quit" | "exit" => Ok(Command::Quit),
        other => Err(format!("unknown command: /{other}")),
    }
}

fn adapter(name: &str) -> Result<Adapter, String> {
    Adapter::parse(name).ok_or_else(|| format!("unknown adapter: {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_settings_commands() {
        assert_eq!(parse("/iters 5"), Ok(Command::Iters(5)));
        assert_eq!(parse("/max-iters 2"), Ok(Command::Iters(2)));
        assert_eq!(parse("/implementer codex"), Ok(Command::Implementer(Adapter::Codex)));
        assert_eq!(
            parse("/reviewers codex,claude"),
            Ok(Command::Reviewers(vec![Adapter::Codex, Adapter::Claude]))
        );
        assert_eq!(
            parse("/verify cargo test --all"),
            Ok(Command::Verify(Some("cargo test --all".to_string())))
        );
        assert_eq!(parse("/verify"), Ok(Command::Verify(None)));
        assert_eq!(parse("/design"), Ok(Command::ToggleDesign));
        assert_eq!(parse("/dry"), Ok(Command::ToggleDry));
    }

    #[test]
    fn parses_tool_commands() {
        assert_eq!(parse("/apply"), Ok(Command::Apply));
        assert_eq!(parse("/browse"), Ok(Command::Browse));
        assert_eq!(parse("/quit"), Ok(Command::Quit));
    }

    #[test]
    fn reports_errors() {
        assert!(parse("/iters").is_err());
        assert!(parse("/iters 0").is_err());
        assert!(parse("/implementer nope").is_err());
        assert!(parse("/bogus").is_err());
    }

    #[test]
    fn palette_filters_by_prefix() {
        assert!(matches("/").len() >= 10);
        let i = matches("/i");
        assert!(i.iter().all(|s| s.name.starts_with('i')));
        assert!(i.iter().any(|s| s.name == "iters"));
        assert_eq!(matches("/verify").iter().filter(|s| s.name == "verify").count(), 1);
    }
}
