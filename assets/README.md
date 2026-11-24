# NOCT brand assets

`logo/nocturne-logo.png` is the canonical generated raster logo: an ivory and muted-peach pixel-art N with interlocking strokes on a transparent background. It was produced with the built-in Imagegen tool. The exact prompt and source are recorded in `logo/nocturne-logo.provenance.json`.

Use this image as the only source of the mark. Do not redraw its contours in SVG, CSS or individual rectangles. Preserve the native alpha and proportions. Small UI instances use normal image sampling so the high-resolution artwork remains readable.

## Application copies

- `../crates/noct-app/icons/icon.png`: 512px application tile, used by the installers (Linux, macOS) and the window.
- `../crates/noct-app/icons/icon.ico`: the Windows icon (16, 24, 32, 48, 64, 128 and 256 px, made from `icon.png`), used by the executable and the installer.

The interface itself shows no raster logo: the sidebar writes the name in text.

The tile keeps the pale artwork visible in browser tabs and launchers. These are the copies consumed by the application and packaging; unused size exports and duplicate tiles have been removed. Raster exports only resize or place the original image on its background; they do not alter the illustration. Each PNG embeds its prompt or source origin. Keep the logo small in navigation and assistant identity; no large decorative illustration is used in the application.
