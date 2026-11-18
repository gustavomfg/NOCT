# Rust rollback backends: state and gaps

- **Status:** implemented in `crates/noct-native` for Linux, macOS (local APFS) and Windows (local fixed NTFS). Each
  backend fails closed on any other filesystem or volume. The Electron product still runs the C++ worker,
  `native/rollback-boundary.cpp`, on every platform; the Rust worker is not wired into the product.
- **Verified:** the Linux backend locally, with its tests and a differential run against the C++ worker. The macOS
  and Windows backends are type-checked and linted for their targets locally, and their protocol tests pass on the
  GitHub runners (`windows-latest` on NTFS, `macos-latest` on APFS) in
  `rust-shell-validation.yml`, run `44124578455` on commit `7b4826a`. They have not been run against the C++ worker
  there, and not on a user's volume.
- **Compared:** `native/rollback-boundary.cpp` (the Linux and Apple branches and the protocol) and
  `native/rollback-windows.h` (the Windows branch).
- **Related:** [Native rollback V1](native-rollback-v1.md) describes the contract. [Compatibility](compatibility.md)
  lists the platforms.

## Layout

- `operation.rs`: the protocol state machine, the same on every system. It holds the bindings, checks them before every
  command, and revokes the operation after any failure.
- `platform/linux.rs`, `platform/macos.rs`, `platform/windows.rs`: the system calls, one module per system with the same
  items. Each module also carries that system's refusals, so the answers match the matching C++ branch.
- `tests/rollback_protocol.rs`: the protocol scenarios, on real files in a scratch directory. The expectations that
  differ by system are gated with `cfg`.
- Any other system: `lib.rs` answers every line with `ERR\tUNSUPPORTED` until `CLOSE`.

## Linux

Port of the Linux branch. Lookups use `openat2` with `RESOLVE_BENEATH`, `RESOLVE_NO_SYMLINKS`,
`RESOLVE_NO_MAGICLINKS` and `RESOLVE_NO_XDEV` (unless the root itself crosses a mount). Staging uses `O_TMPFILE`, and
publication links the anonymous object through `/proc/self/fd`. Displacement uses `renameat2` with `RENAME_NOREPLACE`.
Durability is `fsync`. Local `tmpfs`, `ext4` or `Btrfs` only.

Checked locally: 12 protocol tests pass; the differential against the C++ worker (built with `g++` from
`native/rollback-boundary.cpp`) gives the same responses and the same final tree in 35 of 36 scenarios. The one
difference is intended, see below.

## macOS

Port of the Apple branch, for local APFS volumes (`f_fstypename == "apfs"` and `MNT_LOCAL`).

| Area | Rust backend | C++ worker |
| --- | --- | --- |
| Lookup | `openat` per component with `O_NOFOLLOW`, from retained descriptors; a crossing into another device is refused | Same |
| Protected names | Compared without ASCII case, on Apple only | Same (lines 132–136) |
| Staging | Named file `.nocturne-rollback-stage-<pid>-<serial>`, `O_EXCL`, `O_NOFOLLOW`; only its descriptor selects it afterwards | Same |
| Displacement | `renameatx_np` with `RENAME_EXCL` | Same |
| Publication | `fclonefileat` with `CLONE_NOOWNERCOPY`; the clone is checked by its bytes and mode, not by inode | Same |
| Durability | `fcntl(F_FULLFSYNC)` | `fsync` |

**Durability decision.** The Rust backend uses `F_FULLFSYNC`, as the owner decided. On macOS, `fsync` does not ask the
drive to flush its cache, so an acknowledged write may not survive a power loss. The C++ worker, which the product runs,
still uses `fsync`. Changing it is a change to the product path, and it is not part of this branch. Until it is made, the
two workers differ in this one respect.

## Windows

