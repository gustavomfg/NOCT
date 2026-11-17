//! macOS backend: the `__APPLE__` branch of `native/rollback-boundary.cpp`, for local APFS volumes only. Lookups go
//! through openat with `O_NOFOLLOW` from retained directory descriptors, and a crossing into another device is refused.
//! Publication is a controlled clone of the staged descriptor, not a link to its inode: the staged name is never used
//! again, and the clone is verified by its bytes and mode.
//!
//! Durability is `F_FULLFSYNC`, which asks the drive to flush its cache. The C++ worker uses `fsync`, which does not;
//! the difference between the two workers is recorded in `docs/rust-native-rollback-backends.md`.

use std::ffi::CStr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::protocol::{Failure, MAX_BYTES, Outcome, require};

/// The handle that an operation holds on an open file or directory.
pub type Object = OwnedFd;

/// `CLONE_NOOWNERCOPY` of `<sys/clonefile.h>`: the clone is owned by the caller, not copied from the source.
const CLONE_NOOWNERCOPY: u32 = 0x0002;
const READ_CHUNK: usize = 16 * 1024;

fn errno() -> libc::c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Failure for an errno: the primitives the backend needs are either present or the operation is refused.
fn classify(error: libc::c_int) -> Failure {
    match error {
        libc::ENOSYS | libc::EOPNOTSUPP | libc::EXDEV | libc::EINVAL => {
            Failure::new("UNSUPPORTED", "Required native primitive unavailable")
        }
        _ => Failure::new("CONFLICT", "Filesystem operation refused"),
    }
}

fn checked(result: libc::c_int) -> Outcome<libc::c_int> {
    if result >= 0 {
        Ok(result)
    } else {
        Err(classify(errno()))
    }
}

/// Takes ownership of a descriptor returned by a syscall, or reports its failure.
fn own(result: libc::c_int) -> Outcome<OwnedFd> {
    let fd = checked(result)?;
    // SAFETY: `checked` accepted a non-negative descriptor freshly returned by the kernel,
    // and nothing else holds it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn cstr(bytes: &[u8]) -> Outcome<std::ffi::CString> {
    std::ffi::CString::new(bytes).map_err(|_| Failure::new("CONFLICT", "Invalid path bytes"))
}

fn stat_of(fd: RawFd) -> Outcome<libc::stat> {
    // SAFETY: an all-zero `stat` is a valid output buffer for fstat.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is a live descriptor held by the caller; `st` is a valid out-pointer.
    require(
        unsafe { libc::fstat(fd, &mut st) } == 0,
        "Identity unavailable",
    )?;
    Ok(st)
}

fn duplicate(fd: RawFd) -> Outcome<OwnedFd> {
    own(unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) })
}

/// The outcome of a lookup: the object, or the fact that its final name does not exist.
enum Lookup {
    Found(OwnedFd),
    Absent,
}

/// Walks `relative` from `start`, one component at a time, each opened `O_NOFOLLOW` from the one before. A crossing
/// into another device is refused unless `crossing` is set. Only the final component may be absent, which is a value
/// and not a failure. Port of the `constrained` helper of the C++ worker.
fn lookup(start: RawFd, relative: &[u8], directory: bool, crossing: bool) -> Outcome<Lookup> {
    if relative == b"." {
        return Ok(Lookup::Found(duplicate(start)?));
    }
    require(
        !relative.is_empty() && relative[0] != b'/' && !relative.contains(&0),
        "Invalid relative acquisition",
    )?;
    let device = stat_of(start)?.st_dev;
    let mut current = duplicate(start)?;
    let parts: Vec<&[u8]> = relative.split(|&byte| byte == b'/').collect();
    for (index, part) in parts.iter().enumerate() {
        require(
            !part.is_empty() && *part != b"." && *part != b"..",
            "Invalid component",
        )?;
        let final_part = index + 1 == parts.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if !final_part || directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        let name = cstr(part)?;
        // SAFETY: `name` is NUL-terminated and `current` is a live descriptor.
        let raw = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if raw < 0 {
            if final_part && errno() == libc::ENOENT {
                return Ok(Lookup::Absent);
            }
            return Err(classify(errno()));
        }
        // SAFETY: `raw` is a non-negative descriptor freshly returned by openat.
        let next = unsafe { OwnedFd::from_raw_fd(raw) };
        if !crossing && stat_of(next.as_raw_fd())?.st_dev != device {
            return Err(classify(libc::EXDEV));
        }
        current = next;
    }
    Ok(Lookup::Found(current))
}

