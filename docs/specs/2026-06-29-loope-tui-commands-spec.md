# Loope TUI Slash Commands Spec

## Background

The home prompt ([TUI spec](2026-06-29-loope-tui-spec.md)) launches a run from a typed
requirement, but with **fixed defaults** — Claude + Codex, 3 iterations, no verify
command, no design step. Everything the CLI exposes as flags (`--max-iters`, `--preset`,
`--reviewers`, `--verify-cmd`, `--design`, `--dry-run`) is unreachable from the
interactive UI, and there is no way to invoke an action like `apply` without dropping back
to the shell.

Following Claude Code / Codex, this adds a **`/` command palette** to the prompt: typing
`/` turns the line into a command, an autocomplete list of commands + descriptions appears
above the input, and running a command either **configures the run** (iterations,
adapters, verify, design, dry-run) or **invokes a tool** (apply, browse, help, quit). A
persistent status line shows the current configuration so the loop's settings are always
visible.

Plain text (no leading `/`) is still a requirement that launches the loop.

## Goals

1. In the home prompt, a leading `/` enters **command mode**: a filtered palette of
   commands (name · args · description) shows above the input; `↑/↓` select, `Tab`
   completes the line to the selected command, `Enter` runs the typed command.
2. **Settings commands** mutate the run configuration used by the next launch:
   `/iters N`, `/preset NAME`, `/implementer A`, `/reviewers A[,B]`, `/verify CMD`
   (empty clears), `/design` (toggle), `/dry` (toggle stub agents).
3. **Tool commands** perform an action: `/apply` (land the selected run's changes into the
   working tree), `/browse` (open the run browser), `/help` (overlay), `/quit`.
4. A **status line** above the prompt always shows the current config (iterations,
   implementer + reviewers, verify command, design/dry flags); a transient **message**
   confirms a command or reports an error.
5. The CLI is unchanged; the same `RunOptions` maps to the same `LoopConfig`/invoker the
   flags already build, so the TUI and CLI stay consistent. No new dependencies; the slash
   system lives entirely in the `tui` feature.

## Non-Goals (v1)

- User-defined / scripted commands or plugins (the set is fixed and built-in).
- Fuzzy matching beyond prefix; one screen of results is enough for ~10 commands.
- Editing commands mid-run — commands are entered from the home prompt (a run in flight
  ignores them); changing settings affects the **next** launch.
- Per-command argument autocomplete (e.g. preset-name completion) — args are typed.

## Core Concepts

### Command mode

The home input has two modes, switched purely by the first character:

- **Requirement mode** (default): `Enter` launches the loop with the typed requirement.
- **Command mode** (`input` starts with `/`): a palette of commands whose names match the
  typed token appears above the input. `↑/↓` move the selection; `Tab` replaces the input
  with `/<selected> ` (ready for args); `Enter` parses and runs the full input line. `Esc`
  clears the input back to requirement mode (a second `Esc` quits).

### Run configuration (`RunOptions`)

A single struct holds what the flags hold, with the same defaults:

```text
max_iters=3  implementer=claude  reviewers=[codex]  designer=claude
include_design=false  verify_command=None  dry_run=<from launch>
```

`RunOptions::summary()` renders the status line; `RunOptions::config(requirement)` builds
the `LoopConfig`, and it selects the stub vs. subprocess invoker — the same mapping
`loope run` uses. Settings commands mutate this struct.

### Commands

| Command | Args | Effect |
| --- | --- | --- |
| `/iters` | `N` | set the iteration cap (alias `/max-iters`) |
| `/preset` | `NAME` | set implementer + reviewers from a preset |
| `/implementer` | `A` | set the implementer adapter |
| `/reviewers` | `A[,B]` | set the reviewer adapter(s) |
| `/verify` | `[CMD]` | set the verify command; no arg clears it |
| `/design` | — | toggle the design-contract step |
| `/dry` | — | toggle stub agents (no real CLIs) |
| `/apply` | — | copy the selected run's changed files into the working tree |
| `/browse` | — | open the run browser |
| `/help` | — | keys + command overlay |
| `/quit` | — | quit |

Unknown command or bad argument → a red status message; the input is kept so it can be
fixed.

## Acceptance Criteria

- Typing `/` shows a palette filtered by the typed prefix; `Tab` completes the selected
  command; `Enter` runs it; `Esc` leaves command mode.
- `/iters 5` then launching a requirement runs up to 5 iterations; `/preset dual-review`
  then launching uses two reviewers; `/verify cargo test` makes the verifier gate on the
  command; `/design` adds the design step; `/dry` uses stub agents. The status line
  reflects each change.
- `/apply` lands the selected run's changes into the working directory and reports how
  many files; `/browse`, `/help`, `/quit` act as described.
- The CLI surface and outputs are unchanged; default `cargo build` stays dependency-free;
  `cargo clippy` clean and tests green with and without `--features tui`.

## Testing Strategy

- **Parsing** (feature build): `parse("/iters 5")`, aliases, unknown commands, bad args,
  `/verify cargo test` capturing the rest of the line, toggles.
- **Filtering**: the palette prefix match for `/i`, `/`, `/verify`.
- **Application**: each settings command mutates `RunOptions` as expected;
  `RunOptions::config` reflects them; `summary()` includes the key fields.
- **Render** (TestBackend): the home frame in command mode shows the palette and status
  line; a settings change updates the status line.

## Related

- [[2026-06-29-loope-tui-spec]] — the home prompt these commands extend.
- [[2026-06-28-loope-iterative-loop-spec]] — `--max-iters` and the config the commands set.
