//! File system abstraction layer.
//!
//! Everything that differs between Windows and Linux lives behind this crate
//! (see the `platform` module). Crates above `vfs` must not use `#[cfg(windows)]`
//! or `#[cfg(unix)]`.

mod platform;

use std::cmp::Ordering;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use platform::{config_dir, home_dir, volumes};

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

/// Lists a directory: directories first, then files, both sorted case-insensitively.
///
/// Entries that cannot be read (for example removed during listing) are skipped.
pub fn list_dir(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(path)? {
        let Ok(item) = item else { continue };
        let entry_path = item.path();
        let Ok(meta) = std::fs::symlink_metadata(&entry_path) else {
            continue;
        };
        let name = item.file_name().to_string_lossy().into_owned();
        let kind = if meta.is_symlink() {
            match std::fs::metadata(&entry_path) {
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
        entries.push(Entry {
            hidden: platform::is_hidden(&name, &meta),
            size: if kind == EntryKind::File {
                meta.len()
            } else {
                0
            },
            modified: meta.modified().ok(),
            name,
            path: entry_path,
            kind,
        });
    }
    entries.sort_by(compare_entries);
    Ok(entries)
}

fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
    b.is_dir()
        .cmp(&a.is_dir())
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn volumes_never_empty() {
        // On every supported system there is at least one volume (C:\ or /).
        assert!(!volumes().is_empty());
    }
}