/// Opens an absolute root path. The root may sit on a volume of its own, so the walk from `/` may cross.
pub fn acquire_root(path: &[u8]) -> Outcome<Object> {
    require(
        path.len() > 1 && path[0] == b'/' && !path.contains(&0),
        "Invalid root",
    )?;
    // SAFETY: literal NUL-terminated path.
    let filesystem_root = own(unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    match lookup(filesystem_root.as_raw_fd(), &path[1..], true, true)? {
        Lookup::Found(root) => Ok(root),
        Lookup::Absent => Err(Failure::new("CONFLICT", "Filesystem operation refused")),
    }
}

/// A directory beneath `root`, without crossing a volume.
pub fn walk(root: &Object, relative: &[u8]) -> Outcome<Object> {
    match lookup(root.as_raw_fd(), relative, true, false)? {
        Lookup::Found(directory) => Ok(directory),
        Lookup::Absent => Err(Failure::new("CONFLICT", "Filesystem operation refused")),
    }
}

/// The entry `name` beneath `parent`, or `None` when it does not exist. APFS has no access that the publication
/// needs beyond reading, so `renameable` does not change the open.
pub fn child(parent: &Object, name: &[u8], _renameable: bool) -> Outcome<Option<Object>> {
    match lookup(parent.as_raw_fd(), name, false, false)? {
        Lookup::Found(entry) => Ok(Some(entry)),
        Lookup::Absent => Ok(None),
    }
}

/// A new private read-write file, which must not exist yet and must not be a link.
pub fn create_new(parent: &Object, name: &[u8]) -> Outcome<Object> {
    own(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            cstr(name)?.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    })
}

/// The identity that binds an entry: its device and inode numbers.
pub fn identity(object: &Object) -> Outcome<String> {
    let st = stat_of(object.as_raw_fd())?;
    Ok(format!("{}:{}", st.st_dev as u64, st.st_ino as u64))
}

/// The mode bits of an entry, as `stat` reports them, including the file type.
pub fn mode_of(object: &Object) -> Outcome<u32> {
    Ok(u32::from(stat_of(object.as_raw_fd())?.st_mode))
}

/// The whole content of a regular file, within the rollback budget.
pub fn read_all(object: &Object) -> Outcome<Vec<u8>> {
    let fd = object.as_raw_fd();
    require(
        (stat_of(fd)?.st_mode & libc::S_IFMT) == libc::S_IFREG,
        "Only regular files are supported",
    )?;
    let mut data = Vec::new();
    let mut buffer = vec![0u8; READ_CHUNK];
    let mut offset: libc::off_t = 0;
    loop {
        // SAFETY: `buffer` is valid for `buffer.len()` bytes.
        let read = unsafe { libc::pread(fd, buffer.as_mut_ptr().cast(), buffer.len(), offset) };
        if read < 0 && errno() == libc::EINTR {
            continue;
        }
        require(read >= 0, "Read failed")?;
        if read == 0 {
            break;
        }
        let read = read as usize;
        require(
            data.len() + read <= MAX_BYTES,
            "File exceeds rollback budget",
        )?;
        data.extend_from_slice(&buffer[..read]);
        offset += read as libc::off_t;
    }
    Ok(data)
}

pub fn write_all(object: &Object, bytes: &[u8]) -> Outcome<()> {
    let fd = object.as_raw_fd();
    let mut offset = 0;
    while offset < bytes.len() {
        // SAFETY: the slice is valid for the length passed.
        let written =
            unsafe { libc::write(fd, bytes[offset..].as_ptr().cast(), bytes.len() - offset) };
        if written < 0 && errno() == libc::EINTR {
            continue;
        }
        require(written > 0, "Write failed")?;
        offset += written as usize;
    }
    Ok(())
}

/// Empties a file and moves its position to the start, so the next write replaces its content.
pub fn rewind_truncate(object: &Object) -> Outcome<()> {
    let fd = object.as_raw_fd();
    // SAFETY: live descriptor; plain lseek/ftruncate on a file the operation holds.
    let rewound = unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } == 0
        && unsafe { libc::ftruncate(fd, 0) } == 0;
    require(rewound, "Journal snapshot unavailable")
}

/// `F_FULLFSYNC`: the drive is asked to flush its cache, which `fsync` does not do on macOS. An error is `UNKNOWN`.
pub fn flush(object: &Object) -> Outcome<()> {
    // SAFETY: fcntl on a live descriptor with a command that takes no argument.
    if unsafe { libc::fcntl(object.as_raw_fd(), libc::F_FULLFSYNC) } != 0 {
        return Err(Failure::new(
            "UNKNOWN",
            "Durability acknowledgment unavailable",
        ));
    }
    Ok(())
}

/// The directory form of [`flush`]: the entries of a directory reach the disk with its full flush.
pub fn flush_directory(directory: &Object) -> Outcome<()> {
    flush(directory)
}

