#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{fs, thread::sleep, time::Duration};

use copy_metadata::copy_metadata;
#[cfg(feature = "copy-time")]
use filetime::{set_file_times, FileTime};
use tap::Tap;

#[cfg(unix)]
#[cfg(feature = "copy-time")]
fn time_tuple(meta: &std::fs::Metadata) -> [i64; 2] {
    [meta.atime(), meta.mtime()]
}

#[cfg(windows)]
#[cfg(feature = "copy-time")]
fn time_tuple(meta: &std::fs::Metadata) -> [std::time::SystemTime; 2] {
    [meta.accessed().unwrap(), meta.modified().unwrap()]
}

#[cfg(unix)]
#[test]
fn test_copy_metadata() {
    fn perm_to_num(perm: &std::fs::Permissions) -> u32 {
        perm.mode() & 0o777
    }

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

            #[cfg(feature = "copy-time")]
            {
                let from_time = time_tuple(&from_meta);
                let to_time = time_tuple(&to_meta);
                assert_eq!(from_time, to_time);
            }
        });
}

#[cfg(windows)]
#[test]
fn test_copy_metadata() {
    [true, false]
        .map(|p| {
            let x = tempfile::NamedTempFile::new().unwrap();
            let x_p = x.path();
            std::fs::write(x_p, "foo").unwrap();
            let mut perm = x_p.metadata().unwrap().permissions();
            perm.set_readonly(p);
            std::fs::set_permissions(x_p, perm).unwrap();
            x
        })
        .into_iter()
        .tap(|_| sleep(Duration::from_secs(1)))
        .for_each(|from| {
            let from = from.path();
            let to = tempfile::NamedTempFile::new().unwrap();
            let to = to.path();
            println!("from: {}, to: {}\n", from.display(), to.display());
            copy_metadata(from, to).unwrap();

            let to_meta = to.metadata().unwrap();
            let from_meta = std::fs::metadata(from).unwrap();
            assert_eq!(
                to_meta.permissions().readonly(),
                from_meta.permissions().readonly()
            );

            #[cfg(feature = "copy-time")]
            {
                let from_time = time_tuple(&from_meta);
                let to_time = time_tuple(&to_meta);
                assert_eq!(from_time, to_time);
            }
        });
}

#[test]
fn test_copy_metadata_while_file_held() {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("src.txt");
    let dst_path = dir.path().join("dst.txt");

    fs::write(&src_path, "source content").unwrap();
    fs::write(&dst_path, "target content").unwrap();

    #[cfg(feature = "copy-time")]
    let expected_atime = FileTime::from_unix_time(1500000000, 0);
    #[cfg(feature = "copy-time")]
    let expected_mtime = FileTime::from_unix_time(1600000000, 0);
    #[cfg(feature = "copy-time")]
    set_file_times(&src_path, expected_atime, expected_mtime).unwrap();

    let mut src_perms = fs::metadata(&src_path).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        src_perms.set_mode(0o755);
    }
    #[cfg(windows)]
    {
        src_perms.set_readonly(true);
    }
    fs::set_permissions(&src_path, src_perms).unwrap();

    let _held_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&dst_path)
        .expect("Failed to open target file for holding");

    copy_metadata(&src_path, &dst_path).expect("copy_metadata failed while file fd was held!");
    let dst_meta = fs::metadata(&dst_path).unwrap();

    #[cfg(feature = "copy-time")]
    {
        let dst_atime = FileTime::from_last_access_time(&dst_meta);
        let dst_mtime = FileTime::from_last_modification_time(&dst_meta);
        assert_eq!(
            dst_atime.unix_seconds(),
            expected_atime.unix_seconds(),
            "Access time mismatch"
        );
        assert_eq!(
            dst_mtime.unix_seconds(),
            expected_mtime.unix_seconds(),
            "Modification time mismatch"
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            dst_meta.permissions().mode() & 0o777,
            0o755,
            "Unix permissions mismatch"
        );
    }
    #[cfg(windows)]
    {
        assert!(
            dst_meta.permissions().readonly(),
            "Windows readonly flag mismatch"
        );

        // after cleanup
        let mut restore_perms = dst_meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        restore_perms.set_readonly(false);
        let _ = fs::set_permissions(&dst_path, restore_perms.clone());
        let _ = fs::set_permissions(&src_path, restore_perms);
    }
}
