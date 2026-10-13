use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use crate::Volume;

/// Dot-files are hidden.
pub fn is_hidden(name: &str, _meta: &Metadata) -> bool {
    name.starts_with('.')
}

/// The root plus every block-device mount from `/proc/mounts`.
pub fn volumes() -> Vec<Volume> {
    let mut out = vec![Volume {
        name: "/".to_string(),
        path: PathBuf::from("/"),
    }];
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return out;
    };
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(mount_point)) = (fields.next(), fields.next()) else {
            continue;
        };
        // /proc/mounts escapes spaces as \040.
        let mount_point = mount_point.replace("\\040", " ");
        if !device.starts_with("/dev/") || device.starts_with("/dev/loop") || mount_point == "/" {
            continue;
        }
        out.push(Volume {
            name: mount_point.clone(),
            path: PathBuf::from(mount_point),
        });
    }
    out
}

/// Linux has no path length limit worth working around: paths are used as they are.
pub fn extended_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Replacing a file by renaming over it does not depend on the old file's mode, so there
/// is nothing to prepare.
pub fn make_writable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Recreates the symlink `src` at `dst` with the same target.
pub fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(src)?, dst)
}
