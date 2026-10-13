use std::fs::{self, Metadata};
use std::io;
use std::os::windows::fs::{symlink_dir, symlink_file, MetadataExt};
use std::path::{Path, PathBuf};

use super::verbatim::{to_verbatim, LONG_PATH_THRESHOLD};
use crate::Volume;

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// Hidden or system attribute set.
pub fn is_hidden(_name: &str, meta: &Metadata) -> bool {
    meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

/// Drive letters that currently exist.
pub fn volumes() -> Vec<Volume> {
    ('A'..='Z')
        .filter_map(|letter| {
            let path = PathBuf::from(format!("{letter}:\\"));
            path.exists().then(|| Volume {
                name: format!("{letter}:"),
                path,
            })
        })
        .collect()
}

/// Long paths (over about 240 characters) get the `\\?\` prefix so Windows accepts them;
/// shorter ones are returned unchanged.
pub fn extended_path(path: &Path) -> PathBuf {
    if path.as_os_str().len() < LONG_PATH_THRESHOLD {
        return path.to_path_buf();
    }
    let Ok(absolute) = std::path::absolute(path) else {
        return path.to_path_buf();
    };
    absolute
        .to_str()
        .and_then(to_verbatim)
        .map_or_else(|| path.to_path_buf(), PathBuf::from)
}

/// Clears the read-only attribute, which would make replacing the file fail.
// The lint warns that on Unix `set_readonly(false)` makes a file world-writable. This module
// only builds on Windows, where it just clears the attribute.
#[allow(clippy::permissions_set_readonly_false)]
pub fn make_writable(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

/// Recreates the symlink `src` at `dst`. Needs the symlink privilege (or developer mode);
/// the error is returned so the caller can fall back to copying the target.
pub fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    let target = fs::read_link(src)?;
    if fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false) {
        symlink_dir(target, dst)
    } else {
        symlink_file(target, dst)
    }
}
