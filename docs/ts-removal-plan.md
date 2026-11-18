# Removal of the TypeScript and Electron side from `migration/rust`

Decision of the owner: the product is Rust only. This removal applies to `migration/rust` alone. `main` keeps the Electron
product, and nothing here touches `main`, pull request #23, the updater key, CI secrets or signing.

## Restore point

- Tag `electron-final`, annotated, at commit `c8c378b` (the last state with the Electron product), published to `origin`.
  The tag object is `e5b49be`. Anything removed here can be read or restored from it:
  `git restore --source=electron-final -- <path>`.

## What the Rust side reads from the TypeScript side

These are the only places where Rust code or Rust tests read a file that this plan removes. Found by reading the code
(`grep` over `crates/`), not by running the suite. The run after each step is the check.

Run in `cargo test --workspace` (not ignored):

| Test | Reads | Decision |
| --- | --- | --- |
| `crates/noct-core/tests/constants_parity.rs` (4 tests) | `shared/constants.ts`, `shared/ai/providerConfiguration.ts`, `shared/ai/model.ts` | PORT: copy the three files, byte for byte, into `crates/noct-core/tests/fixtures/ts-reference/` as `*.ts.txt`. The test reads the copies. |
| `crates/noct-core/tests/ipc_contracts.rs` (1 test reads it: `the_command_names_replace_the_typescript_channels`, line 105) | `shared/ipc/channels.ts` | PORT: copy to `crates/noct-core/tests/fixtures/ts-reference/channels.ts.txt`. The other tests in the file read fixtures already in the repo. |
| `crates/noct-db/tests/observer.rs` (1 test, `the_catalog_names_what_the_typescript_runtime_reports`, line 130) | every `electron/database/*.ts` source, scanned for `measure('` and `run('` | PORT: extract the two name lists now into `crates/noct-db/tests/fixtures/typescript-observer-names.json`. The test compares the catalog with the JSON. |
| `crates/noct-app/tests/shell_ui_mock.rs` | `scripts/check-shell-ui.mjs` | KEEP: the script stays (Node, Playwright). |
| `crates/noct-app/tests/updater_config.rs` | `docs/noct-updater.key.pub` | KEEP: a public key in `docs/`. |
| `crates/noct-app/tests/shell_policy.rs` (line 132) and `crates/noct-ui/index.html` | `scripts/set-glass.mjs` | KEEP: the script stays (Node). |
| `crates/noct-app/src/updater.rs`, `crates/noct-app/tests/dev_server.rs`, `updater_config.rs` | `crates/noct-app/tauri.conf.json`, `crates/noct-ui/Trunk.toml` | KEEP: Rust-side configuration. |

Ignored, run only by the TypeScript gates (`#[ignore]`, so `cargo test --workspace` does not run them):

| Test | Needs | Decision |
| --- | --- | --- |
| `crates/noct-db/tests/row_gate.rs` (10 tests) | `NOCT_ROW_GATE_DIR`, produced by `scripts/verify-row-gate.mjs`, which compiles the TypeScript of every release tag | DELETE with the gate. Its oracle is TypeScript. The evidence stays at `electron-final`. |
| `crates/noct-db/tests/rust_written_gate.rs` (2 tests) | `NOCT_RUST_WRITTEN_DIR`, from `scripts/verify-rust-written-gate.mjs` (TypeScript repositories) | DELETE with the gate. |
| `crates/noct-app/tests/output_differential.rs` (1 test) | release-line bases from `scripts/generate-output-differential.mjs` | DELETE with the gate. |

The bases of the release lines are databases written by the TypeScript code of each tag, generated at run time and
never committed (`scripts/release-lines.mjs`, and `AGENTS.md`: no databases in the repository). They cannot be frozen
into the repository without breaking that rule, so the gate that needs them cannot run on this branch after the removal.

## Inventory by group

Legend: **APAGAR** (remove in the step named), **MANTER** (stays), **PORTAR** (rewritten to run on the Rust side or on Node).

### 1. Renderer and Electron product (step a)

| Path | Tracked files | Decision | Why |
| --- | --- | --- | --- |
| `electron/` | 137 | APAGAR | Electron main process, preload and IPC. Replaced by `crates/noct-app`. `electron/database/migrations.ts` is the source of `crates/noct-db/src/migrations/generated.rs`, which stays as Rust source. |
| `src/` | 87 | APAGAR | React renderer. Replaced by `crates/noct-ui` (Leptos). No Rust file reads `src/`. Its CSS is checked only by `scripts/check-design-system.mjs`, which is removed (group 5). |
| `index.html` (root) | 1 | APAGAR | Vite entry of the renderer. `crates/noct-ui/index.html` is the Rust entry and stays. |
| `public/` | 2 | APAGAR | Vite static files (`favicon.png`, `nocturne.png`). No Rust file references them. |
| `dist-electron/`, `dist-native/`, `dist/` | untracked | APAGAR locally | Ignored build output of Electron and Vite. Listed, not tracked. Removed from disk after step a. `crates/noct-ui/dist` (Trunk output) stays. |

