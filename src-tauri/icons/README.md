# App icons

A default brand icon set (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`) ships
here so the app builds out of the box — Tauri reads the files listed under `bundle.icon` in
`tauri.conf.json`.

To replace them with your own logo (and produce `.icns` / `.ico` for distribution):

```bash
# from src-tauri/, point at any square PNG (≥ 512×512):
cargo tauri icon path/to/logo.png
```

This writes `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, and `icon.ico`
into this directory.