Port of `native/rollback-windows.h`. Every lookup is an `NtCreateFile` relative to a retained directory handle. The
NT entry points are resolved from `ntdll.dll` with `GetProcAddress`; a missing one is `UNSUPPORTED`. Reparse points are
refused at every step. The root is a fixed NTFS volume, opened as `\\?\X:\` and walked from there. Identity is the
volume serial number and the two halves of the file index, in the format the TypeScript client compares.

- **Custody (`BIND`).** `BIND` binds a root and answers its identity. The TypeScript client uses it through a holder
  process that keeps the root open while an action runs. The holder's operation never becomes active.
- **Publication.** The staged handle is renamed exclusively (`NtSetInformationFile`, `FileRenameInformation`, no
  replacement). The published entry is verified to be the same object.
- **Modes.** Staging needs a writable regular file, as in C++. The published mode is checked as `0666`.
- **Dependency.** `windows-sys` 0.61.2, pinned, in `noct-native` only, for the Windows target only. Features:
  `Win32_Foundation`, `Win32_Security`, `Win32_Storage_FileSystem`, `Win32_System_IO`, `Win32_System_LibraryLoader`,
  `Win32_System_Threading`, `Win32_System_WindowsProgramming`. The NT structures (`OBJECT_ATTRIBUTES`, `IO_STATUS_BLOCK`,
  `UNICODE_STRING`, the rename record) are declared in `platform/windows.rs` with their layouts, so the `Wdk` feature set
  stays off.

Differences from the C++ header, all in the stricter direction or in wording:

- `DISPLACE` also compares the mode of the displaced entry. The C++ header compares identity and bytes only.
- When an observed entry cannot be reopened for verification, the message is `Windows filesystem operation refused`,
  where C++ says `Observed AFTER changed`. The code is `CONFLICT` in both.
- No directory flush, as in C++. See the open decisions.

## Differences that apply to every system

- **Trailing slash.** The C++ worker drops a trailing `/` and treats the empty leaf as absent, so `OPEN parent/`
  answers `DATA 0`. The Rust worker refuses it with `CONFLICT`. This is intended: an empty name is not a file.
- **Numbers.** The Rust worker parses the mode field strictly, and refuses a value with a non-digit tail. C++
  `std::stoul` accepts a prefix. The client sends digits only.

## Open decisions for the owner

1. **Product durability on macOS.** Should the C++ product worker also use `F_FULLFSYNC`? This changes the shipped
   path, so it needs its own change and review.
2. **Directory flush on Windows.** The C++ header flushes no directory after a rename or a create. Adding one is a
   durability decision, not a port detail.

## Verification

| Check | Linux | macOS (APFS) | Windows (NTFS) |
| --- | --- | --- | --- |
| `cargo check` and `cargo clippy -D warnings` for the target | Passed locally | Passed locally (`aarch64-apple-darwin`) | Passed locally (`x86_64-pc-windows-msvc`) |
| Protocol tests (`tests/rollback_protocol.rs`) | Passed locally, 12 tests; passed in CI (`ubuntu-latest`) | Passed in CI (`macos-latest`), 13 tests with the APFS case test | Passed in CI (`windows-latest`), 12 tests |
| Binary tests (`tests/rollback_binary.rs`, the child process) | Passed locally, 4 tests; passed in CI | Passed in CI, 4 tests | Passed in CI, 4 tests |
| Differential against the C++ worker | 35 of 36 scenarios identical; the difference is the trailing slash | Not run; the C++ worker was not built on macOS | Not run; the C++ header needs MSVC |
| Volume of the test run | `tmpfs` or `ext4` on the runner | APFS, the runner's temporary directory | NTFS, the runner's temporary directory |
| A user's volume | No | No | No |

`native-rollback-validation.yml` runs the TypeScript suite `tests/nativeRollbackBoundary.test.ts` on Linux, Windows and
macOS. It runs on pushes to `main` and `stabilization/native-rollback-v1` only, so it does not run on `migration/rust`.
That suite exercises the C++ worker, not the Rust one.

## What remains before the Rust worker can replace the C++ worker

1. The owner decides the two open decisions above.
2. The differential runs on macOS and Windows, with the C++ worker built there in CI.
3. The TypeScript suite runs against the Rust worker on each system, on APFS and NTFS.
4. A real restore of a file on a user's APFS and NTFS volumes, not only the runner's temporary directory.