### 2. Shared TypeScript contracts (step b)

| Path | Tracked files | Decision | Why |
| --- | --- | --- | --- |
| `shared/` | 40 | APAGAR | TypeScript contracts. The four files that Rust tests read are copied first (see the table above). |

### 3. TypeScript tests and their configuration (step b)

| Path | Tracked files | Decision | Why |
| --- | --- | --- | --- |
| `tests/*.ts` | 104 | APAGAR, except three | Tests of the Electron product, its repositories and its renderer. Three are not about Electron, and are PORTED (group 4). |
| `tests/renderer/` | screenshots | APAGAR | Visual baselines of the React renderer. |
| `vitest.config.ts`, `playwright.config.ts` | 2 | APAGAR | Config of the TypeScript test runners. After the port, nothing runs with them. |
| `tsconfig.json`, `tsconfig.node.json` | 2 | APAGAR | Config of `tsc`, which only the Electron build uses. |
| `eslint.config.js` | 1 | APAGAR | Lint of the TypeScript sources. It also names `check-leptos-spike.mjs`, which is removed. |
| `vite.config.ts` | 1 | APAGAR | Build of the React renderer. |
| `electron-builder.json5` | 1 | APAGAR | Electron packaging. |

### 4. Ported tests (step b, after the freeze)

| Source | Cases | Target | Why |
| --- | --- | --- | --- |
| `tests/shellRelease.test.ts` | 17 | `scripts/shell-release.test.mjs` (`node:test`) | Checks `.github/workflows/shell-release.yml`, the Rust release, which stays. |
| `tests/shellLatestJson.test.ts` | 10 | `scripts/shell-latest-json.test.mjs` (`node:test`) | Checks `scripts/shell-latest-json.mjs`, which stays. |
| `tests/rustShellValidation.test.ts` | 5 | `scripts/rust-shell-validation.test.mjs` (`node:test`) | Checks `.github/workflows/rust-shell-validation.yml`, which stays. |

The assertions are kept. Only the runner changes (`vitest` to `node:test`), and the test command becomes
`node --test scripts/*.test.mjs`.

### 5. Scripts (`scripts/`, 34 tracked files) (step c)

Each script that the Rust flow uses, and what happens to it:

| Script | Used by the Rust flow | Decision | Why |
| --- | --- | --- | --- |
| `run-rust-shell.mjs` | yes (`rust-shell-validation.yml`, docs) | KEEP, adjust | Node. The `--release-line` option imports `release-lines.mjs` (TypeScript compile of a tag), so that option is removed with its test case. |
| `run-rust-shell.test.mjs` | yes | KEEP, adjust | Node test of the launcher. Its release-line case is removed. |
| `set-glass.mjs` | yes (`crates/noct-app/tests/shell_policy.rs`, `crates/noct-ui/index.html`) | KEEP | Node. Pure file edit. |
| `check-shell-ui.mjs` | yes (CI, `shell_ui_mock.rs`) | KEEP | Node with Playwright. It exercises the Rust UI build. |
| `check-shell-csp.mjs` | yes (CI) | KEEP | Node with Playwright. |
| `shell-latest-json.mjs` | yes (`shell-release.yml`) | KEEP | Node, no TypeScript. |
| `verify-row-gate.mjs` | no (only `row-gate.yml`) | APAGAR | Its oracle is the TypeScript repositories. It cannot be ported without the TypeScript. Evidence at `electron-final`. |
| `verify-rust-written-gate.mjs` | no | APAGAR | Same reason. |
| `ipc-inventory.mjs` | no | APAGAR, not ported | It generates `docs/ipc-inventory.md` from the TypeScript channels. Without them it has no input. `docs/ipc-inventory.md` stays as the frozen record. |
| `generate-ipc-samples.mjs`, `generate-log-redaction-fixture.mjs`, `generate-schema-fixtures.mjs`, `transpile-migrations.mjs` | in `rust-shell-validation.yml` (two `--check` steps) | APAGAR | Each reads TypeScript (`zod`, `typescript`, `electron/database/migrations.ts`). Their JSON outputs stay as fixtures in `crates/*/tests/fixtures/`. |
| `generate-output-differential.mjs`, `release-lines.mjs`, `row-gate-typescript.mjs`, `fixtures/release-lines.json` | in `rust-shell-validation.yml` (differential step) and `row-gate.yml` | APAGAR | They compile the TypeScript of release tags. The manifest only serves them. |
| `run-tests.mjs`, `benchmark-sqlite-sync.mjs`, `build-native-boundary.mjs` | `package-validation.yml`, `native-rollback-validation.yml` | APAGAR | Electron test runner, benchmark of the TypeScript database, and the Electron native build. |
| `smoke-package.mjs`, `rehearse-packaged-recovery.mjs`, `rehearse-updater.mjs`, `updater-rehearsal-contract.mjs`, `updater-rehearsal-runtime.mjs`, `verify-updater-rehearsal-contract.mjs`, `verify-packaged-metadata.mjs` | Electron workflows | APAGAR | They run the packaged Electron app. |
| `stage-release-assets.mjs`, `verify-release-assets.mjs`, `verify-published-release.mjs`, `verify-release-metadata.mjs`, `verify-package-signature.mjs` | `stable-release.yml` | APAGAR | The Electron release. The Rust release is `shell-release.yml`, which does not use them. |
| `check-design-system.mjs` | `npm run lint` only | APAGAR | Checks the CSS of the React renderer (`src/`). |
| `check-leptos-spike.mjs` | none | APAGAR | A spike check. `check-shell-ui.mjs` covers the same `crates/noct-ui/dist`. |
| `transpile-migrations.mjs` and `generate-schema-fixtures.mjs` (see above) | | APAGAR | |

