use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::schedule::{cron_line, cron_replace, cron_tasks};
use crate::{ScheduleWhen, ScheduledTask, Volume};

/// Dot-files are hidden.
pub fn is_hidden(name: &str, _meta: &Metadata) -> bool {
    name.starts_with('.')
}

/// Terminal emulators to try, in order; each starts in `dir` (the working directory of the
/// process). `$TERMINAL` (a command line) comes first when set.
pub fn terminal_candidates(_dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut list = Vec::new();
    if let Some(own) = std::env::var("TERMINAL")
        .ok()
        .filter(|t| !t.trim().is_empty())
    {
        let mut words = own.split_whitespace().map(str::to_string);
        if let Some(program) = words.next() {
            list.push((program, words.collect()));
        }
    }
    for program in [
        "x-terminal-emulator",
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "alacritty",
        "kitty",
        "xterm",
    ] {
        list.push((program.to_string(), Vec::new()));
    }
    list
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

/// The user's crontab as text (empty when there is none).
fn read_crontab() -> io::Result<String> {
    let out = std::process::Command::new("crontab")
        .arg("-l")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| io::Error::new(e.kind(), format!("crontab: {e}")))?;
    // "no crontab for user" is an error exit with nothing on stdout.
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn write_crontab(text: &str) -> io::Result<()> {
    use std::io::Write;
    let mut child = std::process::Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("crontab: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "crontab: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Adds (or replaces) the entry `id` in the user's crontab.
pub fn schedule_set(
    id: &str,
    when: ScheduleWhen,
    program: &Path,
    args: &[String],
) -> io::Result<()> {
    let line = cron_line(id, when, program, args);
    let (text, _) = cron_replace(&read_crontab()?, id, Some(&line));
    write_crontab(&text)
}

/// Removes the entry `id`; an error when there is none.
pub fn schedule_remove(id: &str) -> io::Result<()> {
    let (text, found) = cron_replace(&read_crontab()?, id, None);
    if !found {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "nie ma takiego zadania",
        ));
    }
    write_crontab(&text)
}

/// The entries bluepane made in the user's crontab.
pub fn schedule_list() -> io::Result<Vec<ScheduledTask>> {
    Ok(cron_tasks(&read_crontab()?))
}
