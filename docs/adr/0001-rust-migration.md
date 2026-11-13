# ADR 0001: Migrate NOCT to Rust (Tauri 2 + Leptos)

- **Status:** Proposed. Direction approved by the maintainer; the decisions in
  [Open decisions](#open-decisions) still need review before phase 2 lands on the
  migration branch.
- **Branch:** `migration/rust`. `main` stays on Electron and TypeScript until the
  Rust build passes the same parity gates (see [Phases](#phases)).
- **Supersedes:** the stack list in `AGENTS.md` (Electron, React, TypeScript,
  Vite, better-sqlite3, Zustand, Zod, Vitest, Playwright, electron-builder, npm)
  once accepted. Until then the two documents conflict, and the conflict is
  recorded here rather than resolved silently.

## Context

The current application is about 44,400 lines of TypeScript and TSX, of which
13,500 are tests, plus 7,200 lines of CSS:

| Layer | Lines | Role |
| --- | ---: | --- |
| `electron/` | 19,242 | Main process: SQLite, IPC, AI providers, change control, indexes, updates |
| `tests/` | 13,492 | 95 Vitest files, Playwright renderer tests |
| `src/` | 6,622 | React 19 renderer, 28 TSX files |
| `shared/` | 3,100 | Contracts and limits, Zod schemas |
| `scripts/` | 1,990 | Build, smoke, release and verification tooling |
| `src/**/*.css` | 7,227 | Nocturne design system (reusable as-is in a webview) |

The database is at schema version 32 (`DATABASE_SCHEMA_VERSION`). Existing user
data must open without loss.

A spike (see [Evidence](#evidence)) ported the rollback boundary worker from C++ to
Rust. It passed the existing Linux rollback tests and matched the C++ worker's
protocol responses and final filesystem state in 35 of 36 scenarios. The one
divergence was an intentional hardening.

## Decision

Application code moves to Rust:

| Current | Rust target | Notes |
| --- | --- | --- |
| Electron main process | Tauri 2 core (`crates/noct-app`) | Privileged operations stay in the Rust core |
| preload + `contextBridge` + IPC | Tauri commands + capabilities | Deny by default, granted per window |
| React 19 renderer | Leptos 0.8 (CSR, WASM, built with trunk) | Fine-grained signals isolate hot state |
| Zustand | Leptos signals | Streaming deltas do not update the whole app |
| Zod schemas | `serde` types + explicit validation in `noct-core` | Wire format must stay identical |
| better-sqlite3 | `rusqlite` (bundled SQLite) | Same schema, WAL and `synchronous=FULL` |
| chokidar | `notify` | Single watcher, as today |
| electron-updater | `tauri-plugin-updater` | Signed artifacts |
| electron-builder | Tauri bundler | Per-platform packages |
| Vitest | `cargo test` + ported test suites | Parity gates per phase |
| Playwright renderer tests | Tauri WebDriver or Playwright against the WASM build | Still to decide |
| `native/rollback-boundary.cpp` | `crates/noct-native` (`rollback-boundary` binary) | Stays a separate process |

Rationale:

- **Tauri over Dioxus desktop.** Tauri keeps a separate webview process and a
  privileged Rust core. That is the boundary `AGENTS.md` requires: the renderer
  never reaches privileged APIs directly. Dioxus desktop runs the UI and the
  core in one process and would remove that boundary.
- **Leptos over Dioxus or Yew.** Leptos signals update only the nodes that
  depend on a changed value. That matches the performance rules in `AGENTS.md`
  (no application-wide updates for streaming deltas). Dioxus is closer to the
  React mental model and is the main alternative.
- **Leptos 0.8, not 0.9.** Version 0.9 is still a release candidate.
- **Rollback worker stays a separate binary.** Its capability is the most
  sensitive one in the product. Moving it in-process would widen what a compromised
  UI could reach.
- **Reuse the CSS.** The Nocturne stylesheets (7,227 lines) move unchanged into
  the webview. The React components are the part that gets rewritten.

## Security invariants

`AGENTS.md` lists the Electron invariants. Each one maps to a Tauri mechanism or
is a gap that must be closed before phase 6 (IPC and shell) is accepted.

| Invariant | Tauri mapping | Status |
| --- | --- | --- |
| `contextIsolation` on | Not applicable. Commands are the only bridge | Set by design |
| `nodeIntegration` off | No Node runtime in the webview | Set by design |
| Renderer never touches privileged APIs | `withGlobalTauri: false`; commands granted by capability | Configured in phase 0 |
| IPC payloads and origins validated | Command arguments validated with `noct-core` types; capability limits windows | Phase 6 |
| Browser permissions denied by default | Webview grants nothing beyond `core:default` | Configured in phase 0; verify per platform |
| External navigation restricted | `on_navigation` handler in the core | **Gap**: implement in phase 6 |
| Content Security Policy | `app.security.csp` in `tauri.conf.json` | Configured in phase 0 |
| Credentials never reach renderer | Secrets held in the OS credential store through `keyring-core` (`crates/noct-secrets`); SQLite and the renderer see opaque references only | Phase 5. Vault done; storage choice pending confirmation (see [Open decisions](#open-decisions)) |
| Review Mode read-only | Logic port, unchanged | Phase 4 |
| Restored workspaces unauthorized | Logic port, unchanged | Phase 4 |
| Electron production fuses and ASAR integrity | **No equivalent.** Replaced by code signing and signed updater artifacts | **Gap**: needs an explicit decision |

## Data compatibility

These are not negotiable during the migration:

- The SQLite schema at version 32 opens unchanged. Migrations stay transactional,
  and WAL with `synchronous=FULL` is preserved.
- Backup envelopes keep their version and checksum format.
- Workspace context files keep atomic writes.
- Provider credentials are **not** migrated. Electron `safeStorage` ciphertext is
  bound to the Electron application and cannot be read by Rust without
  reimplementing that decryption, which is fragile. Proposed: users re-enter their
  provider keys once after the migration. Pending confirmation (see
  [Open decisions](#open-decisions)).
- **User data location.** Electron derives its data directory from the product
  name and `appId`. Tauri derives its own. The identifier is kept as
  `com.nocturne.codex` so the updater identity continues, but the data directory
  will still differ. Phase 2 must include a migration step that reads the existing
  directory, and it needs a test.

Gate for phase 2: fixture databases created by the TypeScript application at
schema 32 must open in Rust and return identical rows.

## Phases

Each phase ends with a parity gate: the matching TypeScript tests must pass
against the Rust code, or the difference must be recorded as an intentional change.

| Phase | Scope (TypeScript lines) | Gate |
| --- | --- | --- |
| 0. Foundation | Workspace; `noct-native` (Linux); `noct-core` limits and contracts; Tauri shell | **Done on branch**: see [Evidence](#evidence) |
| 1. Contracts | `shared/` (3,100) to `noct-core` | Contract tests; JSON fixtures produced by the TypeScript code |
| 2. Persistence | `electron/database` + `persistence` (about 5,000) to `noct-db` | Database, migration, durability and recovery tests; real schema-32 fixtures. **In progress**: see [Progress](#progress) |
| 3. Native rollback | macOS (APFS clone, `renameatx_np`) and Windows (232-line header) backends | `native-rollback-validation.yml` on Linux, macOS and Windows |
| 4. Workspace services | workspaces, change control, project index, semantic index, validation, engineering (about 6,000) | Their Vitest suites, ported |
| 5. AI | `electron/ai` and `security` (about 4,100) | Provider adapter, tool loop and vault tests |
| 6. IPC and shell | `electron/ipc`, `main.ts`, `runtime` (about 3,600) to Tauri commands and setup | Boundary tests; navigation and permission checks |
| 7. UI | `src/` (6,622 TSX) to Leptos, reusing the CSS | Renderer tests and canonical screenshots |
| 8. Updates and packaging | `electron/updates`, `electron-builder`, release workflows, smoke scripts | Signed packages validated on three platforms |
| 9. Retirement | Remove `electron/`, TypeScript, Vite, npm, Node requirements | All phases green; `AGENTS.md`, `docs/` and `CONTRIBUTING.md` updated |

Phases 1 to 4 can overlap once phase 2 lands. Phase 7 cannot start before phase
6, because the UI calls the commands that phase 6 defines.

### Progress

Recorded on branch `migration/rust`. Each item names the commit that delivers it.

- **Done, phase 2 (persistence):**
  - Runtime: open order, pragmas, schema-newer refusal, pre-migration backup with
    retention, transactional migrations, 7-day integrity check, orphan cleanup,
    recovery snapshots, close (`85f538f`).
  - Migrations 1 to 32, generated from `electron/database/migrations.ts`. Sixty-three
    schema fingerprints (fresh, every legacy state, every upgrade path) match the
    TypeScript output (`85f538f`).
  - Settings, workspace history without `relocate`, conversations and messages,
    with transaction nesting as savepoints (`abbc412`).
- **Done, phase 5 (credential vault, `crates/noct-secrets`):** provider secrets go
  to the operating system credential store (Secret Service on Linux, Keychain on
  macOS, Credential Manager on Windows). A non-secret index of UUID references,
  written atomically with mode 0600, lets unused secrets be pruned. There is no
  plaintext or application-encrypted fallback. Tests cover the index contract,
  rollback after a failed index write, a locked store, length rules counted in
  UTF-16 units, and symlink refusal. The OS round-trip is an ignored test
  (`cargo test -p noct-secrets --test os_store -- --ignored`). On this machine it
  reached KWallet's Secret Service in an isolated session and stopped at a locked
  wallet, which the vault reports as unavailable. A full round-trip needs an
  unlocked collection. The macOS and Windows backends are compiled only for their
  targets and are not verified here.
- **Done, phase 2 (repositories, continued):**
  - Model descriptors and workspace bindings contracts, with Zod-identical
    serialization of numbers and key order (`5941b7f`).
  - Model catalog, workspace model bindings and `WorkspaceRepository.relocate`, in
    one transaction (`e1082a1`).
  - Artifacts, approvals and workspace memory (`33728e7`).
  - Provider configurations. The table holds only a UUID credential reference; the
    secret stays in the operating system store.
  - Startup reconciliation of interrupted operations, with its interrupted-operation
    contract in `noct-core` (this commit).
- **Row gate:** `scripts/verify-row-gate.mjs` has the TypeScript repositories write a
  schema-32 database in a temporary directory and record the rows they return. Rust
  must return the same rows for model catalog, bindings, settings, workspaces,
  conversations, messages, artifacts, workspace memory and provider configurations.
  Approvals are not in the gate yet. Reconciliation is checked differently: the
  TypeScript repository reconciles a copy of a database that has interrupted rows, and
  Rust must reach the same state from the copy saved before it ran.
- **Remaining, phase 2:** checkpoints, executions and evidence, change sets, validation,
  suggestions, brain memories, project index, semantic index, engineering intelligence,
  backup and import, recovery, and the `Database` facade.
  Upgrade fixtures written by earlier application releases are also still needed.
- **Not started:** phases 3 to 9.

The row gate now reads rows that the TypeScript repositories wrote at schema 32. It
does not yet read databases from earlier releases, which the migration fingerprints
cover only for their schema. Phase 2 stays open until those are covered.

## Open decisions

1. **Signing and updater keys.** Who holds the signing keys, and how are the
   Electron fuses and ASAR integrity replaced? The repository already ships
   `docs/release-signing-public.asc`.
2. **UI framework.** Leptos (recommended here) or Dioxus. Changing this after
   phase 4 is expensive.
3. **Renderer test harness.** Tauri WebDriver or Playwright against the WASM build.
4. **Minimum platform versions.** WebKitGTK on Linux (the spike ran on 2.54), WebView2
   on Windows, WKWebView on macOS. `docs/compatibility.md` must state them.
6. **Provider key storage.** Proposed: secrets live only in the operating system
   credential store, with no plaintext or application-encrypted fallback. The
   alternative is an application-encrypted file, which needs a master key the
   app would have to store itself and so adds little protection. Confirm this, and
   confirm that existing provider keys are re-entered after the migration.
7. **Dependency review.** Each new crate needs the justification `AGENTS.md`
   requires. `libc` is required because `std` does not expose `openat2`,
   `renameat2` or `O_TMPFILE`. Add `cargo audit` to `dependency-security.yml`. The vault adds `keyring-core`,
   `secrecy`, and one platform store per target (`zbus-secret-service-keyring-store`,
   `apple-native-keyring-store`, `windows-native-keyring-store`). The standard
   library has no access to Secret Service, Keychain or Credential Manager, so
   these are needed. Each store is passed to the vault explicitly; none is set as
   the process-wide default.

## Consequences

- The webview engine changes. Electron uses Chromium. Tauri uses WebKitGTK on
  Linux, WebView2 on Windows and WKWebView on macOS. CSS, fonts and layout can
  differ between them, and the current renderer tests run only on Chromium. Visual
  checks must run on all three platforms.
- The Electron security model is re-established in Tauri's model. It is not
  carried over automatically. Each invariant in the table needs a test.
- Until phase 9, the repository holds two implementations. The TypeScript code is
  not changed by the migration except where a parity test requires it.
- The TypeScript test suite is the parity specification. It is ported, not
  deleted, until the Rust suite covers the same behavior.

## Evidence

Phase 0 results on Linux x86_64, WebKitGTK 2.54.2, rustc 1.97.1:

- `crates/noct-native` (rollback worker): the existing `nativeRollbackBoundary`
  and `buildRollbackBoundary` suites pass with the Rust binary, 16 passed and 9
  skipped. The skipped tests are Windows- and macOS-only, the same as with the C++
  worker. A 36-scenario differential harness matched the C++ worker in 35 scenarios,
  including the final filesystem state. The divergence is `OPEN parent/`: C++
  returned `DATA 0` for the empty leaf, and Rust rejects it with
  `CONFLICT Invalid relative path`.
- `crates/noct-core`: 8 tests pass. They cover the JSON wire shape of the first
  contracts and a parity guard. The guard reads `shared/constants.ts` and fails on
  any drift. A mutation test (changing one limit) confirmed that it fails.
- `crates/noct-app`: the Tauri 2.12 shell builds and starts. On this machine
  (Wayland session with XWayland) the process stays alive for 8 seconds, with no
  error output. Visual verification is not done: the window could not be captured
  in isolation, and the page is a placeholder anyway. WebKitGTK printed
  `Failed to create GBM buffer` warnings, which must be checked on other GPU
  drivers before release.

## What this ADR does not cover

- The database migration of existing user data (phase 2).
- The macOS and Windows native backends (phase 3).
- Any change to the TypeScript application's behavior.
