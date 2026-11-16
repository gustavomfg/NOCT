# ADR 0001: Migrate NOCT to Rust (Tauri 2 + Leptos)

- **Status:** Proposed. **Partly superseded by
  [ADR 0002](0002-envelope-provider-credentials.md):** decision 6 (provider key storage)
  and the storage design of the phase 5 vault. Direction approved by the maintainer; the
  decisions in [Open decisions](#open-decisions) still need review before phase 2 lands
  on the migration branch.
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
| Credentials never reach renderer | Secrets held in the OS credential store through `keyring-core` (`crates/noct-secrets`); SQLite and the renderer see opaque references only | Phase 5. Vault done; storage choice pending confirmation (see [Open decisions](#open-decisions)). **Superseded in part by [ADR 0002](0002-envelope-provider-credentials.md):** one master key in the store, secrets in an encrypted file |
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
- **Done, phase 5 (credential vault, `crates/noct-secrets`):** *Partly superseded by
  [ADR 0002](0002-envelope-provider-credentials.md). The storage described here, one store
  entry per secret, is replaced by one master key in the store and an encrypted document.
  The index, the locked-store rules and the symlink refusal carry over; the write order
  changed.* Provider secrets go to the operating system credential store (Secret Service on Linux, Keychain on
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
    contract in `noct-core`.
  - Workspace state evidence: immutable manifests, stale propagation along references,
    verification and index references. Its contract is in `noct-core`, and the gate
    checks it against manifests the TypeScript repository wrote.
  - Executions and checkpoints, with the agent mode and change-control records in
    `noct-core`. A terminal execution outcome cannot change; a completed capture stores
    its files and its evidence as one unit.
  - Change sets, file changes, hunk annotations and review decisions. A decision
    updates the change set, the change and the execution, records its evidence and
    completes its operation as one unit (`790fb15`).
  - Validation runs and execution evidence (commands, errors, validation links). A run
    records its evidence with the latest index manifests and the ready checkpoints of
    its execution, as one unit. Command arguments are kept to 100 entries of 4,096 units
    and messages to 500,000 units, as in the TypeScript repository. Rust refuses a stored
    run or command whose kind or status it does not know, where TypeScript returns it
    unchecked (`12c92d9`).
  - Review suggestions: the stored shape, the identity that matches a suggestion across
    reviews, and the repository (add, status changes with their history, reconciliation).
    The identity needs Unicode normalization and general categories, so `noct-core` uses
    `icu_normalizer` with its compiled tables only, and `icu_properties`. Tauri's URL stack
    already compiles both, and the lockfile gains only the two dependency edges, no new
    package. Node's ICU and these tables may carry different Unicode
    versions; the gate compares the identity for the hard cases (compatibility forms, final
    sigma, dotted capital I, controls, emoji). A review is also compared with the TypeScript
    repository on a copy of the seeded database. Reading agent output into suggestions and
    the structured-review instructions belong to the AI layer, which is not ported yet
    (`431791e`).
  - Brain memories: the lifecycle (candidate, active, outdated, archived), the workspace
    and conversation scopes, the equivalence of agent candidates, search by relevance,
    retrieval for a conversation, usage counts, and a history entry for every change. The
    credential filter keeps JavaScript's semantics: `\s` is JavaScript whitespace, `\S{n}`
    counts UTF-16 units, and `/i` folds ASCII letters only. `regex` already resolves in the
    lockfile, so `noct-core` adds one dependency edge and no package (`96e0792`).
  - Project index: files with their state and hashes, runs, symbols, imports, exports,
    exclusions, stack evidence and the summary. A file's analysis is replaced as one unit.
    A run that ends is recorded as project-index evidence in the same unit. The stack is
    named after the last segment of the workspace path, whichever separator it uses. The
    change scenario is replayed on a copy: the paths a change no longer finds are removed,
    and a stored path with a backslash is not taken for a path under a directory, as in
    TypeScript (`6bf7710`).
  - Semantic index: units with their embeddings, runs and the summary. A file's units are
    replaced as one unit of work, and a refused embedding rolls the whole replacement back.
    An embedding is stored as little-endian `f32`, and a stored vector whose length disagrees
    with its space fails the read. Ending a run records its units as semantic-index evidence
    in the same unit. Lexical search requires every token to match, and only indexed and
    lexical-only units are searchable. The run's index-version column is not part of the run
    contract, so the gate removes it on the TypeScript side.
  - Engineering intelligence: signals, insights and health snapshots, under the same rules as
    the TypeScript repository. Every value is checked before the transaction starts, and each
    row is checked again when it is read. An evaluation upserts its signals, resolves the
    active signals of the categories it assessed and of the explicit fingerprints it names, and
    stores its snapshot unless one with the same state exists. A new snapshot records its
    sources as engineering evidence in the same unit. A signal keeps its identity and first
    sighting, and an insight keeps the identity and creation time of its fingerprint. Rust
    reads a missing nullable key as null, where zod rejects it. The difference is recorded in
    `crates/noct-core/src/engineering_intelligence.rs` and matters only for JSON that the
    application did not write.
  - Backup: the export, its metrics and the import, under the rules of `BackupRepository.ts`.
    An import runs in one transaction that ends with the orphan cleanup. A full import replaces
    every collection and the settings, and it resets the authorization of restored workspaces;
    a project-data import keeps the settings. Text is limited to 10,000,000 UTF-16 units and may
    not contain NUL, and a refused value rolls the whole import back. The export hides
    `maintenance.*` settings, and a full import maps `openai-compatible` providers to
    `chat-completions`. The zod contract (`shared/ipc/backupSchemas.ts`) and the envelope limits
    (`shared/ipc/backupLimits.ts`) belong to the IPC layer and are not ported yet.
  - Recovery: the copies that can replace a damaged database, their integrity check and the
    restore. A restore moves `nocturne.db` and its sidecars to a quarantine directory, installs
    a copy that passes `quick_check` and moves the originals back when any step fails. Only
    intact copies with a supported schema are offered, newest first. The functions block on
    filesystem I/O and need the database closed, so the application runs them off the
    interactive path. A failed final rename is not tested, because the Rust tests cannot inject
    it; the other rollback steps are.
  - Persistence helpers: an atomic file write (a temporary file in the same directory, synced
    and renamed over the target, created with the mode the caller passes), a queue that serializes the writes
    that share a key, and the choice of the product's data directory after a rename. The queue
    keeps writes on one key in order, lets other keys run in parallel, and forgets a key once
    nothing is pending on it, even after a panic. The migration keeps the rules of the
    TypeScript version. Injected failures of the TypeScript tests (a denied rename, a disk that
    fills up) are not reproducible here; the Rust tests use real failures for the rollback.
    The suggestion decision log reads the workspace memory through the workspace read policy,
    so it is ported with the workspace services.
- **Row gate:** `scripts/verify-row-gate.mjs` has the TypeScript repositories write a
  schema-32 database in a temporary directory and record the rows they return. Rust
  must return the same rows for model catalog, bindings, settings, workspaces,
  conversations, messages, artifacts, workspace memory and provider configurations.
  Executions, checkpoints, change sets, hunks, validation runs, execution commands,
  errors, validation links, workspace evidence, review suggestions and brain memories are
  in the main gate. Suggestion identities are compared for the hard Unicode cases. Brain
  memories are read, paged, searched and retrieved with the same results, their error
  messages match, and the credential filter answers the same for 29 inputs covering the
  thresholds, UTF-16 counting, JavaScript whitespace and ASCII-only case folding. The
  TypeScript side fixes the update times of its memories, because the repository stamps the
  wall clock and the orders must not depend on the run. The project index is read through
  the same gate. JSON has one number type, so a whole REAL timestamp compares as an integer;
  the TypeScript position aliases on symbols, imports and exports are removed on the
  TypeScript side, as the `existsFlag` alias was. A review is
  reconciled on a copy by both sides, and the results are compared without identifiers
  or clock values. The TypeScript `listFiles` leaks an
  internal `existsFlag` alias; Rust does not reproduce it, and the gate removes it on the
  TypeScript side. It goes away with the TypeScript repository.
  Workspace evidence is checked by a third scenario, with its stale-clock values masked
  because each run records its own time. The semantic index is read through the main gate
  and its write scenario is replayed on a copy: two replacements, a refused replacement that
  must roll back, the removal of a file's units, stale marking, a deletion and the evidence
  of an ended run. The lexical rank is SQLite's bm25 value, and it keeps 12 significant
  digits on both sides, because better-sqlite3 ships SQLite 3.53.4 and the Rust build bundles
  3.54.1, which differ in the last bit. Engineering intelligence is read through the main
  gate, and its evaluation is replayed on a copy: an updated signal keeps its identity and
  first sighting, a category resolves only its missing signals, an explicit fingerprint
  resolves a signal the evaluation does not find, a repeated state returns the stored
  snapshot, a refused signal leaves nothing behind, and an insight keeps the identity of its
  fingerprint. Approvals are not in the gate yet.
  Backup is read through the main gate, with its export and metrics. Three imports run on
  copies through the TypeScript runtime: a full import, a project-data import that must keep
  the settings, and a refused import with a NUL character, which must leave the copy as it
  was. The export's clock is removed on both sides.
  Reconciliation is checked differently: the
  TypeScript repository reconciles a copy of a database that has interrupted rows, and
  Rust must reach the same state from the copy saved before it ran.
  - Facade: `noct_db::store::Store` is the `LocalDatabase` of the TypeScript application. Opening
    it records the operations that the last run left running, as the constructor does. It
    exposes every repository through an accessor, which is a view over the connection, and it
    runs the two workflows that span repositories: a conversation opens its workspace first, and
    an assistant turn (the message, its markdown copy and its artifacts) is one transaction, so
    a refused artifact leaves nothing behind. The TypeScript convenience methods are not copied,
    because callers use the accessors. The markdown title of a reply is passed in, since the
    TypeScript title depends on the locale's date format and the interface knows the language.
    The brain memory retrieval, the paged lists and the other methods that only delegate are
    repository calls.
- **Remaining, phase 2:** upgrade fixtures from the other tagged releases (0.5.0-beta to
  0.9.0-beta, and 1.0.0 to 1.0.2), and the operation timing observer, which goes with the
  diagnostics command.
- **Not started:** phases 3 to 9.

The row gate now reads rows that the TypeScript repositories wrote at schema 32, and it
upgrades a database that release 0.9.5-beta wrote. The migrations of that release are read
from git and run as they ran then. The TypeScript upgrade of a copy is the reference, and
Rust's export of the same file must match it collection by collection, with the same user
version. The fingerprints cover the other releases only for their schema. The releases from
0.5.0-beta to 0.9.0-beta and from 1.0.0 to 1.0.2 still need fixtures, so phase 2 stays open
until they are covered.

## Open decisions

1. **Signing and updater keys.** Who holds the signing keys, and how are the
   Electron fuses and ASAR integrity replaced? The repository already ships
   `docs/release-signing-public.asc`.
2. **UI framework.** Leptos (recommended here) or Dioxus. Changing this after
   phase 4 is expensive.
3. **Renderer test harness.** Tauri WebDriver or Playwright against the WASM build.
4. **Minimum platform versions.** WebKitGTK on Linux (the spike ran on 2.54), WebView2
   on Windows, WKWebView on macOS. `docs/compatibility.md` must state them.
6. **Provider key storage.** *Partly superseded by
   [ADR 0002](0002-envelope-provider-credentials.md).* Proposed: secrets live only in the operating system
   credential store, with no plaintext or application-encrypted fallback. The
   alternative is an application-encrypted file, which needs a master key the
   app would have to store itself and so adds little protection. Confirm this, and
   confirm that existing provider keys are re-entered after the migration.
   Decided in ADR 0002: the operating system store holds one 32-byte master key, and
   the secrets sit in an encrypted file. Still in force: no plaintext fallback, and
   existing provider keys are re-entered once after the migration.
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
