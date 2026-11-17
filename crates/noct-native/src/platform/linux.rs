//! Linux backend: capability-based operations over retained directory descriptors. Port of the `__linux__` branch of
//! `native/rollback-boundary.cpp`. Every lookup goes through openat2 with `RESOLVE_BENEATH`, `RESOLVE_NO_SYMLINKS`
//! and `RESOLVE_NO_XDEV` unless it crosses a mount on purpose, and publication links the anonymous staging object
//! through its retained `/proc/self/fd` reference.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use crate::protocol::{Failure, MAX_BYTES, Outcome, require};

/// The handle that an operation holds on an open file or directory.
pub type Object = OwnedFd;

// Not exported by the libc crate for every glibc target; values from <linux/fs.h>
// and <linux/openat2.h>.
const RENAME_NOREPLACE: libc::c_uint = 1;
const RESOLVE_NO_XDEV: u64 = 0x01;
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;
const READ_CHUNK: usize = 16 * 1024;

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

fn errno() -> libc::c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Failure for the errno that the last syscall left behind.
fn classify_errno() -> Failure {
    match errno() {
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
        Err(classify_errno())
    }
}

/// Takes ownership of a descriptor returned by a syscall, or reports its failure.
fn own(result: libc::c_int) -> Outcome<OwnedFd> {
    let fd = checked(result)?;
    // SAFETY: `checked` accepted a non-negative descriptor freshly returned by the kernel,
    // and nothing else holds it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn cstr(bytes: &[u8]) -> Outcome<CString> {
    CString::new(bytes).map_err(|_| Failure::new("CONFLICT", "Invalid path bytes"))
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

/// Resolves `name` beneath `parent` through openat2 constraints. Returns the raw result
/// so callers can inspect errno, matching the C++ helper.
fn constrained(parent: RawFd, name: &CString, directory: bool, crossing: bool) -> libc::c_int {
    let how = OpenHow {
        flags: (libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            }) as u64,
        mode: 0,
        resolve: RESOLVE_BENEATH
            | RESOLVE_NO_SYMLINKS
            | RESOLVE_NO_MAGICLINKS
            | if crossing { 0 } else { RESOLVE_NO_XDEV },
    };
    // SAFETY: `name` is NUL-terminated and `how` outlives the call; the kernel validates both.
    unsafe {
        libc::syscall(
            libc::SYS_openat2,
            parent,
            name.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        ) as libc::c_int
    }
}

/// Opens an absolute root path. The root may sit on a mount of its own, so the walk from `/` may cross.
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
    let name = cstr(&path[1..])?;
    own(constrained(filesystem_root.as_raw_fd(), &name, true, true))
}

/// A directory beneath `root`, without crossing a mount.
pub fn walk(root: &Object, relative: &[u8]) -> Outcome<Object> {
    own(constrained(root.as_raw_fd(), &cstr(relative)?, true, false))
}

