# Loope Desktop

A graphical hub for the loop, in the **Liquid Glass** style — it manages the agent CLIs and
presents the loop's plan and the agents' execution content visually. See the design in
[`docs/specs/2026-06-29-loope-liquid-glass-design-spec.md`](../docs/specs/2026-06-29-loope-liquid-glass-design-spec.md)
and the architecture in
[`docs/specs/2026-06-29-loope-desktop-hub-spec.md`](../docs/specs/2026-06-29-loope-desktop-hub-spec.md).

## Independent of the core

This is a **separate crate**, deliberately excluded from the `loope` workspace. It carries
its own dependency tree (Tauri + a path dependency on `loope`), so the std-only `loope`
crate keeps `deps = 1` and its offline builds are never affected. The TUI (the `tui` cargo
feature) and this desktop app are built and packaged separately.

The backend is a thin layer over `loope::hub`: each command maps a hub function to an IPC
call (converting the core's plain types into serde DTOs, so `loope` stays serde-free). The
front-end is **vanilla** HTML/CSS/JS — no npm or bundler — served statically by Tauri.

## Build & run

Prerequisites: a Rust toolchain, the platform's webview libraries (WebKitGTK on Linux;
nothing extra on macOS/Windows), and the Tauri CLI:

```bash
cargo install tauri-cli --version '^2'
```

Generate the app icons once (see `icons/README.md`):

```bash
cd src-tauri
cargo tauri icon path/to/logo.png
```

Then:

```bash
cd src-tauri
cargo tauri dev      # run in development
cargo tauri build    # produce a distributable bundle
```

## Layout

```
src-tauri/
  Cargo.toml          standalone crate: tauri + loope (path dep)
  tauri.conf.json     window + bundle config; frontendDist = ui/, withGlobalTauri
  build.rs            tauri_build
  src/
    main.rs           thin entry → lib::run()
    lib.rs            Tauri commands over loope::hub + DTOs
  ui/                 vanilla front-end (Liquid Glass)
    index.html · app.css · app.js
  icons/              generated app icons
```
