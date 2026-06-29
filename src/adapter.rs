//! Adapter execution model: how Loope launches and talks to a coding-agent CLI.
//!
//! An [`AdapterSpec`] is pure data describing one CLI (program name, env override,
//! capabilities). An [`Invoker`] runs a single [`AgentInvocation`] and returns an
//! [`InvocationResult`]. The loop engine only depends on the trait, so the real
//! subprocess invoker and the deterministic stub invoker are interchangeable.

pub mod cell;
pub mod event;
pub mod stub;
pub mod subprocess;

pub use cell::{Cell, cells_from_events, parse_hunks};
pub use event::LoopEvent;
pub use stub::StubInvoker;
pub use subprocess::SubprocessInvoker;

use std::env;
use std::path::PathBuf;

use crate::{Adapter, Role};

/// Static description of how to launch and talk to one adapter's CLI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdapterSpec {
    pub adapter: Adapter,
    /// Default binary name on `PATH` (empty when the adapter has no real CLI).
    pub default_program: &'static str,
    /// Environment variable that overrides the program path.
    pub env_override: &'static str,
    /// Whether this adapter maps to a real external CLI.
    pub has_real_cli: bool,
}

/// Look up the static spec for an adapter.
pub fn spec_for(adapter: Adapter) -> AdapterSpec {
    match adapter {
        Adapter::Claude => AdapterSpec {
            adapter,
            default_program: "claude",
            env_override: "LOOPE_CLAUDE_BIN",
            has_real_cli: true,
        },
        Adapter::Codex => AdapterSpec {
            adapter,
            default_program: "codex",
            env_override: "LOOPE_CODEX_BIN",
            has_real_cli: true,
        },
        Adapter::OpenCode => AdapterSpec {
            adapter,
            default_program: "opencode",
            env_override: "LOOPE_OPENCODE_BIN",
            has_real_cli: true,
        },
        Adapter::Generic => AdapterSpec {
            adapter,
            default_program: "",
            env_override: "LOOPE_GENERIC_BIN",
            has_real_cli: false,
        },
    }
}

/// Resolve a program name from an explicit override value, falling back to the
/// spec default. Returns `None` when neither yields a usable program. Kept pure so
/// it can be tested without touching the process environment.
pub fn resolve_program_from(spec: &AdapterSpec, override_val: Option<&str>) -> Option<String> {
    if let Some(val) = override_val {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if spec.default_program.is_empty() {
        None
    } else {
        Some(spec.default_program.to_string())
    }
}

/// Resolve a program name, reading the adapter's override environment variable.
pub fn resolve_program(spec: &AdapterSpec) -> Option<String> {
    let env_val = env::var(spec.env_override).ok();
    resolve_program_from(spec, env_val.as_deref())
}

/// The local availability of one adapter's CLI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterStatus {
    pub adapter: Adapter,
    /// The resolved program name (honoring the override env var), if any.
    pub program: Option<String>,
    /// Whether that program was found on the system.
    pub available: bool,
}

/// Self-check the real coding-agent adapters (Claude, Codex, OpenCode) for a local CLI.
pub fn check_adapters() -> Vec<AdapterStatus> {
    [Adapter::Claude, Adapter::Codex, Adapter::OpenCode]
        .into_iter()
        .map(status_for)
        .collect()
}

/// Probe one adapter: resolve its program and check whether it is installed.
pub fn status_for(adapter: Adapter) -> AdapterStatus {
    let program = resolve_program(&spec_for(adapter));
    let available = program.as_deref().is_some_and(program_exists);
    AdapterStatus {
        adapter,
        program,
        available,
    }
}

