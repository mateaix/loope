# Loope TUI Slash Commands Implementation Plan

> Add a `/` command palette to the home prompt. All in the `tui` feature; no new deps.
> Commit once at the end.

**Spec:** [TUI Slash Commands Spec](../specs/2026-06-29-loope-tui-commands-spec.md)

---

## Tasks

### Task 1: RunOptions + command model

- [ ] `tui/config.rs`: `RunOptions` (max_iters, implementer, reviewers, designer,
      include_design, verify_command, dry_run) with `Default`, `summary() -> String`,
      `config(requirement) -> LoopConfig`, and `make_invoker()`.
- [ ] `tui/command.rs`: `Command` enum; `SPECS` (name, args, help); `parse(&str) ->
      Result<Command, String>`; `matches(prefix) -> Vec<&Spec>`; unit tests.

### Task 2: Wire commands into the app

- [ ] `App`: add `options: RunOptions`, palette state (`palette: Vec<usize>`,
      `palette_selected`), and `message: Option<String>`.
- [ ] `App::command_mode()` (input starts with `/`), `refresh_palette()`,
      `palette_up/down`, `complete_palette()` (Tab), `run_command()` (parse + apply +
      set message). Settings commands mutate `options`; `/browse`/`/quit`/`/help` act;
      `/apply` defers to the event loop (needs the runs dir) via an intent like submit.
- [ ] `Session::start` takes `&RunOptions`; the home launch builds from `options`.

### Task 3: Home key handling + apply tool

- [ ] `handle_key` (Home): in command mode, `↑/↓` move the palette, `Tab` completes,
      `Enter` runs the command, `Esc` clears to requirement mode; otherwise unchanged.
- [ ] `/apply`: apply the selected run's `changed-files.txt` into the cwd (shared helper),
      report the count in the status message.

### Task 4: Views

- [ ] `view/home.rs`: a **status line** (options summary) above the input; when in command
      mode, render the **palette** (matching commands + descriptions, selection
      highlighted) above the input; show the transient `message` (red on error) in the
      footer.

### Task 5: Tests + docs

- [ ] Tests: command parsing/aliases/errors, palette filtering, each settings command
      mutating `RunOptions`, `config()` reflecting them, a render snapshot in command mode.
- [ ] Docs: `docs/guide/usage.md` (a "Slash commands" subsection under Interactive TUI)
      and README. `cargo clippy` + tests green with and without `--features tui`. Commit.
