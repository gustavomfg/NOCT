# NOCT — Agent Instructions

## Product identity

NOCT is a local desktop Engineering Workspace for coordinating
a developer, user-selected AI providers, and the state of a real software project.
It is free software, independent of any AI vendor; no vendor CLI or account is required.

It is not:

- an IDE;
- a replacement for developer judgment;
- merely a chat client;
- tied to a single AI vendor.

The developer retains intent, approval, judgment, and accountability.

The expected engineering loop is:

Analyze → Plan → Explain → Suggest → Approve → Implement → Validate

Do not collapse analysis, approval, implementation, and validation into one
implicit step when the existing product flow keeps them separate.

## Current release line

Current release line: `0.1.0` (the Rust product; the Electron line ended at `1.0.5`). The historical `v1.0.1` tag and release assets
must not be moved or modified. The previous TypeScript and Electron product is
kept at the tag `electron-final`; nothing in this branch depends on it.

Current priorities:

1. Stability and compatibility.
2. Predictable developer experience.
3. Performance on large histories and workspaces.
4. Secure and recoverable local persistence.
5. Safe cross-platform packaging and updates.
6. Explicit compatibility with supported provider protocols.

Provider APIs evolve independently. Validate adapter compatibility explicitly;
keep provider credentials in the main process and support local endpoints.

## Technology stack

- Rust workspace (toolchain pinned in `rust-toolchain.toml`, with the
  `wasm32-unknown-unknown` target)
- Tauri 2 shell (`crates/noct-app`), with its plugins pinned in `Cargo.toml`
- Leptos 0.8 interface, client-side rendered and compiled to WebAssembly with
  trunk (`crates/noct-ui`)
- SQLite through the Rust persistence crate (`crates/noct-db`)
- Operating-system credential stores (`crates/noct-secrets`)
- serde / serde_json for every boundary contract
- Node.js 24 and npm 11 only for the launcher (`scripts/run-rust-shell.mjs`),
  the Node tests of the scripts and the Playwright (Chromium) checks

There is no Electron, React, TypeScript or Vite code in this branch. Do not
reintroduce them without an explicit architectural decision.

Required development environment:

- Rust as pinned by `rust-toolchain.toml`
- trunk `0.22.1`
- Node.js `>=24.18 <25` and npm `>=11 <12`
- On Linux, the Tauri system packages listed in the README

Do not change the pinned Rust, trunk, Node or npm lines without an explicit
compatibility task.

## Architectural authority

Before making architectural or security-sensitive changes, inspect the relevant
documentation under `docs/` (ADRs in `docs/adr/`, `docs/status.md`,
`docs/provider-allowlist-design.md`), the README, and existing implementation.

Prefer sources in this order:

1. Accepted architecture and security documentation.
2. Current implementation and shared contracts.
3. Tests that encode expected behavior.
4. README and contributor documentation.
5. Changelog and historical notes.

If authoritative sources conflict, report the conflict instead of choosing
silently.

## Shell security invariants

These requirements must remain true unless an explicit architectural decision
changes them:

- The interface reaches native capabilities only through Tauri commands that
  are listed in the capability files and covered by the ACL test
  (`crates/noct-app/tests/capabilities.rs`). Never grant a broad or generic
  permission to the window.
- Every command validates its input against its contract and returns a
  `CommandError` with a code and a fixed message; causes, paths and stacks stay
  in the main process.
- The Content-Security-Policy stays strict: no inline script, no inline style
  (views use classes only), no remote origins for the page.
- Provider credentials never reach the interface, the logs or the repository.
  They live in the operating-system credential store.
- Provider network calls go through the network policy (built-in approvals,
  resolved-address checks, pinned addresses, no proxy, no redirects, timeouts,
  byte caps).
- Workspace paths are normalized and constrained before access; symbolic links
  out of the project are refused.
- Review Mode remains read-only.
- Assistant output is untrusted: it is drawn as text (Markdown without HTML, no
  clickable links, no loaded images), and proposed file changes stay pending
  until the person approves each one.
- Restored workspaces remain unauthorized until explicitly reselected.
- Release builds keep updater signature checks.

When changing a native capability, update together:

- the command and its input/output types;
- the capability file and the command list;
- validation and error behavior;
- relevant tests (including the ACL test);
- documentation when the contract is externally meaningful.

## Modes and boundaries

### Review Mode (Revisar)

Review Mode is analysis-only and must remain read-only. It may inspect files,
explain findings and record suggestions or proposed changes as pending. It must
not modify the workspace.

### Build Mode (Construir)

Build Mode may modify files only within the configured workspace, one approved
change at a time, according to the path rules of
`noct_core::file_change::check_relative_path`. Preserve visibility of the
changed files, the diffs, the approvals and undo.

### Docs Mode (Documentar)

