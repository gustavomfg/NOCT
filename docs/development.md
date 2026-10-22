# Development

[Português do Brasil](development.pt-BR.md)

## Environment

- Node.js `>=24.18 <25`;
- npm `>=11 <12`;
- WebStorm is recommended and the `webstorm` launcher should be available in
  `PATH`;
- native tooling compatible with the Electron ABI and `better-sqlite3`.

## Daily commands

```bash
npm ci
npm run dev
npm run typecheck
npm run lint
npm test
npm run build
```

Additional validation:

```bash
npm run test:abi
npm run test:renderer
npm run benchmark:sqlite
```

`npm run test:renderer` runs Playwright. `npm run test:abi` verifies the native
SQLite module inside the adopted Electron runtime.

The renderer expects the Electron preload bridge (`window.nocturne`). Use
`npm run dev` to launch the supported desktop shell; opening the Vite URL in a
standalone browser does not provide native capabilities and intentionally lands
in the recovery boundary. Playwright uses a local bridge mock for renderer-only
coverage.

## Packaging and smoke checks

```bash
npm run package -- --publish never
npm run package:dir -- --publish never
npm run smoke:package
npm run rehearse:packaged-recovery
```

The rehearsal commands use isolated temporary data and are release-engineering
checks, not a replacement for signed release validation.

Run `git diff --check` before submitting a change. Keep tests, contracts,
preload APIs and documentation aligned when changing a native capability.

## Generated files and repository layout

Builds (`dist/`, `dist-electron/`, `dist-native/`), local packages (`release/`),
renderer captures (`output/`), test reports and the Vite dependency cache are
reproducible local output and stay ignored. Old packages can be removed locally;
published installers remain attached to their versioned GitHub releases.

Keep only referenced images in `docs/images/` and active comparison fixtures in
`tests/renderer/__screenshots__/`. The canonical brand source and the three
application image copies are described in [assets](../assets/README.md).
Superseded planning drafts live in [the historical archive](archive/README.md).

`.nocturne/`, credentials, databases and local editor settings are not build
caches. Preserve them when cleaning generated output. Schema migrations and
legacy installation identifiers remain necessary for supported upgrades.