### 6. Workflows (`.github/workflows`, 16 files) (step c)

| Workflow | Decision | Why |
| --- | --- | --- |
| `package-validation.yml` (CI · source, renderer and packages) | APAGAR | Electron source, renderer and packages. |
| `stable-release.yml` | APAGAR | Electron release. |
| `updater-rehearsal.yml`, `packaged-recovery.yml` | APAGAR | Electron rehearsals. |
| `row-gate.yml` | APAGAR, not adjusted | Its only job is the TypeScript gate. Adjusting it would mean it no longer checks anything. |
| `native-rollback-validation.yml` | APAGAR | Runs the TypeScript native tests (`tests/nativeRollbackBoundary.test.ts`). The C++ source stays (`native/`), but nothing compiles it in CI after this. Stated as a loss. |
| `rust-shell-validation.yml` | ADJUST | Remove the `npm ci` steps that only serve TypeScript, the four TypeScript-derived checks and the differential. Keep the Node setup and `npm ci` (minimal lock) for Playwright. |
| `shell-release.yml` | KEEP | The Rust release. Its gate reads `rust-shell-validation.yml`, which is kept. |
| `keyring-round-trip.yml`, `rust-audit.yml`, `build-provenance-dry-run.yml` | KEEP | No TypeScript dependency. |
| `dependency-security.yml` | KEEP, adjust wording | `npm audit` and SBOM of the minimal lock. The step named "packaged Electron runtime" is renamed. |
| `.github/dependabot.yml` | KEEP | `npm` (Playwright) and `github-actions`. |

Repository settings, not files: CodeQL is configured in GitHub with `javascript-typescript` in its language set. That is
owner-level. Changing it needs the owner, and it is listed under "depends on the owner".

### 7. Package and lockfile (step d)

`package.json` and `package-lock.json` are reduced to the dependencies that the remaining Node scripts import:

- `playwright` (used by `check-shell-ui.mjs` and `check-shell-csp.mjs`).
- Scripts: `test` (`node --test scripts/*.test.mjs`), `check:ui`, `check:csp`, `shell:latest-json` if useful.

Every other dependency and script goes: `electron`, `electron-builder`, `electron-updater`, `better-sqlite3`, `react`,
`react-dom`, `zustand`, `zod`, `typescript`, `vite`, `vitest`, `eslint`, `@playwright/test`, and the `@fontsource` packages
(the Rust UI loads no fonts from npm, checked by grep: none in `crates/noct-ui`). The lockfile is regenerated with `npm install`
for the reduced set. No new package is added: `playwright` is already locked.

`.nvmrc` stays: the remaining Node scripts still run on Node 24.

### 8. Native worker (step c)

| Path | Decision | Why |
| --- | --- | --- |
| `native/rollback-boundary.cpp`, `native/rollback-windows.h` | KEEP | The mission keeps the C++ worker source. `crates/noct-native` documents itself as a port of these files. |

### 9. Rust crates (step c, and the freeze)

