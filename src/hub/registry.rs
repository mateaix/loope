//! The agent registry: a queryable list of the coding-agent CLIs Loope can drive, what
//! each can do, and whether it is installed locally.
//!
//! This generalizes the one-off `doctor` probing into a reusable registry: a static
//! [`AgentDescriptor`] per adapter (capabilities + install hint), plus on-demand
//! [`detect`](AgentRegistry::detect)ion of availability and version behind a short-lived
//! cache. Detection goes through the [`Prober`] trait so callers (and tests) can supply
//! their own — the real one shells out to the CLI; tests inject a fake.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::Adapter;

/// What an agent's CLI is able to stream or do — a small capability bitset so the UI can
/// adapt instead of hardcoding per-agent behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capabilities(u32);

impl Capabilities {
    /// Streams incremental assistant text.
    pub const STREAM_TEXT: Capabilities = Capabilities(1 << 0);
    /// Streams tool/command calls as it works.
    pub const STREAM_TOOLS: Capabilities = Capabilities(1 << 1);
    /// Streams "thinking" / reasoning.
    pub const STREAM_REASONING: Capabilities = Capabilities(1 << 2);
    /// Accepts image input.
    pub const IMAGE_INPUT: Capabilities = Capabilities(1 << 3);
    /// Can resume a prior session.
    pub const RESUME: Capabilities = Capabilities(1 << 4);
    /// Has a configurable global config.
    pub const CONFIG: Capabilities = Capabilities(1 << 5);

    /// An empty set.
    pub const fn empty() -> Capabilities {
        Capabilities(0)
    }

    /// True if `self` contains every flag in `other`.
    pub const fn contains(self, other: Capabilities) -> bool {
        self.0 & other.0 == other.0
    }

    /// The raw bits (for persistence / display).
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for Capabilities {
    type Output = Capabilities;
    fn bitor(self, rhs: Capabilities) -> Capabilities {
        Capabilities(self.0 | rhs.0)
    }
}

/// Static description of one agent: which adapter it maps to, what it can do, and how to
/// install it when missing. The binary name and override env var come from the adapter spec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentDescriptor {
    pub adapter: Adapter,
    pub capabilities: Capabilities,
    /// A one-line command that installs the CLI when it is missing.
    pub install_hint: &'static str,
}

impl AgentDescriptor {
    /// Stable lowercase id (the adapter id).
    pub fn id(&self) -> &'static str {
        self.adapter.as_str()
    }

    /// Human label.
    pub fn display_name(&self) -> &'static str {
        self.adapter.display_name()
    }

    /// Default binary name on `PATH`.
    pub fn binary(&self) -> &'static str {
        crate::adapter::spec_for(self.adapter).default_program
    }

    /// Environment variable that overrides the binary path.
    pub fn env_override(&self) -> &'static str {
        crate::adapter::spec_for(self.adapter).env_override
    }
}

/// The result of probing one agent: is its CLI installed, at what version, resolved to
/// which program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Detected {
    pub available: bool,
    pub version: Option<String>,
    pub program: Option<String>,
}

/// The known agents, with their capabilities and install hints.
pub fn descriptors() -> Vec<AgentDescriptor> {
    use Capabilities as C;
    vec![
        AgentDescriptor {
            adapter: Adapter::Claude,
            capabilities: C::STREAM_TEXT
                | C::STREAM_TOOLS
                | C::STREAM_REASONING
                | C::IMAGE_INPUT
                | C::RESUME
                | C::CONFIG,
            install_hint: "npm install -g @anthropic-ai/claude-code",
        },
        AgentDescriptor {
            adapter: Adapter::Codex,
            capabilities: C::STREAM_TEXT | C::STREAM_TOOLS | C::IMAGE_INPUT | C::RESUME | C::CONFIG,
            install_hint: "npm install -g @openai/codex",
        },
        AgentDescriptor {
            adapter: Adapter::OpenCode,
            capabilities: C::STREAM_TEXT | C::STREAM_TOOLS | C::STREAM_REASONING | C::RESUME | C::CONFIG,
            install_hint: "npm install -g opencode-ai",
        },
    ]
}

/// Probes whether an agent's CLI is installed. Pluggable so tests don't shell out.
pub trait Prober {
    fn probe(&self, descriptor: &AgentDescriptor) -> Detected;
}

/// The real prober: resolves the program (honoring the override env var), checks `PATH`,
/// and reads `<program> --version`.
pub struct RealProber;

impl Prober for RealProber {
    fn probe(&self, descriptor: &AgentDescriptor) -> Detected {
        let spec = crate::adapter::spec_for(descriptor.adapter);
        let program = crate::adapter::resolve_program(&spec);
        let available = program.as_deref().is_some_and(crate::adapter::program_exists);
        let version = if available {
            program.as_deref().and_then(probe_version)
        } else {
            None
        };
        Detected {
            available,
            version,
            program,
        }
    }
}

