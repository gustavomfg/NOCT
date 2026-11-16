# Windows: the local files have no 0600 equivalent

Status: the warning is implemented; the ACL is not. The owner decided that `windows-sys` is not approved now, so the ACL
waits for the integration of the Rust vault, with the owner present. The open points are at the end.

## What is applied today

The Rust code restricts the local files on Unix only. Each place is gated with `#[cfg(unix)]`:

- Files: mode `0600`, created with `create_new` (`crates/noct-secrets/src/file.rs`, `crates/noct-db/src/file_log.rs`,
  `crates/noct-db/src/runtime.rs`, `crates/noct-db/src/recovery.rs`, `crates/noct-db/src/persistence/atomic_file.rs`).
- Directories: mode `0700` (`crates/noct-secrets/src/file.rs`, `crates/noct-db/src/runtime.rs`).

On Windows the mode is ignored. A new file or folder takes the access rules that its parent folder passes down (its
inherited ACL). The TypeScript application is the same: its `chmod` calls (`electron/persistence/AtomicFile.ts`,
`electron/security/ProviderCredentialVault.ts`) only set the read-only flag on Windows.

## Files affected

| File (default folder) | Content | Sensitivity | Written by |
| --- | --- | --- | --- |
| `nocturne.db`, `-wal`, `-shm` | Plaintext SQLite: conversations, workspace paths, memories, suggestions | High | `noct-db/src/runtime.rs` |
| `nocturne.db.backup-<ms>.db`, `nocturne-before-restore-<ts>.db` | Copies of the database | High | `noct-db/src/runtime.rs` |
| `nocturne.log`, `nocturne.log.1` | JSON lines, redacted before they are queued (`noct-db/src/file_log.rs`) | Medium: activity metadata | `noct-db/src/file_log.rs` |
| `provider-credentials.json` (TypeScript vault, in use today) | Base64 of the ciphertext of each credential, from Electron `safeStorage` | Low alone | `electron/security/ProviderCredentialVault.ts` |
| `provider-credentials.enc` (Rust vault, not yet opened by the shell) | XChaCha20-Poly1305 envelope of the credential document | Low alone | `crates/noct-secrets/src/document.rs` |
| `provider-credential-index.json` (Rust vault) | Opaque UUIDs of the references, nothing else (`crates/noct-secrets/src/index.rs`) | Low | `crates/noct-secrets/src/index.rs` |

No provider secret is stored in the database: `provider_configurations.credential_ref` holds a 36-character reference.

## The real risk

**Confidentiality of the vault.** The ciphertext does not open without the key, and the key belongs to the user. For
the TypeScript vault, Electron's `safeStorage` uses DPAPI on Windows, which is bound to the user's logon. For the Rust
design, the master key sits in the Windows Credential Manager. Another standard user cannot read the key, so a wide
ACL on the folder does not expose the secrets to them. Administrators and SYSTEM read any file in any case.

**Integrity and availability of the vault.** Here a wide ACL matters. Another user who can write the folder can delete
or replace `provider-credentials.json` or the index. The envelope authenticates its contents, so a replaced file cannot
be read as valid, but the references can be lost or rolled back.

**Database and backups.** The database is plaintext. Its confidentiality depends only on the folder's ACL. The default
folder is inside the user's profile, whose ACL normally excludes other standard users. That was not verified on a
Windows machine here.

**`NOCT_DATA_DIR` outside the profile.** This is the case that changes the picture. `resolve_data_directory`
(`crates/noct-app/src/bootstrap.rs`) accepts any absolute path and checks neither its location nor its ACL, and a
release build honours the variable too. A data folder in a shared location (for example `C:\Users\Public`), on a
network share, or in a cloud-synced folder (OneDrive, Dropbox) exposes the plaintext database and its backups to other
users, or uploads them. The same applies to a folder that inherits `Everyone` or `Users` read access from its parent.

The residual exposure of the default folder is low. The exposure of a custom folder is the real one.

## What is implemented

- **The warning, on Windows only.** When the data folder is outside the user's profile (the folder named by
  `USERPROFILE`), `bootstrap::open` writes one warning to `nocturne.log`: category `bootstrap.data-directory`, level
  `warn`, and a fixed message in English that names no path. The shell still opens the folder; the check informs, it
  does not refuse. The profile is not checked on the other systems, where the POSIX modes apply.
- **The tests** (`crates/noct-app/tests/bootstrap.rs`): a folder outside the profile is warned and still opens; a folder
  inside it, or a shell with no known profile, is not warned; a sibling whose name starts with the profile's name is
  outside it. The reading of `USERPROFILE` itself runs only on Windows and is not tested directly; the Windows job
  runs the rest of the tests.
- **Not implemented:** the ACL of the folder (proposal items 1, 2 and 4 below).

## Proposal

1. **A protected ACL for the folder the shell creates.** The folder gets a DACL with inheritance removed, granting full
   control to the current user and to SYSTEM. New files and subfolders inherit that DACL. SYSTEM stays in the list so
   that backup software and antivirus keep working. Administrators still read the files, as they do everywhere on
   Windows; this protects against other standard users and against sync, not against administrators.
2. **How to set it.** `SetNamedSecurityInfoW` with `PROTECTED_DACL_SECURITY_INFORMATION`, and an ACL built with
   `SetEntriesInAclW` from the current user's SID (`GetTokenInformation(TokenUser)`). The `windows-sys` crate is
   already in `Cargo.lock` as a transitive dependency, in two versions. Using it directly adds a direct dependency,
   which needs the owner's approval first. `icacls` run as a child process was rejected: its output depends on the
   locale, and it spawns a process to change a security descriptor.
3. **`NOCT_DATA_DIR` policy.** The warning when the folder is not under the user's profile is implemented (see above).
   Refusing such a folder, or refusing one whose ACL grants access beyond the owner, SYSTEM and Administrators, is not
   proposed now.
4. **Tests.** A test that runs only on Windows, in the existing `shell` matrix: after `bootstrap::open`, the folder's
   DACL has no entry for `Everyone`, `Users` or `Authenticated Users`, and a file created in it inherits the same rule.
5. **Nothing in the TypeScript application changes.** Its files keep their current behaviour on Windows, and this
   proposal does not reach it.

## Decisions

Decided by the owner:

- `NOCT_DATA_DIR` outside the profile: warn, and do not refuse. Implemented.
- `windows-sys` as a direct dependency: not approved now. It is decided with the owner present, in the integration of
  the Rust vault.
- The ACL itself is not applied by this slice.

Still open, for the integration:

- Keep SYSTEM in the DACL (recommended), or owner only?
- Apply the DACL to a folder that already exists, or only to folders the shell creates? Changing an existing folder
  silently is a behaviour change, so the recommendation is to warn and not to change it.

## Not verified

- The ACL of a default profile folder on a Windows machine.
- The behaviour of DPAPI for the TypeScript vault on this machine's Windows setup.
- Any of the proposal: it needs a Windows run, which the matrix provides only after a change is made.
