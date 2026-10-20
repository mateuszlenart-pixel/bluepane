use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::Volume;

/// Dot-files are hidden.
pub fn is_hidden(name: &str, _meta: &Metadata) -> bool {
    name.starts_with('.')
}

/// Runs a program in `dir` and collects its output (nothing is shown on screen).
pub fn run_hidden(program: &str, args: &[&str], dir: &Path) -> io::Result<std::process::Output> {
    std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
}

/// The permission bits of a listed entry, including setuid, setgid and sticky.
pub fn entry_mode(meta: &Metadata) -> Option<u32> {
    Some(meta.permissions().mode() & 0o7777)
}

/// The user names of `/etc/passwd`, read once.
fn user_names() -> &'static std::collections::HashMap<u32, String> {
    static NAMES: std::sync::OnceLock<std::collections::HashMap<u32, String>> =
        std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        fs::read_to_string("/etc/passwd")
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let mut fields = line.split(':');
                let name = fields.next()?;
                let uid = fields.nth(1)?.parse().ok()?;
                Some((uid, name.to_string()))
            })
            .collect()
    })
}

/// The owner's name, or the user number when `/etc/passwd` does not know it.
pub fn entry_owner(meta: &Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let uid = meta.uid();
    Some(
        user_names()
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string()),
    )
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

/// Opens a file with the desktop's default application.
pub const DEFAULT_OPENER: (&str, &[&str]) = ("xdg-open", &[]);

/// The permission bits of a file (`rwx` for user, group and other).
pub fn file_mode(path: &Path) -> Option<u32> {
    fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

/// Sets the permission bits. setuid, setgid and the sticky bit are never set from outside
/// (an archive must not be able to create a privileged program).
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
}

/// The neighbours (IP address, MAC address) the kernel has resolved lately.
pub fn neighbours() -> Vec<(std::net::Ipv4Addr, String)> {
    fs::read_to_string("/proc/net/arp")
        .map(|text| super::arp::parse_proc_net_arp(&text))
        .unwrap_or_default()
}