/// Read `<program> --version` and return its first non-empty line.
fn probe_version(program: &str) -> Option<String> {
    let output = std::process::Command::new(program)
        .arg("--version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    Some(line.trim().to_string())
}

/// A registry of the known agents that caches detection results for a short window.
pub struct AgentRegistry {
    descriptors: Vec<AgentDescriptor>,
    cache: Mutex<HashMap<&'static str, (Detected, Instant)>>,
    ttl: Duration,
}

impl AgentRegistry {
    /// A registry with the default 60s detection cache.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(60))
    }

    /// A registry whose detection cache lives for `ttl` (use `Duration::ZERO` to disable
    /// caching — every `detect` re-probes).
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            descriptors: descriptors(),
            cache: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// All known agents.
    pub fn descriptors(&self) -> &[AgentDescriptor] {
        &self.descriptors
    }

    /// The agent with this id, if known.
    pub fn get(&self, id: &str) -> Option<AgentDescriptor> {
        self.descriptors.iter().copied().find(|d| d.id() == id)
    }

    /// Detect one agent, serving a cached result when it is fresher than the TTL.
    pub fn detect(&self, descriptor: &AgentDescriptor, prober: &dyn Prober) -> Detected {
        let key = descriptor.id();
        if !self.ttl.is_zero()
            && let Ok(cache) = self.cache.lock()
            && let Some((detected, at)) = cache.get(key)
            && at.elapsed() < self.ttl
        {
            return detected.clone();
        }
        let detected = prober.probe(descriptor);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, (detected.clone(), Instant::now()));
        }
        detected
    }

    /// Detect every known agent.
    pub fn detect_all(&self, prober: &dyn Prober) -> Vec<(AgentDescriptor, Detected)> {
        self.descriptors
            .iter()
            .map(|d| (*d, self.detect(d, prober)))
            .collect()
    }

    /// Drop all cached detection results so the next `detect` re-probes.
    pub fn invalidate(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingProber {
        calls: AtomicUsize,
        detected: Detected,
    }

    impl CountingProber {
        fn new(detected: Detected) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                detected,
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl Prober for CountingProber {
        fn probe(&self, _descriptor: &AgentDescriptor) -> Detected {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.detected.clone()
        }
    }

    fn detected() -> Detected {
        Detected {
            available: true,
            version: Some("1.2.3".to_string()),
            program: Some("claude".to_string()),
        }
    }

    #[test]
    fn capabilities_compose_and_test() {
        let caps = Capabilities::STREAM_TEXT | Capabilities::RESUME;
        assert!(caps.contains(Capabilities::STREAM_TEXT));
        assert!(caps.contains(Capabilities::RESUME));
        assert!(!caps.contains(Capabilities::IMAGE_INPUT));
        assert!(caps.contains(Capabilities::STREAM_TEXT | Capabilities::RESUME));
        assert!(!Capabilities::empty().contains(Capabilities::STREAM_TEXT));
    }

    #[test]
    fn descriptors_cover_the_real_agents() {
        let reg = AgentRegistry::new();
        let ids: Vec<_> = reg.descriptors().iter().map(|d| d.id()).collect();
        assert_eq!(ids, ["claude", "codex", "opencode"]);
        for d in reg.descriptors() {
            assert!(!d.install_hint.is_empty());
            assert!(!d.binary().is_empty());
            assert!(d.capabilities.contains(Capabilities::STREAM_TEXT));
        }
        assert!(
            reg.get("claude")
                .unwrap()
                .capabilities
                .contains(Capabilities::STREAM_REASONING)
        );
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn detection_is_cached_within_the_ttl() {
        let reg = AgentRegistry::with_ttl(Duration::from_secs(60));
        let claude = reg.get("claude").unwrap();
        let prober = CountingProber::new(detected());

        let first = reg.detect(&claude, &prober);
        let second = reg.detect(&claude, &prober);
        assert_eq!(first, second);
        assert_eq!(prober.calls(), 1, "second detect should hit the cache");

        reg.invalidate();
        reg.detect(&claude, &prober);
        assert_eq!(prober.calls(), 2, "invalidate forces a re-probe");
    }

    #[test]
    fn zero_ttl_disables_the_cache() {
        let reg = AgentRegistry::with_ttl(Duration::ZERO);
        let codex = reg.get("codex").unwrap();
        let prober = CountingProber::new(detected());

        reg.detect(&codex, &prober);
        reg.detect(&codex, &prober);
        assert_eq!(prober.calls(), 2);
    }

    #[test]
    fn detect_all_covers_every_agent() {
        let reg = AgentRegistry::new();
        let prober = CountingProber::new(Detected {
            available: false,
            version: None,
            program: None,
        });
        let all = reg.detect_all(&prober);
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|(_, d)| !d.available));
    }
}
