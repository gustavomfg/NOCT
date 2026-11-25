# NOCT application icons

The interface shows no raster logo: the sidebar writes the name in text.

- `icon/noct-icon-1024.png`: the source of the application icon, 1024 px, RGBA with 8 bits per channel: a light crescent moon on a dark rounded tile, drawn by a script.
- `../crates/noct-app/icons/`: every size made from it with `cargo tauri icon assets/icon/noct-icon-1024.png` (the PNGs, `icon.ico` for Windows and `icon.icns` for macOS); the Android and iOS sets are not kept. Make them again from the source, never edit them by hand.

`tauri.conf.json` (`bundle.icon`) names the PNGs, `icon.icns` and `icon.ico`, and `crates/noct-app/tests/installers.rs` requires them.