/// The entry `name` beneath `parent`, or `None` when it does not exist. The Linux backend needs no access
/// beyond reading, so `renameable` does not change the open.
pub fn child(parent: &Object, name: &[u8], _renameable: bool) -> Outcome<Option<Object>> {
    let raw = constrained(parent.as_raw_fd(), &cstr(name)?, false, false);
    if raw >= 0 {
        return Ok(Some(own(raw)?));
    }
    if errno() == libc::ENOENT {
        return Ok(None);
    }
    Err(classify_errno())
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
    Ok(stat_of(object.as_raw_fd())?.st_mode as u32)
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

/// `fsync`, the durability acknowledgment of the C++ worker. An error is reported as `UNKNOWN`.
pub fn flush(object: &Object) -> Outcome<()> {
    // SAFETY: fsync on a live descriptor.
    if unsafe { libc::fsync(object.as_raw_fd()) } != 0 {
        return Err(Failure::new(
            "UNKNOWN",
            "Durability acknowledgment unavailable",
        ));
    }
    Ok(())
}

/// The directory form of [`flush`]: the entries of a directory reach the disk with its `fsync`.
pub fn flush_directory(directory: &Object) -> Outcome<()> {
    flush(directory)
}

/// The local-filesystem gate of `INIT`: the workspace root must be on tmpfs, ext4 or Btrfs.
pub fn check_filesystem(root: &Object) -> Outcome<()> {
    // SAFETY: an all-zero statfs is a valid output buffer.
    let mut filesystem: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: live descriptor and valid out-pointer.
    require(
        unsafe { libc::fstatfs(root.as_raw_fd(), &mut filesystem) } == 0,
        "Filesystem unavailable",
    )?;
    let supported = [
        libc::TMPFS_MAGIC,
        libc::EXT4_SUPER_MAGIC,
        libc::BTRFS_SUPER_MAGIC,
    ];
    if !supported.contains(&(filesystem.f_type as libc::c_long)) {
        return Err(Failure::new(
            "UNSUPPORTED",
            "V1 requires local tmpfs, ext4 or Btrfs",
        ));
    }
    Ok(())
}

/// Checks a relative path of `OPEN` and splits it into its directory and its leaf. The directory of a leaf in the
/// root is `.`. A component that is empty, `.`, `..`, or protected is refused, and so is a trailing slash.
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
        // An empty component is refused here, with the other refusals of this loop. The C++ worker drops a
        // trailing `/` before it checks the parts, so it accepts `parent/` and treats the empty leaf as absent;
        // the port refuses that path instead (see docs/rust-native-rollback-backends.md).
        let protected = part.is_empty()
            || part == &b"."[..]
            || part == &b".."[..]
            || part == &b".git"[..]
            || part == &b".nocturne"[..]
            || part.starts_with(b".nocturne-rollback-");
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

/// Creates the staged object: an anonymous file with no name anywhere in the namespace. It is written, given the
/// requested permission bits, flushed and read back. Its name is empty, which the contract allows on Linux.
pub fn stage(parent: &Object, bytes: &[u8], mode: u32) -> Outcome<(Object, Vec<u8>)> {
    // SAFETY: "." is a literal; the parent descriptor is live.
    let staged = own(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c".".as_ptr(),
            libc::O_TMPFILE | libc::O_RDWR | libc::O_CLOEXEC,
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
    Ok((staged, Vec::new()))
}

/// The mode that a published file must have, for the mode that its stage was given.
pub fn produced_mode(requested: u32) -> u32 {
    requested & 0o777
}

/// Publishes the staged object as `leaf` under `parent`. The retained `/proc/self/fd` reference selects the anonymous
/// source: no temporary name, no `AT_EMPTY_PATH` privilege and no fallback. The published entry is the same object,
/// and that is verified here.
pub fn publish(staged: &Object, parent: &Object, leaf: &[u8], _mode: u32) -> Outcome<()> {
    let source = cstr(format!("/proc/self/fd/{}", staged.as_raw_fd()).as_bytes())?;
    checked(unsafe {
        libc::linkat(
            libc::AT_FDCWD,
            source.as_ptr(),
            parent.as_raw_fd(),
            cstr(leaf)?.as_ptr(),
            libc::AT_SYMLINK_FOLLOW,
        )
    })?;
    let produced = child(parent, leaf, false)?
        .ok_or(Failure::new("CONFLICT", "Filesystem operation refused"))?;
    require(
        identity(&produced)? == identity(staged)?,
        "Published source identity diverged",
    )
}

/// Moves `leaf` to `to` under `parent` without replacing anything. The source is named, and no compare-and-swap on
/// its inode is claimed: `RENAME_NOREPLACE` only refuses to overwrite.
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
        libc::syscall(
            libc::SYS_renameat2,
            parent.as_raw_fd(),
            leaf.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
            RENAME_NOREPLACE,
        ) as libc::c_int
    })?;
    Ok(())
}
