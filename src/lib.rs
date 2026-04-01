#![warn(clippy::cargo)]

#[cfg(unix)]
use std::os::unix::fs::{chown, MetadataExt as _, PermissionsExt as _};
use std::{fs::File, io, path::Path};

use filetime::{set_file_handle_times, set_file_times, FileTime};

const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x2000000;

/// Opens a file with optimal flags for metadata updates.
#[inline]
fn open_file_for_metadata(path: &Path) -> io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // 0x0100 = FILE_WRITE_ATTRIBUTES: Allows metadata changes even if read-only.
        // 0x7 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE: Maximize concurrency.
        // 0x2000000 = FILE_FLAG_BACKUP_SEMANTICS: Allows dir.
        opts.access_mode(0x0100)
            .share_mode(0x7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }

    opts.open(path)
}

#[cfg(unix)]
fn copy_permission_impl(
    to_path: &Path,
    to_file: Option<&File>,
    from_meta: &std::fs::Metadata,
    to_meta: &std::fs::Metadata,
) -> io::Result<()> {
    let from_gid = from_meta.gid();
    let to_gid = to_meta.gid();

    let mut perms = from_meta.permissions();
    perms.set_mode(perms.mode() & 0o0777);

    // chown only supports path-based operation in std
    if from_gid != to_gid && chown(to_path, None, Some(from_gid)).is_err() {
        let new_perms = (perms.mode() & 0o0707) | ((perms.mode() & 0o07) << 3);
        perms.set_mode(new_perms);
    }

    // Use handle-based fchmod if available, avoiding TOCTOU
    if let Some(file) = to_file {
        file.set_permissions(perms)
    } else {
        std::fs::set_permissions(to_path, perms)
    }
}

#[cfg(windows)]
#[inline]
fn copy_permission_impl(
    to_path: &Path,
    to_file: Option<&File>,
    from_meta: &std::fs::Metadata,
    _to_meta: &std::fs::Metadata,
) -> io::Result<()> {
    let permissions = from_meta.permissions();
    if let Some(file) = to_file {
        file.set_permissions(permissions)
    } else {
        std::fs::set_permissions(to_path, permissions)
    }
}

#[inline]
fn copy_time_path(to: &Path, from_meta: &std::fs::Metadata) -> io::Result<()> {
    let atime = FileTime::from_last_access_time(from_meta);
    let mtime = FileTime::from_last_modification_time(from_meta);
    set_file_times(to, atime, mtime)
}

/// Copy metadata from one file to another, including permissions and time.
pub fn copy_metadata(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let from_meta = std::fs::metadata(from)?;

    match open_file_for_metadata(to) {
        Ok(to_file) => {
            let to_meta = to_file.metadata()?;
            let atime = FileTime::from_last_access_time(&from_meta);
            let mtime = FileTime::from_last_modification_time(&from_meta);

            if let Err(e) = set_file_handle_times(&to_file, Some(atime), Some(mtime)) {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    copy_time_path(to, &from_meta)?;
                } else {
                    return Err(e);
                }
            }

            copy_permission_impl(to, Some(&to_file), &from_meta, &to_meta)
        }
        Err(_) => {
            // Fallback to path-based operations if a handle cannot be opened
            let to_meta = std::fs::metadata(to)?;
            let res = copy_time_path(to, &from_meta);
            copy_permission_impl(to, None, &from_meta, &to_meta)?;

            if let Err(err) = res {
                // Retry setting time if the initial failure was due to a read-only lock
                if err.kind() == io::ErrorKind::PermissionDenied {
                    copy_time_path(to, &from_meta)?;
                } else {
                    return Err(err);
                }
            }
            Ok(())
        }
    }
}

/// Copy permission from one file to another.
pub fn copy_permission(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let from_meta = std::fs::metadata(from)?;

    match open_file_for_metadata(to) {
        Ok(to_file) => {
            let to_meta = to_file.metadata()?;
            copy_permission_impl(to, Some(&to_file), &from_meta, &to_meta)
        }
        Err(_) => {
            let to_meta = std::fs::metadata(to)?;
            copy_permission_impl(to, None, &from_meta, &to_meta)
        }
    }
}

/// Copy time stamp from one file to another.
///
/// Including last_access_time (atime) and last_modification_time (mtime).
pub fn copy_time(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let from_meta = std::fs::metadata(from)?;

    let atime = FileTime::from_last_access_time(&from_meta);
    let mtime = FileTime::from_last_modification_time(&from_meta);

    if let Ok(to_file) = open_file_for_metadata(to) {
        if let Err(e) = set_file_handle_times(&to_file, Some(atime), Some(mtime)) {
            if e.kind() == io::ErrorKind::PermissionDenied {
                return set_file_times(to, atime, mtime);
            }
            return Err(e);
        }
        Ok(())
    } else {
        set_file_times(to, atime, mtime)
    }
}
