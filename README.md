# NOCT: state of the Rust migration

[Português do Brasil](README.pt-BR.md)

> This is the branch `migration/rust`. It holds the work that moves NOCT's desktop shell and local persistence
> from Electron and TypeScript to Rust. It is not a release. The stable product is the Electron application on
> `main` (version 1.0.5), and its README is on `main`.

This file describes the branch as it was checked on 2026-11-17, against the code of commit `de7dba4`. The commits
after it change only documentation. Each statement names what was checked. Anything that was not checked is listed
as not checked.

## 1. What NOCT is and what the migration does

NOCT is a local desktop engineering workspace. It keeps the state of a developer's project and coordinates the
developer with AI providers that the user selects. The developer keeps the approval and the judgement. The
application records plans, approvals and validation results (`AGENTS.md`, "Product identity").

The migration replaces three parts of the Electron application:

- the Electron main process and IPC, with a Tauri 2 shell in Rust (`noct-app`);
- the TypeScript persistence layer, with a SQLite layer in Rust (`noct-db`) that opens the same schema, version 32;
- the React renderer, with a Leptos interface compiled to WebAssembly (`noct-ui`), which reuses the CSS of the
  Electron interface.

The provider credentials move to a Rust vault (`noct-secrets`) that keeps the secrets in the operating system
credential store. The architecture decisions are in [ADR 0001](docs/adr/0001-rust-migration.md) and
[ADR 0002](docs/adr/0002-envelope-provider-credentials.md).

What this branch does not do yet:

- it does not replace the Electron application on `main`;
- it produces no installer or signed package;
- it does not call any AI provider;
- it does not show conversations in the interface;
- no window has been checked on Windows or on macOS (section 3).

## 2. Current state

### Phases

The Status column is the status that ADR 0001 gives in its phase table (after commit `e86b24b`). The last column says
what the repository shows, where that differs from the ADR or needs a note. Section 10 lists the conflicts.

| Phase | Scope in the ADR | Status (ADR 0001) | What the repository shows |
| --- | --- | --- | --- |
| 0. Foundation | Workspace, `noct-native` (Linux), limits in `noct-core`, Tauri shell | **Done on branch** | The ADR's evidence section. The workspace builds on Linux, Windows and macOS in CI. |
| 1. Contracts | `shared/` to `noct-core` | Not reviewed in this update | Contract modules exist in `noct-core`, and a guard reads `shared/constants.ts`. The ADR records no completion. |
| 2. Persistence | `electron/database` and `persistence` to `noct-db` | In progress | Done: the runtime, migrations 1 to 32, the repositories, and the closure items (public open path, nine release-line fixtures, operation observer catalog). Open: the check on a copy of real data before any public release, and the observer has no call sites yet (phase 6). |
| 3. Native rollback | macOS (APFS clone) and Windows backends | Partly done, Linux only | `crates/noct-native/src/linux.rs` is the only backend. The Rust worker answers `UNSUPPORTED` on macOS and Windows. The missing parts are listed in [the backends document](docs/rust-native-rollback-backends.md). |
| 4. Workspace services | Indexer, change control, validation, engineering | Partly done | Repositories exist under phase 2 for workspaces, change sets, project index, semantic index and validation. The services, such as `ProjectIndexService`, are not ported. |
| 5. AI | `electron/ai`, `security` | Partly done | Done: the credential vault in `noct-secrets`. Its round trip with the operating system store passes in CI on three systems. Not ported: the provider adapters and the tool loop. The Rust crates contain no HTTP client for providers. |
| 6. IPC and shell | `electron/ipc`, `main.ts`, `runtime` to Tauri | Partly done | The first slice is done: five commands, granted only to the `main` window. Checked in a running window only by the manual settings check in section 3. |
| 7. UI | `src/` to Leptos | Partly done | The settings area, the project list in the sidebar, the glass layer and the themes. Checked in Chromium with a mocked shell. Not checked in a real window. |
| 8. Updates and packaging | `electron/updates`, `electron-builder`, release workflows | Not started on the Rust side | Blocked. See below. |
| 9. Retirement | Remove `electron/`, TypeScript, Vite, npm and the Node requirement | Not started | Depends on phase 8 and on the phases before it. |

