use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io;
use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
use std::path::{Component, Path, PathBuf};

use windows_sys::Win32::Foundation::{
    ERROR_CALL_NOT_IMPLEMENTED, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER,
    ERROR_NOT_SUPPORTED,
};
use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
    FileCaseSensitiveInfo, GetFileInformationByHandleEx,
};
use windows_sys::Win32::System::SystemServices::FILE_CS_FLAG_CASE_SENSITIVE_DIR;

use super::PolicyError;

pub(super) fn is_within(path: &Path, root: &Path) -> Result<bool, PolicyError> {
    // Apply Win32 spelling rules even to missing paths. Explicit verbatim paths
    // keep their literal names (e.g. a directory with a trailing dot).
    let resolve = |path: &Path| {
        let spelling = if path.as_os_str().is_empty() { Path::new(".") } else { path };
        let absolute = std::path::absolute(spelling).map_err(|source| PolicyError::Io {
            path: path.to_owned(),
            source,
        })?;
        super::resolve(&absolute)
    };
    let path = resolve(path)?;
    let root = resolve(root)?;
    if path.starts_with(&root) {
        return Ok(true);
    }

    let mut path = path.components();
    let mut parent = PathBuf::new();
    let mut differing_parents = Vec::new();
    for component in root.components() {
        let Some(other) = path.next() else {
            return Ok(false);
        };
        if component != other {
            if !equal_ignoring_case(component.as_os_str(), other.as_os_str()) {
                return Ok(false);
            }
            if matches!(component, Component::Normal(_)) {
                differing_parents.push(parent.clone());
            }
        }
        parent.push(component);
    }
    // Only ask the filesystem if the full prefix differs solely in casing.
    for dir in differing_parents {
        if case_sensitive(&dir).map_err(|source| PolicyError::Io { path: dir, source })? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn equal_ignoring_case(a: &OsStr, b: &OsStr) -> bool {
    let a: Vec<_> = a.encode_wide().collect();
    let b: Vec<_> = b.encode_wide().collect();
    // SAFETY: Windows path components fit in an i32. The buffers live through the call.
    unsafe {
        CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1)
            == CSTR_EQUAL
    }
}

fn case_sensitive(directory: &Path) -> io::Result<bool> {
    // A missing directory inherits its parent's case sensitivity when created.
    for dir in directory.ancestors() {
        let handle = match OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(dir)
        {
            Ok(handle) => handle,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let mut info = FILE_CASE_SENSITIVE_INFO::default();
        // SAFETY: The handle stays open and the output buffer matches the information class.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.as_raw_handle(),
                FileCaseSensitiveInfo,
                (&raw mut info).cast(),
                size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
            )
        };
        if ok != 0 {
            return Ok(info.Flags & FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0);
        }
        let err = io::Error::last_os_error();
        // Older Windows and filesystems without this flag use the Win32 default.
        return match err.raw_os_error().map(|code| code as u32) {
            Some(
                ERROR_CALL_NOT_IMPLEMENTED | ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER
                | ERROR_NOT_SUPPORTED,
            ) => Ok(false),
            _ => Err(err),
        };
    }
    Err(io::ErrorKind::NotFound.into())
}