/// The local-filesystem gate of `INIT`: the workspace root must be on a local APFS volume.
pub fn check_filesystem(root: &Object) -> Outcome<()> {
    // SAFETY: an all-zero statfs is a valid output buffer.
    let mut filesystem: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: live descriptor and valid out-pointer.
    require(
        unsafe { libc::fstatfs(root.as_raw_fd(), &mut filesystem) } == 0,
        "Filesystem unavailable",
    )?;
    // SAFETY: `f_fstypename` is a NUL-terminated name that the kernel fills.
    let kind = unsafe { CStr::from_ptr(filesystem.f_fstypename.as_ptr()) };
    if kind.to_bytes() != b"apfs" || (filesystem.f_flags & libc::MNT_LOCAL as u32) == 0 {
        return Err(Failure::new("UNSUPPORTED", "V1 requires local APFS"));
    }
    Ok(())
}

/// Checks a relative path of `OPEN` and splits it into its directory and its leaf. The check of the protected names
/// ignores ASCII case, because APFS commonly resolves case aliases of a name to the same entry, even when it is
/// case-sensitive. The directory of a leaf in the root is `.`.
pub fn split_relative(relative: &[u8]) -> Outcome<(Vec<u8>, Vec<u8>)> {
    require(
        !relative.is_empty()
            && relative.len() < 4096
            && relative[0] != b'/'
            && !relative.contains(&0)
            && !relative.contains(&b'\\'),
        "Invalid relative path",
    )?;
    for part in relative.split(|&byte| byte == b'/') {
        let folded = part.to_ascii_lowercase();
        let protected = folded.is_empty()
            || folded == b"."
            || folded == b".."
            || folded == b".git"
            || folded == b".nocturne"
            || folded.starts_with(b".nocturne-rollback-");
        require(!protected, "Protected path")?;
    }
    Ok(match relative.iter().rposition(|&byte| byte == b'/') {
        None => (b".".to_vec(), relative.to_vec()),
        Some(slash) => (relative[..slash].to_vec(), relative[slash + 1..].to_vec()),
    })
}

/// The name that a displaced entry is kept under: a retention entry of this worker, in the same directory.
pub fn validate_retention(name: &[u8]) -> Outcome<()> {
    require(
        name.starts_with(b".nocturne-rollback-") && !name.contains(&b'/') && !name.contains(&0),
        "Invalid retention entry",
    )
}

/// Creates the staged object under a name of this worker's own. Only the descriptor selects it afterwards: the name
/// is never unlinked, renamed or reopened, and it stays in the directory, as the C++ worker leaves it.
pub fn stage(parent: &Object, bytes: &[u8], mode: u32) -> Outcome<(Object, Vec<u8>)> {
    static SERIAL: AtomicU32 = AtomicU32::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed) + 1;
    let name = format!(".nocturne-rollback-stage-{}-{serial}", std::process::id()).into_bytes();
    let staged = own(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            cstr(&name)?.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    })?;
    write_all(&staged, bytes)?;
    // SAFETY: live descriptor.
    require(
        unsafe { libc::fchmod(staged.as_raw_fd(), (mode & 0o777) as libc::mode_t) } == 0,
        "Stage mode failed",
    )?;
    flush(&staged)?;
    require(read_all(&staged)? == bytes, "Staged bytes changed")?;
    Ok((staged, name))
}

/// The mode that a published file must have, for the mode that its stage was given.
pub fn produced_mode(requested: u32) -> u32 {
    requested & 0o777
}

/// Publishes a controlled clone of the staged descriptor as `leaf` under `parent`. The clone is a new object, not the
/// staged inode, so there is no identity to compare: the bytes and the mode are verified by the caller.
pub fn publish(staged: &Object, parent: &Object, leaf: &[u8], _mode: u32) -> Outcome<()> {
    let leaf = cstr(leaf)?;
    checked(unsafe {
        libc::fclonefileat(
            staged.as_raw_fd(),
            parent.as_raw_fd(),
            leaf.as_ptr(),
            CLONE_NOOWNERCOPY,
        )
    })?;
    Ok(())
}

/// Moves `leaf` to `to` under `parent` without replacing anything. The source is named, and no compare-and-swap on
/// its inode is claimed: `RENAME_EXCL` only refuses to overwrite.
pub fn displace(
    parent: &Object,
    leaf: &[u8],
    _observed: Option<&Object>,
    to: &[u8],
) -> Outcome<()> {
    let leaf = cstr(leaf)?;
    let to = cstr(to)?;
    // SAFETY: both names are NUL-terminated and relative to the live parent descriptor.
    checked(unsafe {
        libc::renameatx_np(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_EXCL,
        )
    })?;
    Ok(())
}