/// True if `program` is runnable: an explicit path that is an executable file, or a bare
/// name found as an executable on `PATH`.
pub fn program_exists(program: &str) -> bool {
    let candidate = std::path::Path::new(program);
    if program.contains(std::path::MAIN_SEPARATOR) {
        return is_executable(candidate);
    }
    let Ok(path) = env::var("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| is_executable(&dir.join(program)))
}

fn is_executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// A single agent turn to run: a prompt against a workspace, with a private home
/// directory and a read-only flag.
#[derive(Clone, Debug)]
pub struct AgentInvocation {
    pub adapter: Adapter,
    pub role: Role,
    pub prompt: String,
    /// The working tree the agent reads and (if not read-only) edits.
    pub workspace_dir: PathBuf,
    /// This agent's private CLI home / session directory.
    pub home_dir: PathBuf,
    /// When true, the agent must not modify the workspace.
    pub read_only: bool,
}

/// The parsed outcome of one agent invocation.
#[derive(Clone, Debug)]
pub struct InvocationResult {
    pub success: bool,
    /// Final agent message / summary.
    pub message: String,
    /// Files the agent reported changing, relative to the workspace.
    pub changed_files: Vec<String>,
    /// Raw captured output stream, persisted as the step transcript.
    pub transcript: String,
}

impl InvocationResult {
    /// Build a failed result with a message and no artifacts.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            changed_files: Vec::new(),
            transcript: String::new(),
        }
    }
}

/// Runs a single agent invocation. Implemented by the real subprocess invoker and
/// the deterministic stub invoker.
pub trait Invoker {
    fn invoke(&self, invocation: &AgentInvocation) -> InvocationResult;

    /// Streaming variant: emit [`LoopEvent`]s to `sink` as they happen. The default
    /// implementation runs `invoke` and emits nothing, so invokers that don't stream
    /// keep working unchanged.
    fn invoke_streaming(
        &self,
        invocation: &AgentInvocation,
        _sink: &mut dyn FnMut(LoopEvent),
    ) -> InvocationResult {
        self.invoke(invocation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_spec_uses_claude_binary_and_env_override() {
        let spec = spec_for(Adapter::Claude);
        assert_eq!(spec.default_program, "claude");
        assert_eq!(spec.env_override, "LOOPE_CLAUDE_BIN");
        assert!(spec.has_real_cli);
    }

    #[test]
    fn override_value_wins_over_default() {
        let spec = spec_for(Adapter::Codex);
        let resolved = resolve_program_from(&spec, Some("/opt/codex"));
        assert_eq!(resolved.as_deref(), Some("/opt/codex"));
    }

    #[test]
    fn program_exists_finds_path_binaries_only() {
        // `sh` is on PATH on every unix; a nonsense name is not, and a bogus absolute
        // path is not executable.
        assert!(program_exists("sh"));
        assert!(!program_exists("loope-definitely-not-a-real-binary-xyz"));
        assert!(!program_exists("/nonexistent/loope-xyz"));
    }

    #[test]
    fn check_adapters_probes_the_three_real_clis() {
        let statuses = check_adapters();
        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses[0].adapter, Adapter::Claude);
        // Each resolves a program name even when not installed.
        assert!(statuses.iter().all(|s| s.program.is_some()));
    }

    #[test]
    fn blank_override_falls_back_to_default() {
        let spec = spec_for(Adapter::Codex);
        assert_eq!(
            resolve_program_from(&spec, Some("   ")).as_deref(),
            Some("codex")
        );
        assert_eq!(resolve_program_from(&spec, None).as_deref(), Some("codex"));
    }

    #[test]
    fn adapter_parses_case_insensitively() {
        assert_eq!(Adapter::parse("claude"), Some(Adapter::Claude));
        assert_eq!(Adapter::parse("  Codex "), Some(Adapter::Codex));
        assert_eq!(Adapter::parse("OPENCODE"), Some(Adapter::OpenCode));
        assert_eq!(Adapter::parse("generic"), Some(Adapter::Generic));
        assert_eq!(Adapter::parse("gpt"), None);
    }

    #[test]
    fn generic_adapter_has_no_program() {
        let spec = spec_for(Adapter::Generic);
        assert!(!spec.has_real_cli);
        assert_eq!(resolve_program_from(&spec, None), None);
        // an explicit override still works for the generic adapter
        assert_eq!(
            resolve_program_from(&spec, Some("my-agent")).as_deref(),
            Some("my-agent")
        );
    }
}
