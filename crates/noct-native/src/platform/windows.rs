//! Windows backend: the `_WIN32` branch of `native/rollback-boundary.cpp`, with `native/rollback-windows.h` as the
//! reference. Every lookup is an NT open relative to a retained directory handle, so ordinary Win32 path resolution is
//! never used after the root is bound. A reparse point is refused at every step, and the volume must be a fixed NTFS
//! volume. Publication renames the staged handle itself, so the name of the staged object is never trusted.
//!
//! The NT structures are declared here, not taken from `windows-sys`, so the crate needs only the Win32 features: the
//! `Wdk` feature set would pull in the driver headers for three small, stable layouts.
//!
//! Directory flushing is not done, as in the C++ worker. See `docs/rust-native-rollback-backends.md`.

use std::ffi::c_void;
use std::mem::{offset_of, size_of};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_CANT_ACCESS_FILE,
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_FUNCTION, ERROR_NOT_SAME_DEVICE, ERROR_NOT_SUPPORTED,
    FALSE, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_BEGIN, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_TYPE_DISK, FileAttributeTagInfo, FlushFileBuffers,
    GetDriveTypeW, GetFileInformationByHandle, GetFileInformationByHandleEx, GetFileSizeEx,
    GetFileType, GetVolumeInformationByHandleW, OPEN_EXISTING, ReadFile, SYNCHRONIZE, SetEndOfFile,
    SetFilePointerEx, WriteFile,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

use crate::protocol::{Failure, MAX_BYTES, Outcome, require};

/// The handle that an operation holds on an open file or directory. Closed when it is dropped.
pub struct Object(HANDLE);

impl Drop for Object {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: the handle was returned by a successful open and is closed exactly once, here.
            unsafe { CloseHandle(self.0) };
        }
    }
}

// NT ABI values, from the Windows driver headers. Named as in those headers.
const OBJ_CASE_INSENSITIVE: u32 = 0x40;
const FILE_DIRECTORY_FILE: u32 = 0x1;
const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_OPEN: u32 = 1;
const FILE_CREATE: u32 = 2;
/// FILE_RENAME_INFORMATION, the class of `NtSetInformationFile` that renames a handle relative to a directory.
const FILE_RENAME_INFORMATION: u32 = 10;
/// The permission bits of a regular file that the C++ worker requires of a published file.
const PRODUCED_PERMISSIONS: u32 = 0o666;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *const UnicodeString,
    attributes: u32,
    security_descriptor: *const c_void,
    security_quality_of_service: *const c_void,
}

#[repr(C)]
struct IoStatusBlock {
    status: usize,
    information: usize,
}

/// The layout of FILE_RENAME_INFORMATION. The first field is the union of ReplaceIfExists and Flags; zero means
/// that nothing is replaced.
#[repr(C)]
struct RenameInformation {
    flags: u32,
    root_directory: HANDLE,
    file_name_length: u32,
    file_name: [u16; 1],
}

type NtCreateFile = unsafe extern "system" fn(
    *mut HANDLE,
    u32,
    *const ObjectAttributes,
    *mut IoStatusBlock,
    *const i64,
    u32,
    u32,
    u32,
    u32,
    *const c_void,
    u32,
) -> i32;
type NtSetInformationFile =
    unsafe extern "system" fn(HANDLE, *mut IoStatusBlock, *const c_void, u32, u32) -> i32;
type RtlNtStatusToDosError = unsafe extern "system" fn(i32) -> u32;

struct NtApi {
    create: NtCreateFile,
    set_information: NtSetInformationFile,
    to_dos_error: RtlNtStatusToDosError,
}

fn unavailable() -> Failure {
    Failure::new(
        "UNSUPPORTED",
        "Required NT relative acquisition unavailable",
    )
}

fn ntdll_export(name: &'static [u8]) -> Option<unsafe extern "system" fn() -> isize> {
    let module: Vec<u16> = "ntdll.dll"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `module` and `name` are NUL-terminated and outlive the calls.
    unsafe {
        let handle = GetModuleHandleW(module.as_ptr());
        if handle.is_null() {
            return None;
        }
        GetProcAddress(handle, name.as_ptr())
    }
}

