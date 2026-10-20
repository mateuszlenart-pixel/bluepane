//! File system abstraction layer.
//!
//! Everything that differs between Windows and Linux lives behind this crate
//! (see the `platform` module). Crates above `vfs` must not use `#[cfg(windows)]`
//! or `#[cfg(unix)]`.

mod platform;
mod smb;

pub use smb::{list_shares, open_remote_desktop, Share, ShareKind};

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use platform::{
    config_dir, copy_symlink, extended_path, file_mode, home_dir, make_writable, neighbours,
    open_external, set_mode, volumes,
};

/// How full the drive holding a path is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskSpace {
    /// Bytes a normal user can still write.
    pub free: u64,
    pub total: u64,
}

impl DiskSpace {
    /// Share of the drive in use, 0.0 to 1.0.
    pub fn used_fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (1.0 - self.free as f64 / self.total as f64).clamp(0.0, 1.0) as f32
    }
}

/// Free and total space of the drive that holds `path`; `None` when it cannot be asked
/// (the path is gone, or the drive does not say).
pub fn disk_space(path: &Path) -> Option<DiskSpace> {
    // Windows answers for the drive even when the folder is gone; Linux refuses. A missing
    // path gets the same answer on both.
    if !path.exists() {
        return None;
    }
    let total = fs4::total_space(path).ok()?;
    let free = fs4::available_space(path).ok()?;
    (total > 0).then_some(DiskSpace { free, total })
}

/// What a directory entry is, from the user's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

/// One row of a panel listing.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// When the file was created, where the file system keeps it.
    pub created: Option<SystemTime>,
    /// The permission bits (`rwxr-xr-x` is `0o755`), where the file system has them.
    pub mode: Option<u32>,
    /// The owner's name (the number when the name is not known), where the file system has one.
    pub owner: Option<String>,
    pub hidden: bool,
}

impl Entry {
    /// True for directories and for symlinks pointing at directories.
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
}

/// A mounted volume (a drive letter on Windows, a mount point on Linux).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub path: PathBuf,
}

/// Describes one existing path the way a listing does (a symlink to a directory counts as a
/// directory). Fails when the path is gone.
pub fn entry_of(path: &Path) -> io::Result<Entry> {
    let meta = std::fs::symlink_metadata(path)?;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let kind = if meta.is_symlink() {
        match std::fs::metadata(path) {
            Ok(target) if target.is_dir() => EntryKind::Dir,
            _ => EntryKind::Symlink,
        }
    } else if meta.is_dir() {
        EntryKind::Dir
    } else if meta.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };
    Ok(Entry {
        created: meta.created().ok(),
        mode: platform::entry_mode(&meta),
        owner: platform::entry_owner(&meta),
        hidden: platform::is_hidden(&name, &meta),
        size: if kind == EntryKind::File {
            meta.len()
        } else {
            0
        },
        modified: meta.modified().ok(),
        name,
        path: path.to_path_buf(),
        kind,
    })
}

/// Lists a directory: directories first, then files, both sorted case-insensitively.
///
/// Entries that cannot be read (for example removed during listing) are skipped.
pub fn list_dir(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(path)? {
        let Ok(item) = item else { continue };
        if let Ok(entry) = entry_of(&item.path()) {
            entries.push(entry);
        }
    }
    // Cached keys: lower-casing once per entry instead of on every comparison matters
    // for directories with 100k files.
    entries.sort_by_cached_key(|e| (!e.is_dir(), e.name.to_lowercase(), e.name.clone()));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_space_of_a_real_folder_is_sane_and_of_a_missing_one_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let space = disk_space(dir.path()).expect("the temp drive answers");
        assert!(space.total > 0 && space.free <= space.total, "{space:?}");
        assert!((0.0..=1.0).contains(&space.used_fraction()));
        assert_eq!(disk_space(&dir.path().join("missing/nowhere")), None);
        assert_eq!(
            DiskSpace {
                free: 25,
                total: 100
            }
            .used_fraction(),
            0.75
        );
        assert_eq!(DiskSpace { free: 0, total: 0 }.used_fraction(), 0.0);
    }

    #[test]
    fn dirs_come_first_then_names_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("b.txt"), "x").unwrap();
        std::fs::write(tmp.path().join("A.txt"), "xyz").unwrap();
        std::fs::create_dir(tmp.path().join("zdir")).unwrap();

        let entries = list_dir(tmp.path()).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zdir", "A.txt", "b.txt"]);
        assert!(entries[0].is_dir());
        assert_eq!(entries[1].size, 3);
    }

    #[test]
    fn missing_directory_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list_dir(&tmp.path().join("nope")).is_err());
    }

    #[test]
    fn normal_paths_are_not_extended() {
        let path = Path::new("/tmp/short");
        assert_eq!(extended_path(path), path);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_recreated_with_the_same_target() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target.txt");
        std::fs::write(&target, "x").unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink("target.txt", &link).unwrap();
        let copy = tmp.path().join("copy");
        copy_symlink(&link, &copy).unwrap();
        assert_eq!(std::fs::read_link(&copy).unwrap(), Path::new("target.txt"));
    }

    #[test]
    fn a_listed_entry_carries_its_permission_bits_and_owner_where_the_system_has_them() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        #[cfg(unix)]
        {
            set_mode(&file, 0o640).unwrap();
            let entry = entry_of(&file).unwrap();
            assert_eq!(entry.mode, Some(0o640));
            assert!(entry.owner.is_some_and(|o| !o.is_empty()));
        }
        #[cfg(windows)]
        {
            let entry = entry_of(&file).unwrap();
            assert_eq!((entry.mode, entry.owner), (None, None));
        }
    }

    #[cfg(unix)]
    #[test]
    fn modes_round_trip_and_special_bits_are_stripped() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("f");
        std::fs::write(&file, "x").unwrap();
        set_mode(&file, 0o754).unwrap();
        assert_eq!(file_mode(&file), Some(0o754));
        // setuid / setgid / sticky are dropped.
        set_mode(&file, 0o6755).unwrap();
        assert_eq!(file_mode(&file), Some(0o755));
        assert_eq!(file_mode(&tmp.path().join("missing")), None);
    }

    #[test]
    fn make_writable_keeps_files_usable() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("f");
        std::fs::write(&file, "x").unwrap();
        make_writable(&file).unwrap();
        std::fs::write(&file, "y").unwrap();
    }

    #[test]
    fn volumes_never_empty() {
        // On every supported system there is at least one volume (C:\ or /).
        assert!(!volumes().is_empty());
    }
}
