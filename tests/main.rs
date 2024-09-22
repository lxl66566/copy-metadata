use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    thread::sleep,
    time::Duration,
};

use copy_metadata::copy_metadata;
use tap::Tap;

fn perm_to_num(perm: &std::fs::Permissions) -> u32 {
    perm.mode() & 0o777
}

fn time_tuple(meta: &std::fs::Metadata) -> [i64; 2] {
    [meta.atime(), meta.mtime()]
}

#[test]
fn test_copy_metadata() {
    [0o777, 0o644]
        .map(|p| {
            let x = tempfile::NamedTempFile::new().unwrap();
            let x_p = x.path();
            std::fs::write(x_p, "foo").unwrap();
            std::fs::set_permissions(x_p, std::fs::Permissions::from_mode(p)).unwrap();
            x
        })
        .into_iter()
        .tap(|_| sleep(Duration::from_secs(1)))
        .for_each(|from| {
            let from = from.path();
            let to = tempfile::NamedTempFile::new().unwrap();
            let to = to.path();
            copy_metadata(from, to).unwrap();

            let to_meta = to.metadata().unwrap();
            let from_meta = std::fs::metadata(from).unwrap();
            println!("{:o}", perm_to_num(&to_meta.permissions()));
            assert_eq!(to_meta.mode(), from_meta.mode());

            let from_time = time_tuple(&from_meta);
            let to_time = time_tuple(&to_meta);
            assert_eq!(from_time, to_time);
        });
}
