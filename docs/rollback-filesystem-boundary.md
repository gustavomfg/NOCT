# Rollback Filesystem Boundary and Concurrency Contract

## Status

Architecture decision for a **future implementation**, based on HEAD
`e6a1c2f66376925092fe0701875eeabd06c6f384`. No new boundary is implemented by
this document. The original R1 continuous-path-membership invariant is **not
satisfied**, and Stabilization Batch 5 remains blocked. Implementing this
contract and passing its tests must not be described as proving the original,
stronger R1 invariant. The product/security contract must explicitly adopt this
distinction before a future readiness decision.

## Context

The current rollback verifies BEFORE/AFTER and workspace paths, but subsequently
uses pathnames for mkdir, displacement, temporary creation, chmod, link, unlink,
and journal replacement. A deterministic parent-symlink swap on Linux causes
real external mutation before the service reports `conflicted`. Internal queues
and `WorkspaceChangeGate` do not lock filesystem structure against other
processes. The gate holds notifications, not namespace mutations.

Two properties are different:

1. A pathname cannot redirect a mutation to a different, unauthorized directory.
2. A previously authorized directory cannot receive mutations after another
   process moves it outside the root's current tree.

Directory descriptors can support the first property, but not the second.
Linux experiments with actual `openat2`, `renameat2`, `linkat`, `unlinkat`, and
`mkdirat` kept a symlink swap from touching a foreign directory. Another process
could nevertheless move the opened directory outside the workspace; creation
and rename through its unchanged descriptor then affected that moved directory.
An advisory lock did not prevent the move. Even Landlock permitted a write
through a file descriptor opened under its rules before an unsandboxed process
moved the parent, while rejecting a new write-open after the move.

