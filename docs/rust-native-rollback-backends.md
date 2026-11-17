# Rust rollback backends: what is missing on macOS and Windows

- **Status:** not implemented. This is a record of what the code does, taken by reading it on `migration/rust`
  at `de7dba4`. No run on macOS or Windows backs it.
- **Compared:** `crates/noct-native` (the Rust rollback worker), with `native/rollback-boundary.cpp` (the C++
  worker that the Electron product uses on every platform) and `native/rollback-windows.h`, which the C++
  worker includes on Windows.
- **Related:** [Native rollback V1](native-rollback-v1.md) describes the contract. [Compatibility](compatibility.md)
  lists the platforms.

## Current behaviour of the Rust worker

- On Linux, `crates/noct-native/src/main.rs` runs the protocol with the Linux backend in `linux.rs`.
- On every other platform, `main.rs` reads each line and answers `ERR\tUNSUPPORTED\t` followed by the hex of
  "Protected rollback backend not verified", until it reads `CLOSE`. Restoration fails closed, and nothing is changed.
- The tests of `noct-native` are in `linux.rs`, which compiles only on Linux. No test runs the non-Linux answer.
- The comment at the top of `main.rs` says the answer is "the fail-closed behavior of the C++ `#else` branch".
  That is true only for platforms the C++ worker does not support. On macOS and Windows the C++ worker has backends,
  so the Rust answer there is not the C++ behaviour, and the comment is inaccurate for those two platforms. This
  document changes no code. The comment should be corrected in a code commit.
- The Electron product does not use the Rust worker. `electron/change-control/NativeRollbackOperation.ts` runs
  `rollback-boundary` (`rollback-boundary.exe` on Windows). `scripts/build-native-boundary.mjs` builds it from
  `native/rollback-boundary.cpp`, and `electron-builder.json5` packages it as `native-boundary`.

## What the C++ worker does on macOS

Line numbers refer to `native/rollback-boundary.cpp` at `de7dba4`. The table lists what a Rust port for macOS would
have to match.

| Area | What the C++ worker does on macOS | Lines |
| --- | --- | --- |
| Platform gate | `INIT` requires the root's filesystem type to be `apfs` and the mount to be local (`MNT_LOCAL`). Otherwise it fails with `UNSUPPORTED`. | 182–185 |
| Protected names | Before the protected-name check, ASCII letters are lowercased, so case aliases of protected names are refused. The Linux build does not do this. | 132–136 |
| Journal | `steps.jsonl` and `operation.json` are created with `O_EXCL` and `O_NOFOLLOW`, mode `0600`, and are flushed at `INIT`. | 186–188 |
| Staging | No `O_TMPFILE` on this path. A named file, `.nocturne-rollback-stage-<pid>-<serial>`, is created with `O_EXCL` and `O_NOFOLLOW`, written, `fchmod`ed, flushed and read back. Afterwards only the retained descriptor selects it. | 219–236 |
| Displacement | `renameatx_np` with `RENAME_EXCL`. Linux uses `renameat2` with flag 1. The code claims no compare-and-swap on the source inode. | 238–254 |
| Publication | `fclonefileat` with `CLONE_NOOWNERCOPY` derives the published file from the staged descriptor. It is a controlled copy, not a link to the same inode. On macOS the worker then compares bytes. The inode-identity check runs on Linux only. | 256–275 |
| Durability | `flush` calls `fsync` on the descriptor, and nothing else. | 126 |

The walk through path components with `openat(O_NOFOLLOW)` from retained directory descriptors, and the refusal of
symlinks and device crossings, are described in [Native rollback V1](native-rollback-v1.md) (its macOS
paragraph). They are not repeated here.

## What the C++ worker does on Windows

Line numbers refer to `native/rollback-windows.h` at `de7dba4`.