Phase 0 is the only phase the ADR marks as done. In phases 2, 5, 6 and 7 the closure items or the first slices are done,
and the rest of each phase is open.

### What blocks phase 8

The ADR's criterion for phase 8 is "signed packages validated on three platforms". In the repository:

- **No packaging is configured.** `bundle.active` is `false` in `crates/noct-app/tauri.conf.json`.
- **No updater.** No updater crate is in `Cargo.lock`.
- **No signing configuration** exists in the Rust crates or in `tauri.conf.json`.
- **No holder for the signing keys.** ADR 0001, open decision 1, is not decided.
- **No equivalent of the Electron fuses and ASAR integrity.** The ADR records this as a gap that "needs an explicit
  decision".
- **The shell has not run in a window on Windows or macOS**, so the criterion "validated on three platforms" cannot be
  met yet.
- **The platform floors are decided: Windows 10 and later, macOS 10.15 and later, and Ubuntu 22.04 and later for Linux.**
  ADR 0001, decision 4, is resolved for them. No CI job runs on Ubuntu 22.04.
  [`docs/compatibility.md`](docs/compatibility.md#minimum-platform-versions) has the decision and the Tauri
  statements behind them. Tauri gives no WebKitGTK version, and its macOS pages give two different values.

## 3. Verification

The results below are for commit `de7dba4`, the code this README describes. This README adds no code. The checks on
this machine ran on Linux. Nothing was run on Windows or macOS by hand.

### What CI covers

A job's result is `success` when every step that ran passed. The Linux-only steps run inside the Ubuntu job, so the
table reads the job result for them.

| Check | Linux (`ubuntu-latest`) | Windows (`windows-latest`) | macOS (`macos-latest`) |
| --- | --- | --- | --- |
| Workspace builds, and `cargo test --workspace --no-fail-fast` passes (ignored tests excluded) | Passed, `de7dba4` | Passed, `de7dba4` | Passed, `de7dba4` |
| Release shell builds with the interface embedded (the job checks that the wasm file name is in the binary) | Passed, `de7dba4` | Passed, `de7dba4` | Passed, `de7dba4` |
| Operating system keyring round trip, `os_store` (latest run, on `1d89435`) | Passed | Passed | Passed |
| Permissions of the Tauri manifests match the committed files | Passed, `de7dba4` | not run | not run |
| Output differential against TypeScript, nine release lines | Passed, `de7dba4` | not run | not run |
| Chromium: Content-Security-Policy, and the settings and interface (14 scenarios, mocked shell) | Passed, `de7dba4` | not run | not run |
| Log redaction fixture and IPC contract samples, against TypeScript and zod | Passed, `de7dba4` | not run | not run |
| Launcher tests (`node --test scripts/run-rust-shell.test.mjs`) | Passed, `de7dba4` | not run | not run |
| Row gate: 10 tests against the databases of the TypeScript repositories | Passed, `de7dba4` (`row-gate.yml`) | not run | not run |
| Dependency policy (`cargo deny check`, `cargo audit`) | Passed, `1d89435` (`rust-audit.yml`, latest run) | not run | not run |

The keyring round trip and the dependency policy are separate workflows. Each runs only when its own paths change,
so its latest run is on an earlier commit.

The statistics tests added by `c0748e0` (`a_failed_analysis_after_a_large_round_is_logged_and_the_round_still_updates`
and `a_failed_statistics_step_at_close_still_runs_the_optimize_and_the_close_succeeds`) appear as `ok` in the logs of
the three shell jobs of `de7dba4`.

### Seen in a real window

| Platform | What was seen | Where it is recorded |
| --- | --- | --- |
| Linux, WebKitGTK | Dev mode and release mode were opened by hand. Both showed the settings area with its default values, and `settings_get` ran end to end. This was the interface before the current layout. | `docs/rust-shell.md`, "Manual check, Linux (WebKitGTK)" |
| Windows, WebView2 | Nothing. | none |
| macOS, WKWebView | Nothing. | none |

Not seen in any window, on any platform:

- the sidebar with real workspaces, and the current layout;
- the minimum window size as the window manager enforces it;
- the navigation policy in a running webview;
- the commands called from a running window (`settings_get`, `settings_set` for the theme, `workspaces_list`,
  `conversations_list` and `workspace_set_favorite`). The browser check uses a mocked shell and the Rust tests use
  Tauri's mock runtime;
- the cost of the glass layer, which was not measured;
- the warning for a data folder outside the user's profile, on a real Windows profile.

### What CI does not cover

- No job opens a window. A window is checked by hand.
- The Chromium checks, the output differential and the row gate run on Linux only.
- Windows applies no file ACL to the data folder (section 6).
- The native rollback workflow, `native-rollback-validation.yml`, runs on `main` and `stabilization/native-rollback-v1`.
  It does not run on this branch, so the Windows and macOS rollback behaviour is not tested here.

## 4. How to run

### Requirements

- Rust 1.97.1, which `rust-toolchain.toml` pins, with the `wasm32-unknown-unknown` target.
- trunk 0.22.1: `cargo install --locked trunk@0.22.1`.
- Node.js 24 (`>=24.18 <25`) and npm 11, with `npm ci` at the repository root. Node runs the launcher, the fixture
  option and the checks that compare with TypeScript.
- On Linux, the system packages that CI installs for Tauri: `build-essential`, `libssl-dev`, `libwebkit2gtk-4.1-dev`,
  `libgtk-3-dev`, `libxdo-dev`, `librsvg2-dev` and `libayatana-appindicator3-dev`.
- The fixture option needs the release tags in the clone, so use a full clone with its tags.

### Commands

The launcher is `scripts/run-rust-shell.mjs`. Its options are `--data-dir <absolute path>` or
`--from-fixture <release tag>`, which cannot be combined, and `--check`.

| Command | What it does | Run for this README |
| --- | --- | --- |
| `node scripts/run-rust-shell.mjs dev --check` | Prints the plan of the development mode and checks the tools. Starts nothing. | Yes. Passed. |
| `node scripts/run-rust-shell.mjs release --check` | The same plan for the release mode. | Yes. Passed. |
| `node scripts/run-rust-shell.mjs dev --data-dir <empty temporary folder> --check` | The same plan with the data folder given. | Yes. Passed. |
| `node scripts/run-rust-shell.mjs release --from-fixture v1.0.2 --check` | Copies the v1.0.2 release database to a new folder under `target/shell-fixtures/`, prints the path, and starts nothing. | Yes. Passed. The copy stays there until it is deleted. |
| `cargo build -p noct-app --release --features custom-protocol` | Builds the release shell with the interface embedded. | Yes. Passed. |
| `node scripts/run-rust-shell.mjs dev` | Starts `trunk serve` on port 1420 and runs `cargo run -p noct-app`. This opens the window. | Not run. The checks for this README do not open windows. The same mode was opened by hand on Linux (section 3). |
| `node scripts/run-rust-shell.mjs release` | Runs `trunk build --release` and the release build, then opens the window. | Not run, for the same reason. Its build step was run as shown above. |
| `node scripts/run-rust-shell.mjs dev\|release --data-dir <absolute path>` | Opens the shell on that folder, which it sets as `NOCT_DATA_DIR`. | Not run. It opens a window. |
| `node scripts/run-rust-shell.mjs dev\|release --from-fixture <tag>` | Opens the shell on a copy of a pinned release database. The tags are the nine in `scripts/fixtures/release-lines.json`. | Not run. It opens a window. The copy step ran with `--check`. |

> **Never point the shell at a data folder that holds real data.** Without `--data-dir`, the shell uses its default
> data folder, and that is where the data of a real user of the Rust shell is kept. For tests, use `--data-dir` with an
> empty folder that you create for the test and delete afterwards. Or use `--from-fixture`: it copies a database into
> `target/` and never opens the original. On Windows, a data folder outside the user's profile is written to the log as
> a warning, and the shell still opens it.

## 5. Architecture by crate

| Crate | Responsibility |
| --- | --- |
| `noct-app` | The Tauri 2 shell. It opens the data folder and the log, registers the five commands, sets the window policy, and checks workspace access. |
| `noct-core` | The contracts, limits and types that mirror `shared/`, and the log redaction. |
| `noct-db` | SQLite persistence: the runtime, the migrations 1 to 32, the repositories, backups and recovery snapshots, atomic writes, the file log and the operation observer. |
| `noct-native` | A separate rollback worker binary for filesystem operations. It has a Linux backend only. |
| `noct-secrets` | The provider credential vault: an encrypted document, a non-secret index, and a master key in the operating system credential store. The shell does not open it yet. |
| `noct-ui` | The Leptos interface, compiled to WebAssembly by Trunk. The Geist fonts are vendored under the SIL Open Font License 1.1. |

## 6. Security model

### In place

- **Webview policy.** `withGlobalTauri` is `false`. The Content-Security-Policy is
  `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost`.
  Chromium checks it (`scripts/check-shell-csp.mjs`). No webview has enforced it yet.
- **Commands.** Five commands, `settings_get`, `settings_set`, `workspaces_list`, `conversations_list` and
  `workspace_set_favorite`, are declared in
  `crates/noct-app/build.rs`. `crates/noct-app/capabilities/default.json` grants them to the `main` window only. Tests
  with Tauri's mock runtime refuse a command that is not granted and a request from a second window.
- **Navigation.** The main window accepts navigation only to the packaged interface, and to the development server in
  debug builds (`crates/noct-app/src/navigation.rs`, with its tests). This is not verified in a running webview.
- **Browser permissions.** The webview grants nothing beyond `core:default` (ADR 0001, security table). Not verified in a
  running webview.
- **Credentials.** The database holds only a UUID reference for each provider configuration. The vault encrypts the
  credentials with XChaCha20-Poly1305 and keeps the master key in the operating system credential store (ADR 0002). The
  shell does not open the vault, so no credential reaches the interface.
- **Logs.** Lines are redacted before they are queued for the file log (`crates/noct-core/src/log_redaction.rs`). The
  redaction fixture is compared with TypeScript on Linux in CI.
- **Local files.** On Unix, the database, its backups, the logs and the atomic writes are created with mode `0600`, and
  the folders with `0700`. On Windows, no ACL is applied (see the list below).
- **Restored workspaces.** A restored backup sets `authorized` to `0`. The workspace must be selected again before the
  shell trusts it (`crates/noct-db/src/repositories/backup.rs`).

### Still missing

- **Provider allowlist.** None is implemented. The repository has a design, [`docs/provider-allowlist-design.md`](docs/provider-allowlist-design.md),
  with six open questions and no code. It records a gap in the Electron product: a local-source endpoint accepts HTTPS to
  any host, including private addresses, and skips the resolved-address check that remote sources run. The Electron
  product's rules are these: remote Chat Completions providers must use HTTPS, reject redirects and check every resolved
  address before connecting; plain HTTP is allowed only for loopback services (`docs/security.md`). The Electron
  child-process environment has its own allowlist (`electron/runtime/childEnvironment.ts`). The Rust branch has no
  provider client, so none of these rules is ported. Which policy applies is a decision for the owner.
- **Windows ACL.** The data folder gets no DACL. Setting one needs `windows-sys` as a direct dependency, which the owner
  has not approved. The owner decided that it waits for the vault integration, with the owner present. The analysis is in
  `docs/rust-windows-file-permissions.md`.
- **Electron fuses and ASAR integrity.** Tauri has no equivalent. ADR 0001 marks this as a gap that needs a decision.
- **Workspace access check.** It approximates `access(R_OK | X_OK)` without a native call. Its tests do not cover ACLs,
  other users or group access (`docs/rust-shell.md`, "Known approximations").
- **npm dependencies.** `npm audit --omit=dev` reports no vulnerabilities. The full tree has 13 moderate findings, all
  from one advisory, GHSA-g3hv-75wg-m249 (`minimatch`), in development tooling. None is on the path of the Rust build.
  `npm audit fix` is not applied, because its output installs `electron-builder@25.1.0`, which it calls a breaking
  change. The options are in [`docs/npm-audit-triage.md`](docs/npm-audit-triage.md). None is decided.

## 7. Compatibility and rollback

- **Schema.** The schema version is 32 (`shared/constants.ts`, `crates/noct-core/src/limits.rs`), and a parity test
  checks the two. This branch adds no migration after 32. The last migration in
  `crates/noct-db/src/migrations/generated.rs` is 32, as it is in the TypeScript migrations.
- **Databases already at 32.** They open with no schema change. The row gate checks the rows that the Rust code reads
  and writes against the TypeScript repositories.
- **Older databases.** The nine pinned release lines run from 0.5.0-beta to 1.0.2. When the Rust runtime opens an older
  database, it writes a backup first, keeps up to three, and then runs the migrations inside transactions. The row gate
  and the output differential compare the result with TypeScript for each line.
- **Newer databases.** The runtime refuses a schema newer than 32 before it changes anything.
- **Rollback to the Electron application.** Both implementations use schema 32, so a file written by one should open in
  the other. This branch does not test it. The row gate reads files that TypeScript wrote, and compares Rust's writes
  with TypeScript's expected results. The other direction is checked as well: `scripts/verify-rust-written-gate.mjs`
  has Rust write a database, TypeScript read it and write to it, and Rust read it back, comparing the logical rows at
  each step. It runs with the row gate on CI. It covers workspaces, settings and conversations, not the release lines,
  the semantic index or the engineering tables. TypeScript opens write to the database as well (`PRAGMA optimize` and
  WAL checkpoints), so the comparison uses logical rows, not file bytes.
- **Rollback on macOS and Windows.** The Rust worker answers `UNSUPPORTED` on those platforms. The C++ worker that the
  Electron product uses has backends for both. The missing parts are listed in
  [the backends document](docs/rust-native-rollback-backends.md).
- **Provider credentials are not migrated.** ADR 0001, decision 6, proposes that users enter their provider keys again
  once, after the migration. That is pending confirmation.
- **Not the same rollback.** `noct-native` rolls back filesystem operations. It does not roll back the database.

## 8. Tests and CI

### Run for this README (Linux, commit `de7dba4`)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo clippy -p noct-ui --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `cargo test --workspace --no-fail-fast` | 456 passed, 0 failed, 19 ignored |
| `cargo tree -p noct-app -e normal -i rusqlite -f '{p} features={f}'`, and `cargo tree --workspace -e normal -f '{p} {f}' \| grep -c hooks` | `rusqlite` 0.42.0 without `hooks`. The grep counts 0. The `hooks` feature is a dev-dependency of `noct-db` (commit `c0748e0`), so it is not in the release graph. |
| `node --test scripts/run-rust-shell.test.mjs` | 7 passed |
| `node scripts/verify-row-gate.mjs` | 10 passed, then `scripts/verify-rust-written-gate.mjs` passed, exit 0 |
| `node scripts/generate-output-differential.mjs`, then `cargo test -p noct-app --test output_differential -- --ignored` | 1 passed |
| `trunk build --release` in `crates/noct-ui` | Passed |
| `cargo build -p noct-app --release --features custom-protocol` | Passed. The binary embeds the wasm file. |
| `node scripts/check-shell-csp.mjs` | Passed |
| `node scripts/check-shell-ui.mjs` | 14 of 14 scenarios passed |
| `node scripts/generate-log-redaction-fixture.mjs --check` | Passed |
| `node scripts/generate-ipc-samples.mjs --check` | Passed |
| `cargo deny check` | Passed (advisories, bans, licenses, sources) |
| `cargo audit` | No vulnerability. Six warnings, all listed with a reason in `deny.toml`: five unmaintained crates and one unsound crate, `glib` (RUSTSEC-2024-0429) |
| `npm ci` | Installed |
| `npm audit --omit=dev` | `found 0 vulnerabilities`. The full tree has 13 moderate findings. See [the triage](docs/npm-audit-triage.md). |

Not run here, with the reason:

- `cargo test -p noct-secrets --test os_store -- --ignored --nocapture`. It writes a test entry to the operating system
  credential store of the machine where it runs. It uses its own service name and does not touch the master key of an
  installed application (ADR 0002). CI runs it on each system instead.
- The semantic index benchmarks and the semantic statistics measurement, which are ignored tests (listed below). They are
  measurements, with no budget set, and they are not part of any gate.

### Ignored tests

| Test | Count | How it runs | Why it is ignored | Run here? |
| --- | --- | --- | --- | --- |
| `crates/noct-db/tests/row_gate.rs` | 10 | `node scripts/verify-row-gate.mjs` | It needs `NOCT_ROW_GATE_DIR`, which the script sets | Yes, 10 passed |
| `crates/noct-db/tests/rust_written_gate.rs` | 2 | `node scripts/verify-rust-written-gate.mjs`, which `verify-row-gate.mjs` also runs | It needs `NOCT_RUST_WRITTEN_DIR`, which the script sets | Yes, passed |
| `crates/noct-app/tests/output_differential.rs` | 1 | `node scripts/generate-output-differential.mjs`, then the test with `--ignored` | It needs the release-line bases | Yes, passed |
| `crates/noct-secrets/tests/os_store.rs` | 1 | `cargo test -p noct-secrets --test os_store -- --ignored --nocapture` | It needs the operating system credential store | No, see above |
| Benchmarks in `crates/noct-db/src/repositories/semantic_index.rs` | 4 | Release mode, `--ignored --nocapture` | They are measurements | No |
| `crates/noct-db/tests/semantic_statistics.rs` | 1 | Release mode, `--ignored --nocapture` | It is a measurement | No |

### Workflows on `migration/rust`

| Workflow | Runs on pushes to `migration/rust` | Runners | What it runs |
| --- | --- | --- | --- |
| `rust-shell-validation.yml` | Yes, and manual dispatch. No pull request trigger. | `ubuntu-latest`, `windows-latest`, `macos-latest` | Build, workspace tests, release build with the interface embedded. The Linux-only steps are listed below. |
| `row-gate.yml` | Yes. Also on pull requests to `main`, and manual dispatch. | `ubuntu-latest` | The row gate. It is not a required check: making it required is a repository setting. |
| `rust-audit.yml` | Yes, when its own paths change. Also on pull requests, a schedule and manual dispatch. | `ubuntu-24.04` | `cargo deny check` and `cargo audit`. |
| `keyring-round-trip.yml` | Yes, when `crates/noct-secrets/**`, `Cargo.toml`, `Cargo.lock` or the workflow change. | `ubuntu-24.04`, `windows-latest`, `macos-latest` | The unit tests, and the `os_store` round trip with the real store on each system. |

The other workflows do not run on a push to this branch. `package-validation.yml` (named "CI · source, renderer and
packages") and `dependency-security.yml` run on pushes to `main`, on pull requests and on manual dispatch.
`native-rollback-validation.yml` runs on pushes to `main` and `stabilization/native-rollback-v1`. `stable-release.yml`
runs on version tags. `updater-rehearsal.yml` and `packaged-recovery.yml` run on manual dispatch only.

### Steps that run only on Linux

In `rust-shell-validation.yml`, these steps have `if: matrix.os == 'ubuntu-latest'`:

- the Linux packages for Tauri, Node 24, `npm ci`, and the release-line bases and TypeScript outputs for the differential;
- the regeneration of the Tauri permissions, which fails on any difference from the committed files;
- the output differential against TypeScript;
- the Chromium installation, and the Chromium checks of the Content-Security-Policy and of the interface;
- the log redaction fixture and the IPC contract samples;
- the launcher tests.

The Windows and macOS jobs run the build, the workspace tests and the release build with the interface embedded.

## 9. Code signing, and what it means for installers

- This branch has no code signing: no certificate, no notarization and no signed updater artifact.
- `bundle.active` is `false`, so this branch produces no installer.
- A binary built from this branch is unsigned. Windows and macOS have no publisher identity to check for it. The only way
  to check where it came from is to build it from the commit yourself.
- The Electron product on `main` has its own policy. Its README states that Linux checksums are GPG-signed and that
  Windows and macOS packages are published without platform signing or notarization. That policy does not apply to this
  branch yet, and the choice of who holds the signing keys is open (ADR 0001, open decision 1).

## 10. Documentation index and next steps

### Documents for the migration

- [ADR 0001: Rust migration](docs/adr/0001-rust-migration.md). Phases, progress, security invariants and open decisions.
  Its status is "Proposed".
- [ADR 0002: envelope encryption of provider credentials](docs/adr/0002-envelope-provider-credentials.md). Accepted.
  It partly supersedes decision 6 of ADR 0001.
- [Running the Rust shell locally](docs/rust-shell.md). The run modes, what is verified, the manual check, the gaps in CI,
  and the known approximations.
- [Minimum platform versions](docs/compatibility.md#minimum-platform-versions). The decided floors, the Tauri pages they
  cite, and what is not confirmed.
- [Row gate in CI](docs/rust-row-gate-workflow.md). The workflow, its decisions, the test in which TypeScript reads and
  writes what Rust wrote, and what is not verified.
- [Rust rollback backends](docs/rust-native-rollback-backends.md). What the macOS and Windows backends must match, and
  what is missing.
- [Windows file permissions](docs/rust-windows-file-permissions.md). The analysis, the warning, and the ACL that is not
  implemented.
- [Provider allowlist design](docs/provider-allowlist-design.md). A proposal, with open questions. No code.
- [npm audit triage](docs/npm-audit-triage.md). The findings, their scope, and the options. Nothing is fixed.
- [Documentation index](docs/README.md). The other documents. The user guides in it describe the Electron product. For
  example, `docs/security.md` describes Electron IPC.
- [`AGENTS.md`](AGENTS.md). Contributor rules and the validation commands.

### Next steps

1. **Owner.** Decide the provider allowlist, using the design in `docs/provider-allowlist-design.md`, or confirm the
   Electron provider policy as the rule for Rust (section 6).
2. **Owner, with the owner present.** Decide on `windows-sys` or on declarations in the repository, and on the Windows
   ACL, during the vault integration (section 6 and `docs/rust-native-rollback-backends.md`).
3. **Real windows.** Open the shell on Windows (WebView2) and macOS (WKWebView). On Linux, check the current layout and
   the sidebar with real workspaces, using `--from-fixture`, and check the minimum window size.
4. **Phase 2.** Connect the operation observer to the phase 6 call sites. Before any public release, run the TypeScript and
   Rust comparison on a copy of real data, kept under `target/` and never committed.
5. **Phase 3.** Write the macOS (APFS clone) and Windows rollback backends, or decide that the C++ worker keeps those
   platforms. Correct the comment about `#else` in `crates/noct-native/src/main.rs`. The details are in
   `docs/rust-native-rollback-backends.md`.
6. **Phase 5.** Port the provider adapters and the tool loop.
7. **Phase 8 decisions.** Who holds the signing keys. How the Electron fuses and ASAR integrity are replaced. The minimum
   platform versions are decided in `docs/compatibility.md`; the macOS bundle setting is still to be written.
8. **Repository setting, owner.** Decide whether the row gate becomes a required check on `main`. It has passed on this
   branch.
9. **Owner.** Decide the npm audit option (`docs/npm-audit-triage.md`).

### Conflicts between the sources

- ADR 0001 used to say "not started: phases 3 to 9" and had no status column. Commit `e86b24b` added the status column,
  and this README uses it. ADR 0001 gives phase 1 the status "Not reviewed in this update", and this README does not
  re-review it.
- ADR 0001 used to list "CI wiring for the gate" as remaining in phase 2. The ADR now says it is done in `row-gate.yml`,
  although the job is not required.
- ADR 0001 is "Proposed", while ADR 0002 is "Accepted" and partly supersedes it.
- ADR 0001, decision 4, asked for the minimum platform versions in `docs/compatibility.md`. The file has the decided
  values and their sources, and the ADR text now records the decision too (Windows 10 and later, macOS 10.15 and later,
  Ubuntu 22.04 and later). The WebKitGTK version on Ubuntu 22.04 and the macOS setting in the bundle are still open.
- No provider allowlist is implemented. `docs/provider-allowlist-design.md` is a design only (section 6).
- ADR 0002 now also records statistics and schema-version text (commit `c0748e0`) that does not concern credentials. The
  description of the vault is unchanged.