/// The NT entry points, resolved from the already-loaded ntdll. A missing one is `UNSUPPORTED`, not a fallback.
fn nt_api() -> Outcome<NtApi> {
    let create = ntdll_export(b"NtCreateFile\0").ok_or_else(unavailable)?;
    let set_information = ntdll_export(b"NtSetInformationFile\0").ok_or_else(unavailable)?;
    let to_dos_error = ntdll_export(b"RtlNtStatusToDosError\0").ok_or_else(unavailable)?;
    // SAFETY: the exports are the documented NT functions with the signatures declared above.
    Ok(unsafe {
        NtApi {
            create: std::mem::transmute::<unsafe extern "system" fn() -> isize, NtCreateFile>(
                create,
            ),
            set_information: std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                NtSetInformationFile,
            >(set_information),
            to_dos_error: std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                RtlNtStatusToDosError,
            >(to_dos_error),
        }
    })
}

/// The refusal of a failed Win32 operation. The C++ worker reports every such failure with this message.
pub fn refused() -> Failure {
    Failure::new("CONFLICT", "Windows filesystem operation refused")
}

/// A Win32 error that means the primitive is absent is `UNSUPPORTED`; any other is a refusal.
fn classify(error: u32) -> Failure {
    if matches!(
        error,
        ERROR_NOT_SUPPORTED | ERROR_INVALID_FUNCTION | ERROR_NOT_SAME_DEVICE
    ) {
        Failure::new("UNSUPPORTED", "Required native primitive unavailable")
    } else {
        refused()
    }
}

fn classify_last_error() -> Failure {
    // SAFETY: reads the calling thread's last error, with no other argument.
    classify(unsafe { GetLastError() })
}

fn utf8(bytes: &[u8]) -> Outcome<&str> {
    std::str::from_utf8(bytes).map_err(|_| Failure::new("CONFLICT", "Invalid UTF8"))
}

/// A single NTFS name: the rules of `validEntry` in the C++ header. `internal` skips the protected-name check, which only
/// applies to names that a client may name.
fn valid_entry(name: &[u8], internal: bool) -> Outcome<()> {
    let text = utf8(name)?;
    require(
        !text.is_empty()
            && name.len() < 256
            && text != "."
            && text != ".."
            && !text.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
            && !text.ends_with('.')
            && !text.ends_with(' '),
        "Invalid single entry",
    )?;
    let upper: String = text
        .chars()
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect();
    let base = upper.split('.').next().unwrap_or("");
    let device = matches!(base, "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit());
    require(!device, "Device name unsupported")?;
    if !internal {
        require(
            upper != ".GIT" && upper != ".NOCTURNE" && !upper.starts_with(".NOCTURNE-ROLLBACK-"),
            "Protected entry",
        )?;
    }
    Ok(())
}

fn by_handle(object: &Object) -> Outcome<BY_HANDLE_FILE_INFORMATION> {
    // SAFETY: an all-zero structure is a valid output buffer for this plain record.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is live and `info` is a valid out-pointer.
    require(
        unsafe { GetFileInformationByHandle(object.0, &mut info) } != 0,
        "Identity unavailable",
    )?;
    Ok(info)
}

fn volume_serial(object: &Object) -> Outcome<u32> {
    Ok(by_handle(object)?.dwVolumeSerialNumber)
}

/// The identity of an entry, in the format the TypeScript client compares: volume serial and the two halves of the
/// file index, all decimal.
pub fn identity(object: &Object) -> Outcome<String> {
    let info = by_handle(object)?;
    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

/// The mode of an entry, in the form the C++ worker reports: the regular-file type and the permission bits of its
/// read-only attribute.
pub fn mode_of(object: &Object) -> Outcome<u32> {
    let readonly = by_handle(object)?.dwFileAttributes & FILE_ATTRIBUTE_READONLY != 0;
    Ok(0o100000 | if readonly { 0o444 } else { 0o666 })
}

fn duplicate(source: &Object) -> Outcome<Object> {
    let mut target: HANDLE = INVALID_HANDLE_VALUE;
    // SAFETY: both process arguments are the current-process pseudo-handle, and `target` is a valid out-pointer.
    let ok = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            source.0,
            GetCurrentProcess(),
            &mut target,
            0,
            FALSE,
            DUPLICATE_SAME_ACCESS,
        )
    };
    require(ok != 0, "Capability duplication failed")?;
    Ok(Object(target))
}