Docs Mode shows the proposed changes to Markdown files of the project, with the
same approval flow. Do not use a documentation-only task as justification for
unrelated source-code changes.

## Persistence and recovery

Treat the SQLite database, the data folder and workspace context as sensitive
state.

When changing persistence:

- preserve transactional migrations;
- preserve rollback on invalid restoration;
- reject duplicate or inconsistent identifiers;
- avoid partial writes; use atomic writes for files in the data folder;
- preserve restrictive permissions;
- test migration from the previous supported schema;
- preserve existing user data unless deletion is explicitly required.

Never point the shell at a data folder that holds real data during
development or tests; use `--data-dir` with an empty temporary folder.

Never add databases, tokens, authentication data, private logs, or unsanitized
diagnostics to the repository.

## Performance

Performance is part of correctness.

When changing interface behavior:

- isolate frequently changing state in its own signals;
- avoid redrawing whole lists for streaming deltas;
- keep long lists, large diffs and large replies bounded;
- avoid blocking work on interactive paths.

When changing main-process behavior:

- avoid blocking command handlers (use blocking tasks for disk work);
- avoid duplicated provider operations;
- preserve deterministic cleanup;
- handle failure and cancellation explicitly;
- consider large workspaces, histories, backups, and diffs.

Do not claim a performance improvement without a measurement, regression test,
budget, or clearly observable reduction in work.

## UI, accessibility, and design system

Respect the existing Nocturne design language (`DESIGN.md`,
`docs/New_Design/`) and the tokens of `crates/noct-ui/shell.css`.

Preserve:

- light and dark themes, and the glass layer with its solid fallback;
- canonical responsive ranges;
- minimum typography and interaction targets;
- visible focus and keyboard navigation;
- focus containment and restoration in dialogs and popovers;
- reduced-motion and reduced-transparency behavior;
- clear async and error feedback in Portuguese;
- accessible status communication;
- stable geometry during hover, focus, and click.

Do not introduce isolated visual values when an existing token, component, or
design-system rule can be reused. Check visual changes in the Chromium check
and, when possible, in the real window.

## Project structure

Respect existing boundaries:

- `crates/noct-core/` — pure rules, contracts and limits (no I/O);
- `crates/noct-app/` — the Tauri shell: commands, capabilities, services,
  provider transport;
- `crates/noct-ui/` — the interface (Leptos, WebAssembly);
- `crates/noct-db/` — persistence and migrations;
- `crates/noct-secrets/` — credential stores;
- `crates/noct-native/` — native helpers;
- `scripts/` — launcher, Chromium checks and release tooling;
- `docs/` — architecture, protocols, security, and workflows.

Before creating a module:

1. Search for an existing responsibility.
2. Reuse shared contracts and canonical limits (`noct_core::limits`).
3. Avoid duplicate utilities and competing abstractions.
4. Keep interface, shell and core responsibilities separate.

Do not move or rename broad areas of the project without an explicit reason and
a validation plan.

## Change discipline

Before implementing a non-trivial task:

1. Inspect the relevant implementation and tests.
2. Identify affected processes and trust boundaries.
3. Check shared contracts and documentation.
4. State assumptions and gaps.
5. Present a short plan when the change crosses multiple crates.

Prefer small, focused changes. Avoid speculative abstractions, unrelated
cleanup, silent behavioral changes, broad rewrites without evidence, and
weakening a security boundary for convenience.

Do not modify unrelated user changes in the working tree.

## Dependencies

Before adding or replacing a dependency:

- explain why existing platform or project capabilities are insufficient;
- evaluate maintenance and security impact, and keep `cargo deny check`
  (`deny.toml`) passing;
- prefer `default-features = false` and only the features needed;
- consider binary and WebAssembly size and native-build implications;
- update `Cargo.lock` consistently;
- verify cross-platform builds when relevant.

## Validation

Minimum source validation:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p noct-ui --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
(cd crates/noct-ui && trunk build --release)
npm test
npm run check:csp
npm run check:ui
```

CI runs the same steps in `.github/workflows/rust-shell-validation.yml`.

# Nocturne Philosophy

When faced with multiple valid implementations, prefer the solution that:

- keeps the architecture clean;
- minimizes technical debt;
- improves long-term maintainability;
- respects existing project patterns.

Avoid implementing features that conflict with the long-term vision of the
project.

## Git Workflow

The Git history is part of the project's documentation.

Rules:

- One logical change = one commit.
- Group related code, tests and documentation together.
- Use Conventional Commits.
- Mark commits that change a trust boundary with `[security-review]` and record
  the review in `docs/status.md`.
- Run the relevant validations before each commit.
- Do not mix unrelated fixes.
- Do not create "miscellaneous" commits.
- Do not push or rewrite history unless explicitly requested.
