#![warn(clippy::cargo)]

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::{fs::File, io, path::Path};

use filetime::{set_file_handle_times, FileTime};

const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x2000000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;

/// Safely open a file handle, specifically for reading or modifying Metadata
#[inline]
fn open_file_for_metadata(path: &Path, is_source: bool) -> io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();

    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        opts.read(true);
        // 0x0100 = FILE_WRITE_ATTRIBUTES (needed if this is the target file)
        // 0x0080 = FILE_READ_ATTRIBUTES (needed if this is the source file)
        let access = if is_source { 0x0080 } else { 0x0100 };
        opts.access_mode(access)
            .share_mode(0x7) // FILE_SHARE_READ | WRITE | DELETE allows opening while another process is using the
            // file
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT); // Allows opening directories and reparse points

        let f = opts.open(path)?;

        // Check if it's a reparse point (e.g., symlink or directory junction) and
        // reject it to prevent TOCTOU
        if f.metadata()?.file_attributes() & 0x400 != 0 {
            // 0x400 = FILE_ATTRIBUTE_REPARSE_POINT
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Symlinks not supported",
            ));
        }

        Ok(f)
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        // O_NONBLOCK prevents hanging on FIFOs/special files.
        // O_NOFOLLOW prevents TOCTOU symlink attacks.
        // Note: O_RDONLY works fine for both files and directories, so we don't need
        // O_DIRECTORY or any prior metadata checks.
        let flags = libc::O_NONBLOCK | libc::O_NOFOLLOW;
        opts.custom_flags(flags);

        // Try opening with read-only access
        opts.read(true);
        match opts.open(path) {
            Ok(f) => Ok(f),
            Err(e) if e.raw_os_error() == Some(libc::ELOOP) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Symlinks not supported",
            )),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && !is_source => {
                // If it's the target file and we have no read permission (e.g. --w-------),
                // try opening write-only to obtain an fd.
                // Note: If the path is a directory, O_WRONLY will naturally fail with EISDIR,
                // which is acceptable since we cannot get a handle for a write-only directory.
                let mut write_opts = std::fs::OpenOptions::new();
                write_opts.write(true).custom_flags(flags);
                match write_opts.open(path) {
                    Ok(f) => Ok(f),
                    Err(we) if we.raw_os_error() == Some(libc::ELOOP) => Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Symlinks not supported",
                    )),
                    Err(we) => Err(we),
                }
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
            // Security fallback: If fchown fails (usually because the user is not root
            // and the source group differs from the user's default group), the target
            // file will be owned by the user's default group.
            // To prevent privilege escalation, we must not grant the original group
            // permissions to the new group, because the new group might contain users
            // who only had 'other' access to the source file.
            // Therefore, we downgrade the group permissions to match the 'other'
            // permissions.
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
