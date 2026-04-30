#![warn(clippy::cargo)]

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::{fs::File, io, path::Path};

use filetime::{set_file_handle_times, FileTime};

const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x2000000;

/// Safely open a file handle, specifically for reading or modifying Metadata
#[inline]
fn open_file_for_metadata(path: &Path, is_source: bool) -> io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.read(true);
        // 0x0100 = FILE_WRITE_ATTRIBUTES (needed if this is the target file)
        // 0x0080 = FILE_READ_ATTRIBUTES (needed if this is the source file)
        let access = if is_source { 0x0080 } else { 0x0100 };
        opts.access_mode(access)
            .share_mode(0x7) // FILE_SHARE_READ | WRITE | DELETE allows opening while another process is using the
            // file
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS); // Allows opening directories
        opts.open(path)
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NONBLOCK prevents hanging on FIFOs/special files
        opts.custom_flags(libc::O_NONBLOCK);

        // Try opening with read-only access
        opts.read(true);
        match opts.open(path) {
            Ok(f) => Ok(f),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && !is_source => {
                // If it's the target file and we have no read permission (e.g. --w-------),
                // try opening write-only to obtain an fd
                let mut write_opts = std::fs::OpenOptions::new();
                write_opts.write(true).custom_flags(libc::O_NONBLOCK);
                write_opts.open(path)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(unix)]
fn copy_permission_impl(
    to_file: &File,
    from_meta: &std::fs::Metadata,
    to_meta: &std::fs::Metadata,
) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;

    let from_gid = from_meta.gid();
    let to_gid = to_meta.gid();
    let from_uid = from_meta.uid();

    let mut perms = from_meta.permissions();
    perms.set_mode(perms.mode() & 0o0777);

    // 1. Use handle-based fchown, completely preventing TOCTOU symlink attacks
    if from_gid != to_gid {
        let fd = to_file.as_raw_fd();
        // Try to change owner and group. If we are not root, changing uid may fail;
        // the main goal here is to synchronize the gid
        let res = unsafe { libc::fchown(fd, from_uid, from_gid) };

        if res != 0 {
            // If fchown fails (usually due to insufficient permissions), use fallback
            // logic: Copy the 'other' permission bits to the group permission
            // bits
            let new_perms = (perms.mode() & 0o0707) | ((perms.mode() & 0o07) << 3);
            perms.set_mode(new_perms);
        }
    }

    // 2. Use handle-based fchmod (Rust's File::set_permissions uses fchmod on Unix
    //    under the hood)
    to_file.set_permissions(perms)
}

#[cfg(windows)]
#[inline]
fn copy_permission_impl(
    to_file: &File,
    from_meta: &std::fs::Metadata,
    _to_meta: &std::fs::Metadata,
) -> io::Result<()> {
    // On Windows, Rust 1.63+ uses handle-based SetFileInformationByHandle for
    // File::set_permissions, so this is safe with no TOCTOU risk.
    to_file.set_permissions(from_meta.permissions())
}

/// Copy Metadata (permissions and timestamps), 100% handle-based with no TOCTOU
/// risk
pub fn copy_metadata(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    // 1. Open the source file handle to prevent the source from being replaced
    //    mid-operation
    let from_file = open_file_for_metadata(from.as_ref(), true)?;
    let from_meta = from_file.metadata()?;

    // 2. Open the target file handle
    let to_file = open_file_for_metadata(to.as_ref(), false)?;
    let to_meta = to_file.metadata()?;

    let atime = FileTime::from_last_access_time(&from_meta);
    let mtime = FileTime::from_last_modification_time(&from_meta);

    // 3. Set timestamps using the handle (calls futimens or SetFileTime internally)
    set_file_handle_times(&to_file, Some(atime), Some(mtime))?;

    // 4. Set permissions using the handle
    copy_permission_impl(&to_file, &from_meta, &to_meta)
}

/// Copy only permissions
pub fn copy_permission(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from_file = open_file_for_metadata(from.as_ref(), true)?;
    let from_meta = from_file.metadata()?;

    let to_file = open_file_for_metadata(to.as_ref(), false)?;
    let to_meta = to_file.metadata()?;

    copy_permission_impl(&to_file, &from_meta, &to_meta)
}

/// Copy only timestamps
pub fn copy_time(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from_file = open_file_for_metadata(from.as_ref(), true)?;
    let from_meta = from_file.metadata()?;

    let atime = FileTime::from_last_access_time(&from_meta);
    let mtime = FileTime::from_last_modification_time(&from_meta);

    let to_file = open_file_for_metadata(to.as_ref(), false)?;
    set_file_handle_times(&to_file, Some(atime), Some(mtime))
}
