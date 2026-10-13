//! Carrying out a plan.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bluepane_ops::{self as ops, Conflict, Host, OpError, Progress, Resolution};

use crate::plan::{Op, Plan};

/// What happens to a file that is deleted or replaced by synchronization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deletion {
    /// The system trash (the default: nothing is lost for good).
    Trash,
    /// Moved into this folder, keeping the relative path; an older version with the same
    /// name is kept too (the new one gets a `(kopia)` name).
    Versioning(PathBuf),
    /// Gone for good. Only for a user who asked for exactly that.
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOptions {
    pub deletion: Deletion,
}

impl Default for ExecOptions {
    fn default() -> Self {
        ExecOptions {
            deletion: Deletion::Trash,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExecReport {
    pub copied: u64,
    pub deleted: u64,
    pub bytes: u64,
    /// One message per action that failed; the rest of the plan still ran.
    pub failed: Vec<String>,
    /// Conflicts and skipped items: left exactly as they were.
    pub left_alone: u64,
}

/// Runs the copies and deletions of `plan`. Deletions go first (deepest first, so a
/// directory is emptied before it is removed) to free space, then copies in path order
/// (so a directory exists before what goes into it). A failed action is reported and the
/// rest continues; cancelling stops at once.
pub fn execute(
    plan: &Plan,
    left_root: &Path,
    right_root: &Path,
    options: &ExecOptions,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<ExecReport, OpError> {
    let mut report = ExecReport::default();
    progress.set_total(plan.bytes(), (plan.copies() + plan.deletions()) as u64);

    let mut deletions: Vec<_> = plan
        .actions
        .iter()
        .filter(|a| matches!(a.op, Op::DeleteLeft | Op::DeleteRight))
        .collect();
    deletions.sort_by(|a, b| b.path.cmp(&a.path));
    for action in deletions {
        progress.check()?;
        progress.set_current(&action.path);
        let root = if action.op == Op::DeleteLeft {
            left_root
        } else {
            right_root
        };
        match remove(
            &root.join(&action.path),
            &action.path,
            &options.deletion,
            host,
        ) {
            Ok(()) => report.deleted += 1,
            Err(OpError::Cancelled) => return Err(OpError::Cancelled),
            Err(e) => report.failed.push(format!("{}: {e}", action.path)),
        }
        progress.finish_item();
    }

    for action in &plan.actions {
        let (from, to) = match action.op {
            Op::CopyToRight => (left_root, right_root),
            Op::CopyToLeft => (right_root, left_root),
            Op::DeleteLeft | Op::DeleteRight => continue,
            Op::Skip | Op::Conflict(_) => {
                report.left_alone += 1;
                continue;
            }
        };
        progress.check()?;
        progress.set_current(&action.path);
        match copy_one(
            &from.join(&action.path),
            &to.join(&action.path),
            &action.path,
            action.is_dir,
            options,
            progress,
            host,
        ) {
            Ok(()) => {
                report.copied += 1;
                report.bytes += action.bytes;
            }
            Err(OpError::Cancelled) => return Err(OpError::Cancelled),
            Err(e) => report.failed.push(format!("{}: {e}", action.path)),
        }
        progress.finish_item();
    }
    Ok(report)
}

/// Overwrites without asking (the plan already said so) but passes platform work through.
struct Overwriting<'a>(&'a mut dyn Host);

impl Host for Overwriting<'_> {
    fn conflict(&mut self, _: &Conflict) -> Result<Resolution, OpError> {
        Ok(Resolution::Overwrite)
    }
    fn make_writable(&self, path: &Path) {
        self.0.make_writable(path)
    }
    fn copy_symlink(&self, src: &Path, dst: &Path) -> io::Result<()> {
        self.0.copy_symlink(src, dst)
    }
    fn tick(&mut self, progress: &Progress) {
        self.0.tick(progress)
    }
}

/// Moves old versions aside under `(kopia)` names instead of asking.
struct KeepingBoth<'a>(&'a mut dyn Host);

impl Host for KeepingBoth<'_> {
    fn conflict(&mut self, _: &Conflict) -> Result<Resolution, OpError> {
        Ok(Resolution::KeepBoth)
    }
    fn make_writable(&self, path: &Path) {
        self.0.make_writable(path)
    }
}

fn copy_one(
    src: &Path,
    dst: &Path,
    relative: &str,
    is_dir: bool,
    options: &ExecOptions,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<(), OpError> {
    if is_dir {
        return Ok(fs::create_dir_all(dst)?);
    }
    let parent = dst.parent().ok_or_else(|| {
        OpError::InvalidName(format!("brak katalogu nadrzędnego: {}", dst.display()))
    })?;
    fs::create_dir_all(parent)?;
    if fs::symlink_metadata(dst).is_ok() && options.deletion != Deletion::Permanent {
        // The old file is set aside first; with `Permanent` it is overwritten in place.
        remove(dst, relative, &options.deletion, host)?;
    }
    ops::copy_with(src, parent, progress, &mut Overwriting(host))?;
    Ok(())
}

fn remove(
    path: &Path,
    relative: &str,
    deletion: &Deletion,
    host: &mut dyn Host,
) -> Result<(), OpError> {
    match deletion {
        Deletion::Trash => ops::delete_to_trash(path),
        Deletion::Permanent => {
            let meta = fs::symlink_metadata(path)?;
            host.make_writable(path);
            if meta.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
            Ok(())
        }
        Deletion::Versioning(folder) => {
            let target_dir = match Path::new(relative).parent() {
                Some(parent) if !parent.as_os_str().is_empty() => folder.join(parent),
                _ => folder.clone(),
            };
            fs::create_dir_all(&target_dir)?;
            ops::move_with(path, &target_dir, &Progress::new(), &mut KeepingBoth(host))?;
            Ok(())
        }
    }
}
