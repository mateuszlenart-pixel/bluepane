use std::fs::{self, Metadata};
use std::io;
use std::os::windows::fs::{symlink_dir, symlink_file, MetadataExt};
use std::path::{Path, PathBuf};

use super::verbatim::{to_verbatim, LONG_PATH_THRESHOLD};
use crate::schedule::{schtasks_create, schtasks_tasks, task_name};
use crate::{ScheduleWhen, ScheduledTask, Volume};

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// Hidden or system attribute set.
pub fn is_hidden(_name: &str, meta: &Metadata) -> bool {
    meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

/// Runs a program in `dir` and collects its output; no console window flashes up.
pub fn run_hidden(program: &str, args: &[&str], dir: &Path) -> io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

/// Windows has no Unix permission bits.
pub fn entry_mode(_meta: &Metadata) -> Option<u32> {
    None
}

/// Reading the owner needs the security API; not shown on Windows.
pub fn entry_owner(_meta: &Metadata) -> Option<String> {
    None
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

/// `explorer <file>` opens the file with its associated application.
pub const DEFAULT_OPENER: (&str, &[&str]) = ("explorer", &[]);

/// Windows has no Unix permission bits.
pub fn file_mode(_path: &Path) -> Option<u32> {
    None
}

/// Windows has no Unix permission bits: nothing to set.
pub fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// The neighbours (IP address, MAC address) Windows has resolved lately (`arp -a`).
pub fn neighbours() -> Vec<(std::net::Ipv4Addr, String)> {
    use std::os::windows::process::CommandExt;
    // No console window flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("arp")
        .arg("-a")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| super::arp::parse_arp_a(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

fn schtasks(args: &[String]) -> io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("schtasks")
        .args(args)
        .stdin(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| io::Error::new(e.kind(), format!("schtasks: {e}")))
}

fn schtasks_checked(args: &[String]) -> io::Result<std::process::Output> {
    let out = schtasks(args)?;
    if out.status.success() {
        Ok(out)
    } else {
        let mut text = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if text.is_empty() {
            text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
        Err(io::Error::other(format!("schtasks: {text}")))
    }
}

/// Adds (or replaces) the task `bluepane\<id>` in the Task Scheduler.
pub fn schedule_set(
    id: &str,
    when: ScheduleWhen,
    program: &Path,
    args: &[String],
) -> io::Result<()> {
    schtasks_checked(&schtasks_create(id, when, program, args)).map(|_| ())
}

/// Removes the task; an error when there is none.
pub fn schedule_remove(id: &str) -> io::Result<()> {
    let args = ["/Delete", "/F", "/TN"]
        .map(String::from)
        .into_iter()
        .chain([task_name(id)])
        .collect::<Vec<_>>();
    schtasks_checked(&args).map(|_| ())
}

/// The tasks bluepane made in its folder of the Task Scheduler.
pub fn schedule_list() -> io::Result<Vec<ScheduledTask>> {
    let args = ["/Query", "/FO", "CSV", "/NH"].map(String::from).to_vec();
    let out = schtasks_checked(&args)?;
    Ok(schtasks_tasks(&String::from_utf8_lossy(&out.stdout)))
}
