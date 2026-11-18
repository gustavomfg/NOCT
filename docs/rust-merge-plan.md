# Merge plan: `migration/rust` into `main`

This document records what stands between the Rust shell branch and `main`, and how the Electron application and the
Rust shell coexist until the TypeScript code is removed. It describes the state of the work. It does not approve the merge.

Pull request [#23](https://github.com/gustavomfg/NOCT/pull/23) (`migration/rust` into `main`) is open and still a draft. The
work that wrote this document did not create, change or merge it.

## State of the checks on PR #23

These are the checks of the head `e8e5c23`, as `gh pr checks 23` reported them. Commits `ac2cfda` and `b867098`, and the
fix `df6e006`, are not in the PR head yet.

| Check | Result | Cause |
| --- | --- | --- |
| Verify source (Node 24), Verify source (Node 24.18.0) | fail | `npm run verify:release-metadata`: `README.md` does not contain `v1.0.5`. Blocker 1. |
| Persistence reliability (windows-latest) | fail | One test file, `tests/shellRelease.test.ts`, read the release workflow with CRLF line ends. Fixed in `df6e006`. 95 other test files passed. |
| Shell on ubuntu-latest, macos-latest, windows-latest | pending at the last read (run 44248536330, in progress) | Not finished. |
| Package and smoke; Renderer interaction and visual regression | skipped | Skipped on this run. The reason is not recorded here. |
| Analyze (actions, c-cpp, javascript-typescript, rust), CodeQL, Keyring round trip (three systems), Persistence reliability (macOS, Ubuntu), Row gate, cargo deny and cargo audit, Validate GitHub Actions workflows | pass | |

## Blockers before a merge into `main`

1. **`verify:release-metadata` fails on the branch README.** The check requires `README.md` to contain the version. The branch
   README is the migration's own document and does not contain `v1.0.5`; the README of `main` does. The options are the ones
   recorded in [`docs/status.md`](status.md#pending-merge-of-migrationrust-into-main): move the branch README to `docs/`, change
   the check, or take the `main` README and move the migration status elsewhere. This document does not choose one.
2. **The updater endpoint.** Resolved by design in `ac2cfda`. The shell reads the `updater-feed` pre-release, and no shell release
   is marked latest, so the Electron updater, which reads the release marked latest, is not affected. This is not yet exercised:
   no `rust-v*` tag has been pushed, so the feed does not exist yet, and no run has checked that GitHub serves a pre-release
   download URL the way the design assumes. See [`shell-updater.md`](shell-updater.md#release-workflow).
3. **The checks on the head.** The Windows persistence failure is fixed in `df6e006`, and the Verify source failures are blocker 1.
   The three Shell runs must finish green on the final head. A green run on an older head does not count.

## Not blockers for the merge, but blockers for replacing Electron

These do not stop `main` from carrying the Rust code while Electron remains the product. They must be settled before the Rust shell
can replace it. The ADR records each one.

4. **The shell is not checked in a running window.** The ADR marks the first slice (phase 6) as "not yet checked in a running
   window". Navigation and permission rules are covered by unit and mock-runtime tests, not by a running webview on each platform.
5. **Provider credentials are not migrated.** Users re-enter their provider keys once after the migration. The owner has to confirm
   this (ADR open decision 6).
6. **The user data location is not migrated.** The shell opens the application data folder of `com.nocturne.codex` by default and
   never the `Nocturne Studio` folder of an existing installation (`crates/noct-app/src/bootstrap.rs`). The ADR requires a phase 2
   step that reads the existing directory, with a test. That step does not exist in the code.
7. **Who holds the updater signing key.** ADR open decision 1. The public key is committed in `docs/noct-updater.key.pub`. Custody of
   the private key is not decided.
8. **Phase 3 and phase 8 are not started on the Rust side.** There is no macOS or Windows native rollback backend; the Rust tree has
   `crates/noct-native/src/platform/linux.rs` only. `tauri.conf.json` sets `bundle.active` to `false`, so there are no installers.
9. **Phase 9 is not started.** Retirement of `electron/`, TypeScript, Vite and npm is the last phase, and it depends on every other
   phase being green.

Phases 4 to 7 are partly done. The [phase table of the ADR](adr/0001-rust-migration.md#phases) names what each one covers and what
it does not.

## Coexistence until the TypeScript cleanup

1. **Electron stays the product on `main`.** Its tags are `v*`, its release marked latest is the Electron one, and its workflows
   (`stable-release.yml`, `package-validation.yml` and the rest) are not changed by the Rust work.
2. **The Rust shell ships only as `rust-v*` pre-releases** and through the `updater-feed` pre-release. The publish job never marks a
   shell release latest, and `tests/shellRelease.test.ts` checks it.
3. **The data stays separate.** The Electron application writes to `Nocturne Studio` (`shared/product-identity.json`). The shell writes
   to the application data folder of `com.nocturne.codex`, or to the folder named by `NOCT_DATA_DIR`. Pointing both at one folder is not
   supported: nothing tests two writers on one database, and the migration in blocker 6 does not exist.
4. **Credentials are separate.** The shell does not read the Electron `safeStorage` ciphertext. Provider keys are entered again in the
   shell (blocker 5).
5. **The shell's updater is inactive.** The main window has no updater permission, so the shell makes no request to the feed. Activating
   it is a separate change with its own review, described in [`shell-updater.md`](shell-updater.md#activation).
6. **The TypeScript cleanup is phase 9.** It is a later change, made after the phases it depends on are green. It is not part of the merge.

## Decisions for the owner

- Blocker 1: which of the three options to take.
- Blocker 5: confirm that provider keys are re-entered once (ADR open decision 6).
- Blocker 6: whether a data migration step must exist before the shell becomes the product. The ADR says yes.
- Blocker 7: who holds the updater signing key (ADR open decision 1).

## Not verified here

- No `rust-v*` tag has been pushed, so `shell-release.yml` has not run and `updater-feed` does not exist.
- The Shell CI runs for the final head have not finished.
- No running window has been checked on any platform.
- The Electron checks were run only as far as PR #23 reports them. No Electron check was run locally for this document.