| Path | Decision | Why |
| --- | --- | --- |
| `crates/noct-core/tests/constants_parity.rs`, `crates/noct-core/tests/ipc_contracts.rs`, `crates/noct-db/tests/observer.rs` | PORT (read the frozen copies) | See the table above. |
| `crates/noct-db/tests/row_gate.rs`, `crates/noct-db/tests/rust_written_gate.rs`, `crates/noct-app/tests/output_differential.rs` | APAGAR | Ignored TypeScript gates. |
| `crates/noct-db/src/migrations/generated.rs` | KEEP, the header changes | It is Rust source now. Its header says "GENERATED by scripts/transpile-migrations.mjs"; that script is removed, so the header is rewritten to say the file is the Rust source of record. |
| `crates/noct-db/tests/fixtures/schema-hashes.json` | KEEP, the header changes | Fixture of `migration_parity.rs`. Its `generatedBy` names a removed script. |
| `crates/noct-core/tests/fixtures/*.json` (`ipc/`, `log-redaction.json`) | KEEP | Fixtures of the Rust contracts. Their `source` fields name TypeScript files only as provenance text. |
| Everything else under `crates/` | KEEP | Rust product. |

### 10. Documentation and assets (step e and step f)

| Path | Decision | Why |
| --- | --- | --- |
| `README.md`, `README.pt-BR.md` | ADJUST (step e) | Describe the real state, without Electron sentences. |
| `CONTRIBUTING.md`, `CONTRIBUTING.pt-BR.md` | ADJUST (step e) | They give `npm test` and Electron commands. Included with the READMEs: the same commands. |
| `docs/status.md` | ADJUST (step f) | Report of this removal. |
| `docs/rust-shell.md`, `docs/rust-row-gate-workflow.md`, `docs/ipc-inventory.md` | ADJUST (step f) | Name removed commands. A note is added that they are historical. |
| all other `docs/` (110 files, including ADRs) | KEEP | Decisions and history. Not rewritten. |
| `AGENTS.md` | KEEP, flagged | It still describes the Electron invariants. The owner decides whether to rewrite it. Not changed here. |
| `CHANGELOG.md`, `SECURITY.md` and `.pt-BR`, `CODE_OF_CONDUCT.md` and `.pt-BR`, `DESIGN.md`, `PRODUCT.md`, `IDENTITY-MIGRATION.md`, `.impeccable/` | KEEP | Project documents. |
| `assets/` (logo) | KEEP | Brand assets. Rust does not reference them, but they are the product's identity. |
| `crates/noct-ui/shell.css`, `crates/noct-ui/*.rs`, `crates/noct-ui/index.html` | KEEP | The Rust UI sources and assets. |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `.editorconfig`, `.gitignore`, `.nvmrc` | KEEP | Rust toolchain and policy. `.gitignore` keeps its Electron lines; they match nothing after the removal. |

## Order of steps and what each one checks

Each step is one commit tagged `[removal]`. Before the first change that touches `.mjs`, `.ts` or `.js` files, `npm run
typecheck` and `npm run lint` are run and their output is recorded (the sync rule of this repository). After each step:
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`,
`trunk build --release` (in `crates/noct-ui`) and `node scripts/check-shell-ui.mjs`. Output is kept in the scratch folder.

1. Inventory (this document).
2. Freeze: copy the TypeScript reference files into test fixtures, extract the observer names, and point the three Rust tests to them.
3. (a) Delete the renderer, `electron/`, and the Electron build configuration (`index.html`, `public/`, `vite`, `electron-builder`, untracked `dist*`).
4. (b) Delete `shared/`, the TypeScript tests and their configuration, and port the three release tests to `node:test`.
5. (c) Delete the Electron workflows and the scripts only they use; delete the three ignored TypeScript gates; adjust `rust-shell-validation.yml`, `dependency-security.yml` and the generated header.
6. (d) Reduce `package.json` and the lockfile; `npm ci` must install the reduced set.
7. (e) READMEs and CONTRIBUTING.
8. (f) `docs/status.md`, and the notes on removed commands in the three docs.

Pushes go in batches of three or four commits, and the CI result of the pushed head is read per workflow and per system.

## What is lost, stated plainly

- The parity gates against the TypeScript repositories (row gate, Rust-written gate, output differential) and their
  release-line bases. They stay runnable at `electron-final`, not on this branch.
- The C++ worker is no longer compiled or tested in CI. Its source stays.
- The React renderer's visual regression (Playwright screenshots).
- The Electron packaging, the packaged smoke, the updater rehearsal and the Electron stable release.
- The `--release-line` option of `run-rust-shell.mjs`.
- The design-system CSS check of the React renderer.

## Depends on the owner

1. **PR #23 must not be merged into `main` as it is.** Merging `migration/rust` into `main` would remove the Electron
   product from `main`, which contradicts "main continues Electron". No action on the PR is taken here.
2. **CodeQL** language set in GitHub settings (`javascript-typescript`), which is outside the repository files.
3. **`AGENTS.md`** still describes the Electron invariants.
4. Whether the parity gates should be re-created on a separate branch or tag, as Rust-only tests against frozen data.
5. Acceptance of the losses above.
