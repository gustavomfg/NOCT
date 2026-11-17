# npm audit triage

- **Status:** Recorded. Nothing is fixed. Waiting for maintainer review.
- **Date:** 2026-11-17, `migration/rust` at `0ae0fb8`.
- **Command:** `npm audit --json`, read-only, against `package-lock.json`, with Node 24.18.0 and npm 11.18.0. The raw JSON was written under `target/` and is not committed.

## Result

- 13 findings: 13 moderate, 0 high, 0 critical.
- The dependency tree has 572 packages: 112 production, 461 development, 55 optional and 13 peer.
- Every finding comes from one advisory, GHSA-g3hv-75wg-m249 (`minimatch`: potential denial of service through globparts amplification). Each affected package reaches it through a dependency chain that ends in `minimatch`.

## Scope of each finding

Each package was checked with `npm ls --package-lock-only --omit=dev <name>`. None of the 13 names appears in the production tree.

| Package | Route to the advisory | Reached through | Production |
| --- | --- | --- | --- |
| `minimatch` (two copies) | The advisory itself | Transitive | No |
| `eslint` | `minimatch` | Direct devDependency `^10.7.0` | No |
| `@eslint/config-array` | `minimatch` | `eslint` | No |
| `@typescript-eslint/parser` | `@typescript-eslint/typescript-estree` | Direct devDependency `^8.64.0` | No |
| `@typescript-eslint/eslint-plugin` | `@typescript-eslint/type-utils`, `@typescript-eslint/utils` | Direct devDependency `^8.64.0` | No |
| `@typescript-eslint/type-utils` | `@typescript-eslint/typescript-estree`, `@typescript-eslint/utils` | `@typescript-eslint/eslint-plugin` | No |
| `@typescript-eslint/typescript-estree` | `minimatch` | `@typescript-eslint/parser` and others | No |
| `@typescript-eslint/utils` | `@typescript-eslint/typescript-estree` | `@typescript-eslint/eslint-plugin` | No |
| `electron-builder` | `app-builder-lib`, `dmg-builder` | Direct devDependency `^26.15.7` | No |
| `app-builder-lib` | `@electron/universal`, `dmg-builder`, `electron-builder-squirrel-windows`, `minimatch` | `electron-builder` | No |
| `dmg-builder` | `app-builder-lib` | `electron-builder` | No |
| `electron-builder-squirrel-windows` | `app-builder-lib` | `electron-builder` | No |
| `@electron/universal` | `minimatch` | `app-builder-lib` | No |

All 13 are development tooling for the TypeScript application: the linters, the TypeScript parser and the Electron packager.

## Path into the Rust build

- **Build path: no finding is on it.** The Rust application is built with `cargo` and `trunk build --release`, as set in `crates/noct-app/tauri.conf.json` (`build.beforeBuildCommand` and `build.beforeDevCommand`). Neither command runs npm or Node, so no npm package is compiled into the Rust application.
- **CI path: findings are installed on the runners.** The Rust workflows install the full npm tree, development packages included:
  - `.github/workflows/row-gate.yml`, step "Install the Node dependencies": `npm ci`.
  - `.github/workflows/rust-shell-validation.yml`: `npm ci`, and `npx playwright install --with-deps chromium`.
  The advisory therefore exists on CI runners while the TypeScript checks run. This is not measured. The exposure is assessed as low: it needs a tool to process a glob pattern that an attacker controls, and the CI jobs process only repository content.

## Not fixed, and why

`npm audit fix` is not run. The audit offers two kinds of fix:

- `electron-builder@25.1.0` for the packaging chain. This is a downgrade from `^26.15.7`, so it is not applied.
- The ESLint and typescript-eslint chain (`fixAvailable: true`), which can move within the declared ranges. It is a dependency change, so it needs its own commit, with the TypeScript gates run first.

These packages leave the repository in phase 9 of [ADR 0001](adr/0001-rust-migration.md), when the TypeScript tooling is retired. Until then, the findings stay on the TypeScript side.

## Options for review

- **A (proposed):** keep this record. Update the ESLint and typescript-eslint chain in a separate commit after the TypeScript gates pass. Decide on `electron-builder` with the packaging decision in phase 8.
- **B:** add an audit step that fails only on production findings, for example `npm audit --omit=dev --audit-level=high`, in the CI workflows. This changes a workflow, so it needs approval first.
