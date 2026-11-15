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

The plans are read from the same query builders that the listings run, in an in-memory database
with the schema of this build. The database was empty, so the plans reflect the indexes, not the
statistics of a large history.

- **File units, first page.** `SEARCH semantic_units USING INDEX idx_semantic_units_workspace_path
  (workspace=? AND relative_path=?)`, then `USE TEMP B-TREE FOR LAST 2 TERMS OF ORDER BY`.
- **File units, keyset page.** The same index, with `start_line>?` as a seek, then the same temporary
  tree for the last two terms of the order. The cursor is a seek on the first column, and the rest
  of the key is checked within that range. The test asserts the seek.
- **Embeddings, keyset page.** `SEARCH semantic_units USING INDEX idx_semantic_units_workspace_status
  (workspace=? AND status=?)`, then `USE TEMP B-TREE FOR ORDER BY`. **The cursor `id > ?` is not used
  as a seek.** Each page reads the indexed units of the workspace and sorts them by identifier, so the
  work per page grows with the number of indexed units, not with the page. The test asserts that the
  page is an index search and never a table scan.

Proposed, not implemented, because each needs a schema change with parity in the TypeScript
migrations (schema version 33 in both):

- an index on `semantic_units(workspace, status, id)` would let the planner seek on `id > ?`, and
  would remove the temporary tree of the embeddings page;
- an index on `semantic_units(workspace, relative_path, start_line, start_column, id)` would remove
  the last temporary tree of the file page.

The maintainer decides whether to add them. Until then, the cost of a large workspace's embeddings
listing is not measured here, and it must not be claimed as bounded per page.

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
- **Dependency policy:** `cargo deny 0.20.2` passes `advisories`, `bans`, `licenses` and `sources`.
  The advisories it ignores are listed in `deny.toml`, with the reason and the upstream crate for
  each. `cargo audit 0.23.0` finds no vulnerability. It reports six warnings: five unmaintained
  crates, which the policy also lists, and `glib` 0.18.5 (RUSTSEC-2024-0429, unsound). That one is
  reached only through the GTK 0.18 bindings that Tauri 2 requires on Linux, and cargo-deny does not
  report unsound advisories by default.
- **Query plans:** the tests in `crates/noct-db/src/repositories/semantic_index.rs` (`plan_tests`)
  assert the plans above.

## References

- ADR 0001, decision 6 and the phase 5 progress note.
- `crates/noct-secrets/src/envelope.rs`, `document.rs`, `vault.rs`, `file.rs`.
- `crates/noct-db/src/repositories/semantic_index.rs`, `crates/noct-db/tests/semantic_pagination.rs`.
- `.github/workflows/keyring-round-trip.yml`, `.github/workflows/rust-audit.yml`, `deny.toml`.
