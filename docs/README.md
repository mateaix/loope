# Loope documentation

This directory holds two kinds of documentation, kept separate because they serve
different readers:

| Folder | Audience | Contents |
| --- | --- | --- |
| [`guide/`](guide/) | **users** | how to use Loope — commands, flags, adapters, troubleshooting |
| [`specs/`](specs/) | **contributors** | SDD specs: what each capability is and why |
| [`plans/`](plans/) | **contributors** | SDD implementation plans: the task breakdown per capability |
| [`prototype/`](prototype/) | **contributors** | the product narrative / prototype |
| [`../benchmarks/`](../benchmarks/) | **contributors** | evaluation methodology, metrics, runners, and results |

## Start here

- New to Loope? → **[guide/usage.md](guide/usage.md)** — the complete usage reference.
- Want the design rationale for a feature? → its spec in [`specs/`](specs/) and plan in
  [`plans/`](plans/).

## SDD capabilities

Each capability ships spec-first as a `specs/` + `plans/` pair:

- MVP plan generation
- Agent integration (real Claude/Codex execution)
- Review orchestration (structured verdicts, parallel reviewers, presets)
- CLI UX (visual identity)
- Live execution visibility (event stream + diffs)
- Live terminal rendering (animated status, timing, hunked diffs)
- OpenCode adapter
- Design Contract generation
- Robustness (per-step timeout + per-step artifact directories)
- Iterative loop (convergence, feedback, `apply`, cumulative diff) — v1.0
- Source layout (domain-grouped modules: `model` / `adapter` / `engine` / `cli`)
- Interactive TUI (ratatui browser + live dashboard, behind the optional `tui` feature)
- TUI slash commands (a `/` palette to configure runs and invoke tools)
- Convergence highlight (the "caught & fixed" card — reviewer blocks, next iteration fixes)
- Desktop hub (a graphical app: multi-agent management + visual execution rendering) — *spec-first, not yet built*
- Liquid Glass design (the desktop visual language + TUI→GUI surface mapping) — *design spec*

> **Maintenance:** when a feature, command, flag, preset, adapter, env var, exit code, or
> run-directory detail changes, update [`guide/usage.md`](guide/usage.md) (and the
> project README) in the same change, and add the new capability's spec + plan here.
