use std::fs::Metadata;
use std::path::PathBuf;

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
