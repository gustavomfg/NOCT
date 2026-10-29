# NOCT brand assets

`logo/nocturne-logo.png` is the canonical generated raster logo: an ivory and muted-peach pixel-art N with interlocking strokes on a transparent background. It was produced with the built-in Imagegen tool. The exact prompt and source are recorded in `logo/nocturne-logo.provenance.json`.

Use this image as the only source of the mark. Do not redraw its contours in SVG, CSS or individual rectangles. Preserve the native alpha and proportions. Small UI instances use normal image sampling so the high-resolution artwork remains readable.

## Application copies

- `../public/nocturne.png`: 128px transparent renderer copy used in navigation and assistant identity.
- `../public/favicon.png`: small mark on the charcoal app tile, used by `index.html`.
- `../build/icon.png`: 512px application tile, used by Electron and electron-builder.

The tile keeps the pale artwork visible in browser tabs and launchers. These are the copies consumed by the application and packaging; unused size exports and duplicate tiles have been removed. Raster exports only resize or place the original image on its background; they do not alter the illustration. Each PNG embeds its prompt or source origin. Keep the logo small in navigation and assistant identity; no large decorative illustration is used in the application.
