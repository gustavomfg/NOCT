# Running the row gate in CI before the merge to main

Status: proposal. No workflow is changed by this document.

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

- Run locally on Linux at `e2913ad`, with the repository's own `node_modules`: the ten tests passed, the script exited
  with 0, and it took about 13 seconds. No temporary folder was left behind.
- No workflow runs it. `cargo test --workspace` does not run it either, because the tests are `#[ignore]`.
- `rust-shell-validation.yml` runs on pushes to `migration/rust` and on manual dispatch. Nothing runs on `main`.

## What CI needs

- **Node 24** (AGENTS.md), and `npm ci`. `better-sqlite3` is a native module built for the Node ABI. The Linux jobs
  of the shell workflow already run `npm ci`, so this is known to work on the runner.
- **Rust 1.97.1.** `rust-toolchain.toml` pins it, and rustup installs it on first use.
- **Full history and tags.** The release-line step resolves every tag with `git rev-parse`. A shallow clone or missing
  tags fails it on purpose, as "the tag moved" or a missing commit. The job needs `fetch-depth: 0` and
  `fetch-tags: true`, as the shell job already sets.
- **No secrets, no caches to trust.** The job reads only the repository.

## Proposed job

Option A (recommended): a new file, `.github/workflows/row-gate.yml`, which leaves the shell workflow unchanged.

- Triggers: `push` to `main` and `migration/rust`, `pull_request` targeting `main`, and `workflow_dispatch`.
- `permissions: contents: read`. `concurrency` with the group `row-gate-${{ github.ref }}` and
  `cancel-in-progress: true`, as the shell workflow does.
- One job on `ubuntu-latest` with `timeout-minutes: 30`.
- Steps: checkout with `persist-credentials: false`, `fetch-depth: 0` and `fetch-tags: true`; `setup-node` with
  Node 24; `npm ci`; `node scripts/verify-row-gate.mjs`.
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

## Decisions for the owner

- Option A or Option B.
- Only Ubuntu, as proposed, or also Windows and macOS? The gate's logic does not depend on the operating system, but
  the paths and the native module do, and none of it runs there today.
- Make the check required on `main` once it has run green, or only report it?
- Which events: the three above, or only the pull request?

## Not verified

- The duration on a cold GitHub runner. The local figure above is from a warm machine with `node_modules` installed.
- A run of the proposed workflow. It exists only as this proposal until the owner approves it.
