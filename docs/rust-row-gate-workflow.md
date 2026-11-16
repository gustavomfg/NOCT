# Running the row gate in CI before the merge to main

Status: implemented as `.github/workflows/row-gate.yml`. It runs on Ubuntu and is not a required check: making it required
is a setting of the repository, and the owner makes it.

## What the gate checks

`node scripts/verify-row-gate.mjs` checks that the Rust persistence layer reads and writes what the TypeScript layer
does, at schema 32:

1. It compiles the TypeScript persistence code (`electron/`, `shared/`) into `node_modules/.cache`.
2. The compiled TypeScript writes its SQLite databases into a temporary folder under `target/`.
3. It compiles each release line in `scripts/fixtures/release-lines.json` from its own tag, and checks that the commit
   and the source digest of every tag match the pinned values.
4. It runs `cargo test -p noct-db --test row_gate -- --ignored --nocapture` with `NOCT_ROW_GATE_DIR` set to that folder.
   The Rust tests open copies of those databases and compare what the Rust code returns with what the TypeScript code
   wrote. The ten tests are:
   - `rust_returns_the_rows_the_typescript_repositories_wrote`
   - `reconciles_like_the_typescript_repository`
   - `evidence_matches_the_typescript_repository`
   - `reconciles_suggestions_like_the_typescript_repository`
   - `removes_changed_files_like_the_typescript_repository`
   - `applies_semantic_writes_like_the_typescript_repository`
   - `applies_engineering_writes_like_the_typescript_repository`
   - `upgrades_legacy_databases_like_the_typescript_repository`
   - `upgrades_every_release_line_like_the_typescript_reference`
   - `applies_backup_imports_like_the_typescript_repository`
5. It removes the temporary folder, also when a step fails. The script exits with the status of the Rust tests.

The database schema is version 32, from `shared/constants.ts`.

No database is committed and no secret is used: the data is synthetic and stays under `target/`.

## Current state, measured

- Run locally on Linux with the repository's own `node_modules`, at `e2913ad` and again with the commits that add the
  workflow: the ten tests passed, the script exited with 0, and it took about 13 seconds. No temporary folder was left.
- Before `row-gate.yml`, no workflow ran it. `cargo test --workspace` does not run it either, because the tests are
  `#[ignore]`.
- `rust-shell-validation.yml` runs on pushes to `migration/rust` and on manual dispatch. `row-gate.yml` runs on pushes to
  `main` and `migration/rust`, on pull requests to `main`, and on manual dispatch.

## What CI needs

- **Node 24** (AGENTS.md), and `npm ci`. `better-sqlite3` is a native module built for the Node ABI. The Linux jobs
  of the shell workflow already run `npm ci`, so this is known to work on the runner.
- **Rust 1.97.1.** `rust-toolchain.toml` pins it, and rustup installs it on first use.
- **Full history and tags.** The release-line step resolves every tag with `git rev-parse`. A shallow clone or missing
  tags fails it on purpose, as "the tag moved" or a missing commit. The job needs `fetch-depth: 0` and
  `fetch-tags: true`, as the shell job already sets.
- **No secrets, no caches to trust.** The job reads only the repository.

## The job (option A, implemented)

A new file, `.github/workflows/row-gate.yml`, which leaves the shell workflow unchanged.

- Triggers: `push` to `main` and `migration/rust`, `pull_request` targeting `main`, and `workflow_dispatch`.
- `permissions: contents: read`. `concurrency` with the group `row-gate-${{ github.ref }}` and
  `cancel-in-progress: true`, as the shell workflow does.
- One job on `ubuntu-latest` with `timeout-minutes: 30`.
- Steps: checkout with `persist-credentials: false`, `fetch-depth: 0` and `fetch-tags: true`; `setup-node` with
  Node 24; `npm ci`; a check that `rustc` is the toolchain pinned in `rust-toolchain.toml`; `node scripts/verify-row-gate.mjs`.
- Actions: the two that are already pinned by SHA in `rust-shell-validation.yml` (`actions/checkout` and
  `actions/setup-node`, both v6). No new third-party action is needed.

Option B: a job `row-gate` inside `rust-shell-validation.yml`. The diff is smaller, but the gate would inherit that
file's triggers and its matrix, and it is about persistence parity, not about the shell.

## Before the merge to main

1. The job must run green at least once on `migration/rust`. GitHub lists a check as available for a required status
   only after it has run.
2. Then the check is made required on `main`. That is a repository setting (branch protection or a ruleset), so it
   is the owner's action, not a change in the code.
3. A `pull_request` trigger reads the workflow from the merge commit, so the job runs on the PR itself.

## What a failure tells the owner

- A TypeScript compile error stops the script before Rust runs.
- A row that differs fails the Rust test with the expected and the actual value.
- A moved tag, or sources that differ from their pinned digest, stop the script with the name of the tag.

## Decisions

Decided by the owner:

- Option A: a separate workflow, not a job of the shell workflow.
- Ubuntu only, with the checks above.
- Not a required check. Making it required on `main` is the owner's setting, and it is not in the code.

Recorded as proposed:

- The events are the ones above. The shell workflow keeps its own triggers.

## Not verified

- The duration on a cold GitHub runner. The local figure above is from a warm machine with `node_modules` installed.
- Whether `row-gate.yml` runs green on GitHub: the result is in the report of the commit that adds it.
- Windows and macOS: the gate has not been run there, and no job runs it there.
