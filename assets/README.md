# NOCT application icons

The interface shows no raster logo: the sidebar writes the name in text.

- `../crates/noct-app/icons/icon.png`: 512px application tile, used by the installers (Linux, macOS) and the window.
- `../crates/noct-app/icons/icon.ico`: the Windows icon (16, 24, 32, 48, 64, 128 and 256 px, made from `icon.png`), used by the executable and the installer.

Both still carry the artwork of the earlier brand and will be replaced by a new icon. `tauri.conf.json` (`bundle.icon`) and `crates/noct-app/tests/installers.rs` require both files.