/// Opens `name` relative to `parent`, refusing a reparse point. The outer result is a refusal of the request itself;
/// the inner one is the Win32 error of the open, so the callers decide what a missing entry means.
fn relative_open(
    parent: HANDLE,
    name: &str,
    directory: bool,
    create: bool,
    writable: bool,
    renameable: bool,
) -> Outcome<Result<Object, u32>> {
    valid_entry(name.as_bytes(), true)?;
    let api = nt_api()?;
    let units: Vec<u16> = name.encode_utf16().collect();
    let length =
        u16::try_from(units.len() * 2).map_err(|_| Failure::new("CONFLICT", "Invalid name"))?;
    let object_name = UnicodeString {
        length,
        maximum_length: length,
        buffer: units.as_ptr(),
    };
    let attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent,
        object_name: &object_name,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: null(),
        security_quality_of_service: null(),
    };
    let mut access = if directory {
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE
    } else {
        GENERIC_READ | SYNCHRONIZE
    };
    if writable {
        access |= GENERIC_WRITE;
    }
    if renameable {
        access |= DELETE;
    }
    let disposition = if create { FILE_CREATE } else { FILE_OPEN };
    let options = FILE_SYNCHRONOUS_IO_NONALERT
        | FILE_OPEN_REPARSE_POINT
        | if directory {
            FILE_DIRECTORY_FILE
        } else {
            FILE_NON_DIRECTORY_FILE
        };
    let mut handle: HANDLE = INVALID_HANDLE_VALUE;
    let mut io = IoStatusBlock {
        status: 0,
        information: 0,
    };
    // SAFETY: every pointer is valid for the call: the name buffer and `object_name` outlive it, and the output
    // handle and status block are valid out-pointers.
    let status = unsafe {
        (api.create)(
            &mut handle,
            access,
            &attributes,
            &mut io,
            null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            disposition,
            options,
            null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: a pure conversion of an NTSTATUS value.
        return Ok(Err(unsafe { (api.to_dos_error)(status) }));
    }
    let object = Object(handle);
    let mut tag = FILE_ATTRIBUTE_TAG_INFO {
        FileAttributes: 0,
        ReparseTag: 0,
    };
    // SAFETY: `tag` is a valid buffer of the size passed.
    let known = unsafe {
        GetFileInformationByHandleEx(
            object.0,
            FileAttributeTagInfo,
            (&mut tag as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if known == 0 || tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        // The object is closed when `object` drops here.
        return Ok(Err(ERROR_CANT_ACCESS_FILE));
    }
    Ok(Ok(object))
}

fn open_or_refuse(
    parent: HANDLE,
    name: &str,
    directory: bool,
    create: bool,
    writable: bool,
    renameable: bool,
) -> Outcome<Object> {
    match relative_open(parent, name, directory, create, writable, renameable)? {
        Ok(object) => Ok(object),
        Err(error) => Err(classify(error)),
    }
}

/// A directory beneath `root`. Each component is a protected-name check and a relative open, and the volume of every
/// component must be the volume of the root.
pub fn walk(root: &Object, relative: &[u8]) -> Outcome<Object> {
    let path = utf8(relative)?;
    if path == "." {
        return duplicate(root);
    }
    let mut current: Option<Object> = None;
    for part in path.split('/') {
        valid_entry(part.as_bytes(), false)?;
        let parent = current.as_ref().map_or(root.0, |object| object.0);
        let next = open_or_refuse(parent, part, true, false, false, false)?;
        require(
            volume_serial(&next)? == volume_serial(root)?,
            "Volume crossing unsupported",
        )?;
        current = Some(next);
    }
    match current {
        Some(object) => Ok(object),
        None => duplicate(root),
    }
}

/// Binds a fixed NTFS volume and the directory at `path` beneath its root. `path` is a drive path such as `C:/x`,
/// with either slash; a relative path, a network path, or a non-NTFS volume is refused.
pub fn acquire_root(path: &[u8]) -> Outcome<Object> {
    let text = utf8(path)?.replace('\\', "/");
    let bytes = text.as_bytes();
    require(
        bytes.len() > 3 && bytes[1] == b':' && bytes[2] == b'/' && bytes[0].is_ascii_alphabetic(),
        "Only local drive roots supported",
    )?;
    let drive = char::from(bytes[0]);
    let drive_root: Vec<u16> = format!("{drive}:\\")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `drive_root` is NUL-terminated and outlives the call.
    if unsafe { GetDriveTypeW(drive_root.as_ptr()) } != DRIVE_FIXED {
        return Err(Failure::new(
            "UNSUPPORTED",
            "V1 requires a fixed local NTFS volume",
        ));
    }
    let anchor: Vec<u16> = format!("\\\\?\\{drive}:\\")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `anchor` is NUL-terminated; the other arguments are constants or null, as CreateFileW allows.
    let raw = unsafe {
        CreateFileW(
            anchor.as_ptr(),
            FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(classify_last_error());
    }
    let volume = Object(raw);
    let mut filesystem = [0u16; 32];
    // SAFETY: the buffer is valid for the 32 units passed; the other output buffers are not requested.
    let known = unsafe {
        GetVolumeInformationByHandleW(
            volume.0,
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    let name_length = filesystem
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(filesystem.len());
    require_ntfs(known != 0 && String::from_utf16_lossy(&filesystem[..name_length]) == "NTFS")?;
    walk(&volume, &text.as_bytes()[3..])
}

fn require_ntfs(is_ntfs: bool) -> Outcome<()> {
    if is_ntfs {
        Ok(())
    } else {
        Err(Failure::new("UNSUPPORTED", "V1 requires NTFS"))
    }
}

/// The entry `name` beneath `parent`, or `None` when it does not exist. `renameable` asks for the access that a
/// later rename of the handle needs.
pub fn child(parent: &Object, name: &[u8], renameable: bool) -> Outcome<Option<Object>> {
    match relative_open(parent.0, utf8(name)?, false, false, false, renameable)? {
        Ok(object) => Ok(Some(object)),
        Err(error) if error == ERROR_FILE_NOT_FOUND => Ok(None),
        Err(error) => Err(classify(error)),
    }
}

/// The absence of `leaf`, which is refused by any entry or by a failed lookup other than not-found.
pub fn absence(parent: &Object, leaf: &[u8]) -> Outcome<()> {
    match child(parent, leaf, false) {
        Ok(None) => Ok(()),
        _ => Err(Failure::new("CONFLICT", "Deleted entry recreated")),
    }
}

/// A new private read-write file, which must not exist yet.
pub fn create_new(parent: &Object, name: &[u8]) -> Outcome<Object> {
    open_or_refuse(parent.0, utf8(name)?, false, true, true, false)
}

/// The content of a regular file, within the rollback budget.
pub fn read_all(object: &Object) -> Outcome<Vec<u8>> {
    let info = by_handle(object)?;
    // SAFETY: the handle is live.
    require(
        info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0
            && unsafe { GetFileType(object.0) } == FILE_TYPE_DISK,
        "Only regular files supported",
    )?;
    let mut size: i64 = 0;
    // SAFETY: the handle is live and `size` is a valid out-pointer.
    require(
        unsafe { GetFileSizeEx(object.0, &mut size) } != 0
            && (0..=MAX_BYTES as i64).contains(&size),
        "File exceeds rollback budget",
    )?;
    // SAFETY: the handle is live; a zero move with no output pointer is always valid for a disk file.
    require(
        unsafe { SetFilePointerEx(object.0, 0, null_mut(), FILE_BEGIN) } != 0,
        "Read seek failed",
    )?;
    let mut data = Vec::new();
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        let mut read: u32 = 0;
        // SAFETY: `buffer` is valid for the length passed; `read` is a valid out-pointer.
        require(
            unsafe {
                ReadFile(
                    object.0,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    &mut read,
                    null_mut(),
                )
            } != 0,
            "Read failed",
        )?;
        if read == 0 {
            break;
        }
        require(
            data.len() + read as usize <= MAX_BYTES,
            "File exceeds rollback budget",
        )?;
        data.extend_from_slice(&buffer[..read as usize]);
    }
    Ok(data)
}

pub fn write_all(object: &Object, bytes: &[u8]) -> Outcome<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        // Writes are bounded by the budget, far below the DWORD limit; the chunk stays within one write call.
        let chunk = (bytes.len() - offset).min(1 << 30) as u32;
        let mut written: u32 = 0;
        // SAFETY: the slice is valid for `chunk` bytes; `written` is a valid out-pointer.
        let ok = unsafe {
            WriteFile(
                object.0,
                bytes[offset..].as_ptr(),
                chunk,
                &mut written,
                null_mut(),
            )
        };
        require(ok != 0 && written > 0, "Write failed")?;
        offset += written as usize;
    }
    Ok(())
}

/// Empties a file and moves its position to the start, so the next write replaces its content.
pub fn rewind_truncate(object: &Object) -> Outcome<()> {
    // SAFETY: the handle is live; a zero move with no output pointer is valid.
    let moved = unsafe { SetFilePointerEx(object.0, 0, null_mut(), FILE_BEGIN) } != 0;
    // SAFETY: the handle is live and the position was moved above.
    require(
        moved && unsafe { SetEndOfFile(object.0) } != 0,
        "Snapshot truncate failed",
    )
}

/// `FlushFileBuffers`. An error is `UNKNOWN`, as in the C++ worker.
pub fn flush(object: &Object) -> Outcome<()> {
    // SAFETY: the handle is live.
    if unsafe { FlushFileBuffers(object.0) } == 0 {
        return Err(Failure::new(
            "UNKNOWN",
            "Durability acknowledgment unavailable",
        ));
    }
    Ok(())
}

/// The C++ worker does not flush a directory on Windows, so this does nothing. Whether a directory flush should be added
/// is an open decision for the owner; see `docs/rust-native-rollback-backends.md`.
pub fn flush_directory(_directory: &Object) -> Outcome<()> {
    Ok(())
}

/// The NTFS requirement is part of `acquire_root`, which refuses any other volume before the root is bound.
pub fn check_filesystem(_root: &Object) -> Outcome<()> {
    Ok(())
}

/// The check of a relative path in `OPEN`: its length, and each component under the NTFS name rules and the protected
/// names. The directory of a leaf in the root is `.`.
pub fn split_relative(relative: &[u8]) -> Outcome<(Vec<u8>, Vec<u8>)> {
    require(relative.len() < 4096, "Path budget exceeded")?;
    for part in relative.split(|&byte| byte == b'/') {
        valid_entry(part, false)?;
    }
    Ok(match relative.iter().rposition(|&byte| byte == b'/') {
        None => (b".".to_vec(), relative.to_vec()),
        Some(slash) => (relative[..slash].to_vec(), relative[slash + 1..].to_vec()),
    })
}

/// The name that a displaced entry is kept under: a retention entry of this worker, under the NTFS rules.
pub fn validate_retention(name: &[u8]) -> Outcome<()> {
    valid_entry(name, true)?;
    require(
        name.starts_with(b".nocturne-rollback-"),
        "Invalid retention entry",
    )
}

/// Staging requires a writable regular file, and no special bits. The C++ worker answers an unwritable request with
/// `UNSUPPORTED`, which differs from the other systems.
pub fn check_stage_mode(mode: u32) -> Outcome<()> {
    if mode & 0o7000 != 0 || mode & 0o222 == 0 {
        return Err(Failure::new(
            "UNSUPPORTED",
            "V1 requires writable regular-file staging",
        ));
    }
    Ok(())
}

/// Creates the staged object under a name of this worker's own. Only its handle selects it afterwards: the name is
/// never reopened or renamed by name, and it stays in the directory, as the C++ worker leaves it. The requested mode
/// is not applied, as in the C++ worker; the published mode is checked by `produced_mode`.
pub fn stage(parent: &Object, bytes: &[u8], _mode: u32) -> Outcome<(Object, Vec<u8>)> {
    static SERIAL: AtomicU32 = AtomicU32::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed) + 1;
    let name = format!(".nocturne-rollback-stage-{}-{serial}", std::process::id());
    let staged = open_or_refuse(parent.0, &name, false, true, true, true)?;
    write_all(&staged, bytes)?;
    flush(&staged)?;
    require(read_all(&staged)? == bytes, "Stage changed")?;
    Ok((staged, name.into_bytes()))
}

/// The permission bits that a published file must have. The C++ worker checks the regular-file mode of `0666`, which
/// does not depend on the requested mode.
pub fn produced_mode(_requested: u32) -> u32 {
    PRODUCED_PERMISSIONS
}

/// Publishes the staged handle as `leaf` under `parent`: an exclusive rename of that handle, so the published entry
/// is the same object. The identity is verified here.
pub fn publish(staged: &Object, parent: &Object, leaf: &[u8], _mode: u32) -> Outcome<()> {
    rename_exclusive(staged.0, parent.0, utf8(leaf)?)?;
    let produced = child(parent, leaf, false)?.ok_or_else(refused)?;
    require(
        identity(&produced)? == identity(staged)?,
        "Produced state diverged",
    )
}

/// Moves the observed handle to `to` under `parent` without replacing anything. The source is the handle that was
/// observed, not a name, so a name swapped by a peer cannot be moved.
pub fn displace(
    parent: &Object,
    _leaf: &[u8],
    observed: Option<&Object>,
    to: &[u8],
) -> Outcome<()> {
    let observed = observed.ok_or_else(refused)?;
    rename_exclusive(observed.0, parent.0, utf8(to)?)
}

/// `NtSetInformationFile` with FILE_RENAME_INFORMATION, relative to `parent`, without replacement.
fn rename_exclusive(source: HANDLE, parent: HANDLE, name: &str) -> Outcome<()> {
    valid_entry(name.as_bytes(), true)?;
    let units: Vec<u16> = name.encode_utf16().collect();
    let name_length =
        u32::try_from(units.len() * 2).map_err(|_| Failure::new("CONFLICT", "Invalid name"))?;
    let total = offset_of!(RenameInformation, file_name) + (units.len() + 1) * 2;
    let mut buffer = vec![0u8; total];
    let root_offset = offset_of!(RenameInformation, root_directory);
    buffer[root_offset..root_offset + size_of::<HANDLE>()]
        .copy_from_slice(&(parent as usize).to_ne_bytes());
    let length_offset = offset_of!(RenameInformation, file_name_length);
    buffer[length_offset..length_offset + 4].copy_from_slice(&name_length.to_ne_bytes());
    let name_offset = offset_of!(RenameInformation, file_name);
    for (index, unit) in units.iter().enumerate() {
        let at = name_offset + index * 2;
        buffer[at..at + 2].copy_from_slice(&unit.to_ne_bytes());
    }
    let api = nt_api()?;
    let mut io = IoStatusBlock {
        status: 0,
        information: 0,
    };
    // SAFETY: `source` is a live handle, and `buffer` holds a FILE_RENAME_INFORMATION of the length passed.
    let status = unsafe {
        (api.set_information)(
            source,
            &mut io,
            buffer.as_ptr().cast(),
            total as u32,
            FILE_RENAME_INFORMATION,
        )
    };
    require(status >= 0, "Exclusive handle-relative rename refused")
}
