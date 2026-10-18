# Nocturne Studio brand assets

`logo/nocturne-logo.png` is the canonical generated raster logo: an ivory and muted-peach pixel-art N with interlocking strokes on a transparent background. It was produced with the built-in Imagegen tool. The exact prompt and source are recorded in `logo/nocturne-logo.provenance.json`.

Use this image as the only source of the mark. Do not redraw its contours in SVG, CSS or individual rectangles. Preserve the native alpha and proportions. Small UI instances use normal image sampling so the high-resolution artwork remains readable.

## Application copies and exports

- `../public/nocturne.png`: 128px transparent renderer copy.
- `icons/nocturne-{32,128,512}.png`: transparent size exports.
- `icons/favicon.png` and `../public/favicon.png`: small mark on the charcoal app tile.
- `icons/electron.png`, `icons/github.png` and `../build/icon.png`: 512px application tile.

The tile keeps the pale artwork visible in browser tabs and launchers. Raster exports only resize or place the original image on its background; they do not alter the illustration. Each PNG embeds its prompt or source origin. Keep the logo small in navigation and assistant identity; no large decorative illustration is used in the application.