| Area | What the C++ worker does on Windows | Lines |
| --- | --- | --- |
| Native calls | `NtCreateFile` is resolved at run time with `GetProcAddress` on `ntdll.dll`. | 67 |
| Drive type | The volume must report `DRIVE_FIXED` (`GetDriveTypeW`). Any other type fails with `UNSUPPORTED`. | 106 |
| Filesystem | The volume must be NTFS (`GetVolumeInformationByHandleW`). | 111–112 |
| Opening | `CreateFileW` with `FILE_FLAG_BACKUP_SEMANTICS` and `FILE_FLAG_OPEN_REPARSE_POINT`. Reparse points are refused. | 78, 108–109 |
| Identity | A file's identity is the volume serial number and the file index, from `GetFileInformationByHandle`. A walk that crosses a volume is refused. | 54–57, 97 |
| Exclusive rename | A `FILE_RENAME_INFO` with `RootDirectory` set and `ReplaceIfExists` set to `FALSE`. | 126–127 |
| Durability | `FlushFileBuffers` on the handle. A failure is reported as `UNKNOWN`. | 124 |

## Decision needed: Windows bindings

The Windows backend needs Win32 and NT calls: `NtCreateFile`, the file information structures and the volume queries.
The Rust crates do not declare them today. There are two ways to get them:

- **A new direct dependency**, such as `windows-sys`. `Cargo.lock` already contains `windows-sys` 0.61.2, pulled in
  transitively by Tauri, but making `noct-native` depend on it directly is still a new dependency. `AGENTS.md`
  requires its justification, maintenance and security review, and the owner's approval.
- **Declarations written in the repository**, with `extern "system"` and `unsafe` blocks. There is no new dependency,
  but the repository then keeps the ABI declarations and the struct layouts itself.

The owner has not chosen between them. The README of this branch records the related decision on the Windows ACL.

## Durability on macOS: parity with the C++ worker

Decision: the macOS backend uses `fsync`, as the C++ worker does. It does not use `F_FULLFSYNC`.

Checked on the branch: `F_FULLFSYNC` appears nowhere in `native/`, `electron/`, `scripts/` or `crates/`, and `native/` has
not changed since `de7dba4`. Every durability call in `native/rollback-boundary.cpp` goes through `flush`, which calls
`fsync` (line 126). The calls are at lines 188, 194, 196, 234, 250 and 270. The rule for this case is parity, so the
port keeps `fsync`.

The difference that stays: on macOS, `fsync(2)` does not ask the drive to flush its cache, and `fcntl(F_FULLFSYNC)` does.
So on macOS, a write that the worker acknowledged may not survive a power loss. The rollback V1 document already says that
"File/directory fsync acknowledgments are not a power-loss certification" (around line 118). Using `F_FULLFSYNC` is a
separate change. It would change the C++ worker and the Rust backend together, and it needs its own measurement of its
cost.

Windows has an open question of its own, for the directory-level rename, which is not reviewed here.

## Verification gaps

- `native-rollback-validation.yml` runs `tests/nativeRollbackBoundary.test.ts` on Linux, Windows and macOS. It runs on
  pushes to `main` and `stabilization/native-rollback-v1` only, not on `migration/rust`.
- `rust-shell-validation.yml` builds and tests the workspace on Windows and macOS, but no test in `noct-native` runs
  there (see "Current behaviour" above).
- No test runs the Rust worker against APFS or NTFS.

## What closing the gap needs

1. A decision on whether the Rust worker takes over the macOS and Windows backends, or whether the product keeps the
   C++ worker there until parity exists. This is the owner's decision.
2. Each backend passes the checks in the tables above, with tests that run on that platform's CI runner. A
   `cfg`-gated block that no runner executes does not count.
3. The Rust worker runs the scenarios of `tests/nativeRollbackBoundary.test.ts` on APFS and on NTFS.
4. The `UNSUPPORTED` answer stays on the platforms that have no backend.
5. The durability decision (`fsync` or `F_FULLFSYNC` on macOS, and the Windows flush semantics for renames) is
   recorded in [Native rollback V1](native-rollback-v1.md).
