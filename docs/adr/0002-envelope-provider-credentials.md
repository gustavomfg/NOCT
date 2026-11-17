# ADR 0002: Envelope encryption for provider credentials

- **Status:** Accepted. **Partly supersedes [ADR 0001](0001-rust-migration.md):** decision 6
  (provider key storage) and the storage design of the phase 5 vault. Nothing else in ADR 0001
  changes.
- **Branch:** `migration/rust`.
- **Validation:** the operating system round trip (Linux, Windows, macOS) is defined in
  `.github/workflows/keyring-round-trip.yml`. It had not run when this ADR was written; see
  [Validation](#validation).

## Context

ADR 0001 proposed that provider secrets live only in the operating system credential store, with
no application-encrypted form. The vault built on that proposal stored each secret as its own
store entry. Two limits make that design unsafe to ship:

- **Windows.** Credential Manager limits a credential blob to 2,560 bytes. The `keyring` crate
  writes passwords as UTF-16, so a secret of about 1,280 characters already exceeds the limit. The
  Rust vault accepted 64,000 characters, a limit ported from the TypeScript vault that was never
  checked against Windows.
- **Linux.** The Secret Service round trip could not be verified on the development machine, whose
  wallet was locked. A store that requires an unlocked collection for every read cannot hold the
  secret bodies without a test of the unlocked path on each platform.

## Decision

The operating system store keeps one 32-byte master key. Provider secrets live in one encrypted
file, `provider-credentials.enc`, and the references live in a plain index, `provider-credential-index.json`.
Both files sit in the vault directory with mode 0600, in a directory with mode 0700.

### Cipher and key

- **Cipher.** XChaCha20-Poly1305 from `chacha20poly1305` 0.11.0 (RustCrypto, `Apache-2.0 OR MIT`,
  MSRV 1.85). The crate was published on 2026-06-28, is not yanked, and is the newest stable
  version. Its 192-bit nonce is drawn at random for every write, so no counter is kept. Its pure
  Rust implementation does not depend on the CPU's AES instructions, so the behavior is the same
  on the three platforms. The `zeroize` feature is enabled, so the cipher state holding the key is
  zeroed when it is dropped.
- **Key.** The master key is 32 bytes from the operating system random source (`getrandom`). It is
  stored with `set_secret` as raw bytes. It is wrapped in `secrecy::SecretBox`, and its `Debug`
  output is redacted. A stored key of any other length is corrupt.
- **Sealed layout.** One version byte, the 24-byte nonce, then the ciphertext and its 16-byte tag.
- **Associated data.** The purpose string `noct/provider-credentials`, followed by the format
  version byte. A changed version, a changed purpose, or bytes read as another file fail
  authentication and are reported as `corrupt-store`. A test seals with another version in the
  associated data and checks that the file does not open.

### Documents

- **Secret document.** The plaintext is the JSON object `{"version":1,"secrets":{"<reference>":"<secret>"}}`,
  bounded at 1 MiB. The references are canonical lowercase UUIDs.
- **Index.** `{"version":1,"references":[...]}`, bounded at 1 MiB. It holds no secret material.

### Configured references

A reference is **configured** only when the index names it and the secret document holds its
secret. Every other state is treated as **not configured**:

- the index names the reference but the document does not hold its secret: `resolve` returns
  nothing and `has` is `false`;
- the document holds a secret the index does not name (an orphan): it is never returned, and
  `prune` removes it;
- the document does not exist: nothing is configured, and the store is not read.

`has` reads the index on every call. It reads the document, which needs the store unlocked and the
master key present, only when the index names the reference and the document exists. This replaces
the earlier rule that `has` reads only the index. The renderer still learns only configured or not
configured.

### Order of writes

- **create.** A master key is created only for an empty vault (no index and no document), and only
  when the store answers. Then the secret is written to the document, and the reference is appended
  to the index. If the index write fails, the secret is removed from the document again. A crash
  between the two writes leaves a secret that no reference names. It is not configured, and
  `prune` removes it. A reference is never indexed without a secret the vault wrote.
- **replace.** Only the document changes. The index must already name the reference.
- **delete.** The secret leaves the document first, then the reference leaves the index. A failure
  in between keeps the reference indexed, so the delete can be retried.
- **prune.** The document is rewritten first, to drop stale and orphan secrets. Then the index drops
  the stale references. The document is read even when nothing is stale, so that orphans are found.
  On a locked store, prune fails and changes nothing.
- **reset.** The store is checked first. An absent or a present master key both allow the reset. A
  locked or unavailable store fails with `secure-storage-unavailable`, and nothing changes. Otherwise the
  document is removed, the index is emptied, and the master key is deleted, in that order.

### Lost key, locked store, and reset

- **Lost key.** If the index or the document holds anything and the master key is gone, the vault
  reports `master-key-missing` and never creates a key. A new key could not open the old secrets,
  and creating one would hide the loss.
- **Locked or unavailable store.** The vault reports `secure-storage-unavailable`. It never creates a
  key in that state, so a locked empty vault stays empty.
- **Reset (confirmed decision 5).** Reset is explicit and triggered by the user. The
  interface offers it only after `master-key-missing`, which is the case where the entry is absent.
  It is never offered after `secure-storage-unavailable`, because that state means the store is
  locked, and a reset there would look like a recovery when it is not.

### Size

The secret is bounded at 1 MiB, the same bound as the whole document. The 64,000-character limit is
removed on the Rust side. The TypeScript constant `credentialCharacters` stays until the TypeScript
application is retired, and `constants_parity` pins the difference.

### Zeroing, stated precisely

- The decrypted buffer returned by `envelope::open` is a `Zeroizing<Vec<u8>>`. Its `Drop` zeroes the
  bytes (`zeroize` 1.9.1, `impl Drop for Zeroizing`), and it is dropped at the end of the document
  load.
- Encoding writes into a buffer created with the bound as its capacity. A write past the bound fails,
  so the buffer is never reallocated, and no earlier copy of the plaintext is left in a freed
  allocation. `serde_json` writes the escaped string straight into that writer.
- Decoding copies each secret into an exact-size string and zeroes the decoded copy.
- **Residue, not zeroed:** `serde_json`'s decoder keeps unescaped text in a scratch buffer
  (`read.rs`, `scratch.push`). A secret that contains characters JSON escapes (quotes, backslashes,
  control characters) passes through that buffer, and the crate does not zero it. The same applies
  to error paths of the decoder. Provider keys rarely contain those characters, and the file is
  authenticated before decoding, so the residue needs a process memory read to be exposed. Removing
  it would mean a different document encoding, which this decision does not make.

### Migration

Provider keys of the Electron version are not migrated. Users enter them again once after the
migration. This keeps Electron's `safeStorage` format out of the Rust code.

## Keyset listings of the semantic index

The two listings that feed internal routines are paged. The rules are the same for both:

- The page limit is mandatory and clamped to `PAGE_LIMIT_MAX` (500), so a caller cannot ask for
  more than the ceiling. A zero limit is clamped to one.
- The query fetches one row more than the limit. `has_more` reports whether that row exists, so no
  further query is needed. `next` is the cursor after the last row kept, and is present exactly
  when `has_more` is set.
- The cursor is exclusive. A file's units resume after `(start_line, start_column, id)`. The
  indexed embeddings resume after `id`.
- The internal streams end at the first page without more rows, so a full last page does not
  produce an empty page.

### Query plans

The planner's choice for the embeddings listing depends on statistics. This section is a summary. The
measurements, the method and their limits are in [`docs/perf/semantic-index-query-plans.md`](../perf/semantic-index-query-plans.md).

- Without statistics, the embeddings page uses `idx_semantic_units_workspace_status` and a temporary sort. The
  identifier cursor `id > ?` is a filter, not a seek. For one workspace, the page walk grew 13.5 times from 50,000
  to 200,000 rows (2.1 s to 27.8 s).
- With exact statistics (`ANALYZE`), the plan depends on the workspace's share of the table. A workspace that is
  the whole table is read in identifier order, and its page walk grew 4.0 times (164 ms to 662 ms).
- Among ten workspaces, the other workspaces cost nothing measurable without statistics. With statistics, the
  planner switches plan for a workspace that is a tenth of the table, and the listing costs 5.2 times more than
  the same rows alone. This is a plan switch, not a scan of the other workspaces. The measurements cover one
  layout per table size.
- `PRAGMA optimize` on a database without statistics stores sampled estimates, not exact counts, and does not
  refresh them when the table grows. It does not replace `ANALYZE`.
- The `plan_tests` assert plans on an empty database, which has no statistics. The `benchmark_tests` measure the
  populated plans. They are ignored by default and run in release mode.

### Decision

Option C: no NOCT schema change and no `user_version` change. The first `ANALYZE` makes SQLite create its
internal tables `sqlite_stat1` and `sqlite_stat4`. The TypeScript build already creates `sqlite_stat1` through
`PRAGMA optimize`.

`ANALYZE semantic_units` runs in three places. A failure of the analysis is written to the diagnostic log and
does not fail the caller: the round still updates, the open still succeeds, and the close still runs `PRAGMA
optimize` and the WAL checkpoint. The optimize and the checkpoint keep their own errors.

1. **End of a large round.** A round that ends (completed, failed or cancelled) with at least 10,000 indexed units
   analyzes the table after the round is recorded. The update still succeeds when the analysis fails.
   `a_failed_analysis_after_a_large_round_is_logged_and_the_round_still_updates` (integration) checks it with the hook
   described under close: the round is recorded, one log entry is written, and no statistics are stored. A read-only
   connection cannot be used for this: the round's own write would fail first.
   `a_failed_analysis_after_a_large_round_is_logged_and_returns_nothing` (unit, `statistics.rs`) covers the analysis
   alone on a read-only connection.
2. **Open.** When the table has at least 10,000 rows and its statistics are missing or do not describe it, the
   analysis runs in the background, on its own connection. It waits at most `statistics_busy_timeout` for a lock
   (5 seconds by default, `OpenOptions`). `a_background_analysis_blocked_past_a_short_busy_timeout_is_logged_and_reported`
   (unit, `statistics.rs`) holds a write lock on a second connection, sets the timeout to 10 ms, and checks that the
   outcome is `Failed` and that one entry is logged.
3. **Close.** The same check runs on the main connection before `PRAGMA optimize`. Its failure is logged, and the
   optimize and the WAL checkpoint still run; the optimize is reported first when both fail. Three tests cover this:
   - `the_checkpoint_runs_when_the_optimize_fails` (unit, `runtime.rs`) makes the optimize fail on a read-only
     connection and checks that the checkpoint still copies every frame of the log.
   - `a_failed_statistics_step_at_close_still_runs_the_optimize_and_the_close_succeeds` (integration) denies the first
     `ANALYZE` check of the connection, which is the refresh at close. The close succeeds, the failure is logged once,
     and the optimize stores its estimates.
   - `a_failed_refresh_at_close_is_logged_and_the_next_open_recovers` (integration) uses a read-only connection. The
     optimize fails too, so the close reports the optimize's error. The log entry and the next open are checked.

   The limits of these tests: a lock held by another connection fails the refresh and the optimize together, so no
   test shows the optimize running after a lock failure. Denying every `ANALYZE` check also fails the optimize,
   because `PRAGMA optimize` makes its own check. That is why the hook denies only the first one. A checkpoint blocked
   by another connection (`busy`) is reported and not tested. The hook is a dev-only dependency feature of `rusqlite`
   (`hooks`, in `crates/noct-db/Cargo.toml`); the application build does not compile it.

The check compares the statistics with the table. It does not test for missing statistics only. It reads the row
count of `semantic_units` and the first two figures of the stored statistics of
`idx_semantic_units_workspace_status`. The stored row count and the stored rows per workspace must each be within
10% of the table's: the row count, and the rows divided by the number of distinct workspaces. A sampled analysis
keeps the total right and the per-workspace figure wrong: 1,409 rows per workspace against 10,000 at the smallest
measured size. A check on the total alone misses it. The tests `opening_a_database_with_sampled_statistics_analyzes_it`
and `opening_a_database_sampled_across_ten_workspaces_analyzes_it` fail when it is used.

The check runs at every open, and it does not analyze at every open. An analysis costs 11 ms at 10,000 rows, 46 ms
at 50,000 and 170 ms at 200,000, and it writes to the database each time. The check costs under 2 ms at 200,000
rows. The figures are in the performance note.

The check covers the sampled estimates that `PRAGMA optimize` stores. The TypeScript build runs the same
statement at close (`electron/database/DatabaseRuntime.ts:112`). A database closed by that build was not opened
here, so its figures are not confirmed. The check reads the workspace figure only. The figure of the
(workspace, status) prefix is not checked.

The thresholds are judgements, not budgets: 10,000 rows, and a 10% tolerance. The performance note records their
limits.

### Schema version

The schema version is 32 everywhere it is held: `shared/constants.ts:1`, `crates/noct-core/src/limits.rs:9`, and
the last migration, `electron/database/migrations.ts:727` (`version: 32`). Both runtimes refuse a database whose
`user_version` is above that constant. The Electron runtime compares at `electron/database/DatabaseRuntime.ts:33`,
against the constant it imports at line 5. That file holds no literal 33. The Rust runtime compares at
`crates/noct-db/src/runtime.rs:101`, against `SUPPORTED_SCHEMA` (`runtime.rs:35`), which is the same constant.

### Deferred

The index `(workspace, status, id)` is evaluated and is not in the schema. It would be migration 33, the next one
after the current version 32. It goes into a versioned migration, in Rust and in TypeScript with fingerprints, only
after the Electron build no longer reads the database. Until then both runtimes refuse a version-33 database,
because 33 is above their constant of 32. The measured effect is in the performance note.

## Fuses and ASAR

Electron's production fuses and ASAR integrity have no object in the Tauri build. Tauri embeds the
frontend assets in the binary, so there is no `app.asar` to verify and no Electron fuse to set. The
reason is the packaging model, not the signing decision. The signing and updater key decision
(ADR 0001, open decision 1) stands on its own.

## Consequences

- The Windows limit no longer applies to the store, which holds 32 bytes. The document is bounded
  at 1 MiB, and the bound is enforced by the encoder.
- One more file is part of the vault's state. A damaged file fails closed as `corrupt-store`.
- Losing the master key makes the stored secrets unreadable. Reset is the only way back, and the
  Electron-era keys must be entered again.
- `has` may ask the user to unlock the store, once the index names a reference and its document
  exists.
- The residue in the decoder's scratch buffer is accepted and stated above.

## Validation

- **Envelope unit tests (6):** round trip under the same key only, every byte authenticated, the
  format version in the associated data, a new nonce on every seal, key length, redacted `Debug`.
- **Vault tests on an in-memory store (28):** create and resolve with ciphertext only in the file,
  a three-kilobyte round trip, replace, delete, retry after a rejected removal, prune, malformed
  references, unindexed references, empty vault, size bounds, escaping refused with the index kept,
  invalid index, failed write leaves no reference and a retry reuses the key, private permissions,
  symlink refusal for the index and the document, tampered document, replaced key, lost key,
  locked store, locked empty vault creates no key, divergence in both directions, missing document
  without a key, and reset in its three cases.
- **Operating system round trip (ignored by default):** `keyring-round-trip.yml` runs a 3 KB secret
  through the real store on Linux (under a private session bus, with a Secret Service daemon and an
  unlocked collection that holds no real secret), Windows and macOS. The test uses its own service
  name, so it never reads or removes the master key of the installed application.
  **Result on commit `10e8eb0` (run 43794363520):** `ubuntu-24.04`, `windows-latest` and `macos-latest` pass
  the unit tests and the round trip. The Linux job had failed on the previous run (43661858148, commit
  `d3eae4e`) with `SecureStorageUnavailable`. The daemon received an empty password without a line feed and
  asked for it through a prompt. The fix is in `58c1f8d`.
- **Dependency policy:** `cargo deny 0.20.2` passes `advisories`, `bans`, `licenses` and `sources`.
  The advisories it ignores are listed in `deny.toml`, with the reason and the upstream crate for
  each. `cargo audit 0.23.0` finds no vulnerability. It reports six warnings: five unmaintained
  crates, which the policy also lists, and `glib` 0.18.5 (RUSTSEC-2024-0429, unsound). That one is
  reached only through the GTK 0.18 bindings that Tauri 2 requires on Linux, and cargo-deny does not
  report unsound advisories by default.
- **Query plans:** the tests in `crates/noct-db/src/repositories/semantic_index.rs` (`plan_tests`)
  assert the plans on an empty database. `benchmark_tests` measures the populated ones.

## References

- ADR 0001, decision 6 and the phase 5 progress note.
- `crates/noct-secrets/src/envelope.rs`, `document.rs`, `vault.rs`, `file.rs`.
- `crates/noct-db/src/repositories/semantic_index.rs`, `crates/noct-db/tests/semantic_pagination.rs`.
- `.github/workflows/keyring-round-trip.yml`, `.github/workflows/rust-audit.yml`, `deny.toml`.
