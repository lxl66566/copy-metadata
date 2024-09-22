#[cfg(unix)]
use std::os::unix::fs::{chown, MetadataExt as _, PermissionsExt as _};
use std::{io, path::Path};

use filetime::{set_file_times, FileTime};

/// from: https://github.com/helix-editor/helix/blob/d6eb10d9f907139597ededa38a2cab44b26f5da6/helix-stdx/src/faccess.rs#L60
///
/// uses MPL-2.0 license, link: https://github.com/helix-editor/helix/blob/master/LICENSE
#[cfg(unix)]
fn copy_permission_inner(
    to: &Path,
    from_meta: &std::fs::Metadata,
    to_meta: &std::fs::Metadata,
) -> io::Result<()> {
    let from_gid = from_meta.gid();
    let to_gid = to_meta.gid();

    let mut perms = from_meta.permissions();
    perms.set_mode(perms.mode() & 0o0777);
    if from_gid != to_gid && chown(to, None, Some(from_gid)).is_err() {
        let new_perms = (perms.mode() & 0o0707) | ((perms.mode() & 0o07) << 3);
        perms.set_mode(new_perms);
    }
    std::fs::set_permissions(to, perms)?;
    Ok(())
}

#[cfg(windows)]
fn copy_permission_inner(
    to: &Path,
    from_meta: &std::fs::Metadata,
    to_meta: &std::fs::Metadata,
) -> io::Result<()> {
    let permissions = from_meta.permissions();
    std::fs::set_permissions(to, permissions)?;
    Ok(())
}

fn copy_time_inner(to: &Path, from_meta: &std::fs::Metadata) -> io::Result<()> {
    let atime = FileTime::from_last_access_time(from_meta);
    let mtime = FileTime::from_last_modification_time(from_meta);
    set_file_times(to, atime, mtime)
}

/// copy metadata from one file to another, including permissions and time.
pub fn copy_metadata(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let from_meta = std::fs::metadata(from)?;
    let to_meta = std::fs::metadata(to)?;

    copy_permission_inner(to, &from_meta, &to_meta)?;
    copy_time_inner(to, &from_meta)?;
    Ok(())
}

pub fn copy_permission(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let from_meta = std::fs::metadata(from)?;
    let to_meta = std::fs::metadata(to)?;
    copy_permission_inner(to, &from_meta, &to_meta)
}

pub fn copy_time(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let from_meta = std::fs::metadata(from)?;
    copy_time_inner(to, &from_meta)
}
