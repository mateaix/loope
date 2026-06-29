# App icons

Generate the icon set before the first build (Tauri reads the files listed under
`bundle.icon` in `tauri.conf.json`):

```bash
# from src-tauri/, point at any square PNG (≥ 512×512):
cargo tauri icon path/to/logo.png
```

This writes `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, and `icon.ico`
into this directory.