These experiments used Linux x64, kernel 7.2.8, UID 1000, and tmpfs. They are
primitive experiments, not native-addon, power-loss, macOS, or Windows
certification. Descriptor stability is also documented by [Linux open(2)](https://man7.org/linux/man-pages/man2/open.2.html).

## Threat Model

Accidental, cooperative, and non-cooperative project-tree concurrency are in
scope. A same-user adversary's attempts to substitute project path components
are also in scope **for redirection containment**, not continuous namespace
membership or isolation from that user's entire authority.

An adversary can move an authorized directory or create additional hardlink
aliases. Neither a private mode nor a descriptor removes that authority.
Compromise of the main process, modification of private application state,
debugger injection, privileged mount manipulation, malicious filesystem/server
behavior, and a root/Administrator adversary are outside the guarantee. This
boundary is not an OS sandbox or a protection of private data from its owner.

Cooperative locks may improve operational stability; ignoring them cannot
weaken the redirection-containment guarantee. Watcher delivery is not a
pre-mutation security primitive.

## Decision

Adopt a **short-lived, operation-scoped directory capability contract**, combined
with copy-on-replace staging, exclusive publication, conservative structural
conflicts, and a durable per-step journal. Use a minimal platform-native boundary
for filesystem mutations; do not implement it with additional pathname checks.

This is an explicit object/entry authorization, not permission to mutate
whatever a previously checked absolute pathname later resolves to. The authorized
unit is a retained directory object plus a single entry name, allowed operation,
execution/ChangeSet/checkpoint identities, expected AFTER state, and intended
BEFORE state. New staging objects require separately retained identities.

The root has an authorized live handle and an identity recorded with the
operation. Its pathname is a location label. Opening a different root at the
same pathname must not silently replace that authorization. A persistent
device/file ID is diagnostic identity, not a reusable capability; restart
requires fresh rollback authorization and capability acquisition. This does not
silently change authorization for unrelated workspace reads. A moved or replaced
root requires explicit workspace reselection. Existing checkpoint
hashes do not retroactively prove root or parent identity.

### Membership over time

- T0: authorize the workspace and the exact rollback decision.
- T1: acquire constrained root/parent capabilities and record their observations.
- T2: admit one bounded primitive after preparation and current checks.
- T3: execute through retained capabilities without re-traversing project paths.
- T4: verify and record the observed result and structural binding.

If a parent is moved outside between T1 and T2 without that loss being observed,
the capability still identifies it and an admitted operation **may affect it**.
This is permitted by this narrower authorization and is **not** continuous
current-tree confinement. The same applies if a move races an already-issued
primitive. Additional checks cannot remove this limitation.

Once a changed path binding, lost membership, revoked authorization, or unknown
required structural binding is known to the operation owner, it must admit no
further workspace mutations, including compensation and temporary cleanup.
Already-issued kernel operations cannot be promised retractable. Private-journal
recording may continue. Do not follow a new symlink, search for the moved object,
or resume mutations merely because its old handle is still usable.

### Deliberate restrictions

The first implementation restores regular-file content and file-level
create/delete/rename only through already acquired parent directories. It must
not recursively recreate a missing directory hierarchy, restore symlinks/reparse
points, cross unverified mounts/volumes, or use a weaker pathname fallback.
Missing parents and unsupported platform/filesystem capabilities require an
explicit conflict/unsupported result and retained bytes for manual recovery.
These are visible contract changes, not a silent disabling of existing rollback.

Do not write, truncate, or chmod an existing user inode in place. Publish a
verified newly created inode and retain displaced objects. This avoids changing
the contents/mode of external hardlink aliases, but does not promise invariant
link counts, immutable displaced bytes, or control of other processes' open FDs.

## Invariants

1. **No authority by string reuse.** Every mutation operand is an acquired
   directory capability and validated single entry, or a retained staging file
   capability. Reject absolute names, separators, `..`, stream/device names,
   symlinks/reparse traversal, and unsupported crossings. Source, destination,
   recovery, metadata updates, and cleanup all require the same discipline.
2. **No silent authority substitution.** Replacing a root/parent pathname cannot
   replace an admitted capability with a different object. Never reacquire by
   that pathname after a boundary conflict without fresh authorization.
3. **Stop on known structural interference.** Revoke further admission for the
   entire rollback when a required binding loses identity/membership or cannot
   be verified. This is a temporal application guarantee, not prevention of an
   undetected move or reversal of an in-flight syscall.
4. **Preserve competing data.** Exclusive publication must not overwrite a
   recreated target. Displacement is not an inode/hash compare-and-swap:
   preserve and inspect the actual displaced object; mismatch stops publication
   and becomes conflict. Never discard it because an earlier check matched.
5. **Truthful outcome.** Only verified filesystem steps plus committed decision
   persistence allow a successful decision. Partial, interrupted, prevented,
   and unconfirmed outcomes remain distinguishable and non-successful.
6. **Recovery without old authority.** Persist intent before effects and results
   after verification. After crash, neither an inode number nor an old path
   grants authority to replay or compensate an unfinished step.

## Platform Semantics

### Linux

Use constrained root-relative acquisition (`openat2` with suitable beneath,
no-symlink/no-magic-link and crossing constraints), then retained dirfds with
single-entry `*at` mutations and fd-based metadata updates. Acquisition errors
fail closed; `RESOLVE_IN_ROOT` is not a permanent jail, and `O_NOFOLLOW` alone
does not constrain intermediate components. See [openat2(2)](https://man7.org/linux/man-pages/man2/openat2.2.html).

Destination no-replace operations were exercised, but do not supply conditional
source-inode/hash mutation or a multi-step transaction. The staging publication
primitive must also preserve source identity; a held fd plus a replaceable
temporary basename is insufficient. A tmpfs experiment published an anonymous
`O_TMPFILE` object through its retained `/proc/self/fd` reference with `linkat`,
after a parent swap, as UID 1000; the published identity matched and the foreign
directory remained unchanged. This verifies that isolated variant only, not
support across filesystems or packaged environments. `AT_EMPTY_PATH` has a
documented privilege requirement; do not depend on elevated privileges or
silently switch to a named, raceable source. See [linkat(2)](https://man7.org/linux/man-pages/man2/linkat.2.html).

No claim covers unverified kernel/filesystem variants, mount changes, network
filesystems, or a parent moved outside after capability acquisition. No Linux
native implementation exists in the product yet.

### macOS

Investigate component-safe acquisition, `openat`/`renameat`/`linkat`/`unlinkat`,
fd metadata operations, and supported no-replace semantics. XNU exposes these
families, but source availability is not runtime verification. A chain of
component FDs is not proof of simultaneous ancestry. Anonymous/named staging,
source-identity protection, APFS behavior, and move races require real tests.
See [Apple XNU syscall definitions](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/syscalls.master).

### Windows

Investigate `NtCreateFile` relative to retained directory handles, per-component
reparse rejection, and rename/disposition by handle with a constrained destination.
Reject junction/reparse redirection, alternate streams and device names.
Do not treat `FILE_FLAG_OPEN_REPARSE_POINT` on the final component as protection
of all ancestors.

Share modes excluding delete may provide stronger structural pinning. Oplocks
or sharing documentation alone do not certify a complete ancestor chain,
pre-existing handles, reparse changes, or filesystem variants. This possible
extra protection is not part of the portable guarantee until tested.
See [NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile)
and [CreateFileW sharing semantics](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).

## Failure Semantics

Structural interference stops the entire operation at the next admission
boundary. Earlier independently verified file restorations remain partial;
do not automatically undo them. No subsequent files are processed. Preserve
BEFORE/AFTER and displaced data; report which effects are verified and which
are uncertain. Compensation is a new authorized mutation, never unconditional
error handling through a suspect pathname.

Distinguish: aborted before workspace effects; boundary redirection prevented;
partial rollback; interrupted; mutation result unknown; filesystem verified but
decision commit failed; and fully verified/persisted rollback. An OS error does
not universally prove no effect, particularly on remote filesystems.

Accept remains a decision over observed AFTER without restoration. Reject uses
the protected mutation path. Whole-Build rollback remains per-file and may be
partial. Hunk decisions remain annotations. No global filesystem/SQLite ACID
transaction or immutable future workspace is promised.

## Recovery Semantics

Keep authoritative operation intent/result records in the private application
recovery store, not solely in a project pathname that the same operation is
trying to constrain. Same-volume displaced-object retention and cross-volume
failure behavior require explicit design; do not fall back to lossy copy/delete.
The private recovery store is a separately authorized application-data root;
its authorization does not grant restoration access to arbitrary external
project paths. Both roots need constrained capabilities.

Each ordered step records operation/decision/execution/ChangeSet IDs,
BEFORE/AFTER IDs, backend/contract version, root/parent/staging identities,
observed relative location, expected states and hashes, intended primitive,
intent acknowledgement, syscall outcome, post-observations, retained bytes,
boundary failure, and uncertainty. Numeric fd/handle values are not recovery
capabilities. Identify any separately authorized compensation.

Persist intent before admitting a mutation. Flush data and relevant namespace
metadata according to verified platform/filesystem semantics before acknowledging
durability. Intent without a verified outcome means **possibly executed**, not
"not executed"; even matching bytes after restart do not establish causation.
Only a durably recorded pre-admission abort supports a no-effect classification
by this operation. A failed journal write must not admit the next mutation.

Restart classifies unfinished decisions conservatively, keeps pending gates and
retained data, and offers manual reconciliation. It does not restore handles,
auto-resume, reauthorize a moved workspace, or invent success. Portable content
backup remains distinct from this operational recovery state.

## Rejected Alternatives

- More `realpath`/`stat` calls: reduce a race window without eliminating it.
- FDs described as continuous current-path confinement: contradicted by rename.
- Watchers/internal queues/advisory locks as external exclusion: unsupported.
- Staging followed by ordinary pathname rename/link: leaves the commit race.
- Direct writes to existing regular files: retain relocation/hardlink hazards.
- A native addon described as a filesystem transaction: primitives do not supply it.
- Landlock/chroot/namespaces described as a proven solution to continuous
  membership: not established; descriptor writes remain a demonstrated limit.
- Silently disabling nested rollback or providing a weaker backend fallback.

Landlock's descriptor-rights behavior is documented in the [Linux kernel
userspace API](https://docs.kernel.org/userspace-api/landlock.html#rights-associated-with-file-descriptors).

## Consequences

The security promise is narrower but explicit: prevent unauthorized pathname
redirection and stop further admission after known structural interference.
It is not "no bytes outside the current pathname under arbitrary concurrent
relocation." If that stronger property remains mandatory, this architecture is
rejected and the original blocker remains without an implementable boundary.

The native surface must be small, main-process-only and operation-specific,
with no generic filesystem IPC, shell execution, downloaded arbitrary binaries,
or renderer-controlled native handles. A Node-API implementation could reduce
Node/Electron ABI coupling but still needs per-OS builds, packaging of native
binaries, lifecycle tests, reproducible toolchains and ABI/package smoke.
It must not disturb `better-sqlite3`. No dependency or native code is added here.

## Verification Plan

Before enabling any backend, require deterministic real-filesystem attacks
before acquisition, after acquisition, during displacement/publication,
metadata updates, journal writes, and cleanup. Inspect complete foreign
directory contents, bytes, existence, identities and link counts.

Test parent/root rename, move outside, symlink/junction replacement, a replaced
leaf/staging name, hardlink aliases, recreated destinations, missing parents,
unsupported crossings, and handle disposal. A move-out test must expose the
portable relocation limitation rather than falsely assert zero external effects.
Once loss is delivered, prove zero subsequent admissions/compensations.

Use real SQLite and controlled process crashes at every intent/mutation/verify/
commit seam. Prove conservative recovery, preserved displaced bytes, invalid
decision zero mutation, and commit failure non-success. Do not certify power
loss on tmpfs or infer macOS/Windows results from Linux.

Source-identity publication, same-volume recovery layout, and each non-Linux
backend require isolated prototypes before product code. Then run existing
Change Control/recovery/IPC tests, full source/renderer regressions, ABI checks
and packaged smoke on the exact candidate for each supported platform.
