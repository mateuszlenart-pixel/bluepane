//! File operations.
//!
//! Operations are synchronous functions that report progress and honour cancellation
//! through a shared [`Progress`] handle. Running them on a background thread and
//! queueing them is the job of the layer above (roadmap stage 4).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub enum OpError {
    Io(io::Error),
    Cancelled,
    /// The target already exists. A conflict dialog (overwrite / skip / rename) is stage 4.
    Exists(PathBuf),
    InvalidName(String),
    Trash(String),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Io(e) => write!(f, "{e}"),
            OpError::Cancelled => write!(f, "anulowano"),
            OpError::Exists(p) => write!(f, "już istnieje: {}", p.display()),
            OpError::InvalidName(n) => write!(f, "niepoprawna nazwa: {n:?}"),
            OpError::Trash(e) => write!(f, "kosz: {e}"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<io::Error> for OpError {
    fn from(e: io::Error) -> Self {
        OpError::Io(e)
    }
}

/// Shared progress and cancellation state. Cloning shares the same counters.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    cancel: Arc<AtomicBool>,
    bytes_done: Arc<AtomicU64>,
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn bytes_done(&self) -> u64 {
        self.bytes_done.load(Ordering::Relaxed)
    }

    fn add(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    fn check(&self) -> Result<(), OpError> {
        if self.is_cancelled() {
            Err(OpError::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn target_for(src: &Path, dst_dir: &Path) -> Result<PathBuf, OpError> {
    let name = src
        .file_name()
        .ok_or_else(|| OpError::InvalidName(src.display().to_string()))?;
    let target = dst_dir.join(name);
    if target.exists() {
        return Err(OpError::Exists(target));
    }
    Ok(target)
}

/// Copies a file or a directory tree into `dst_dir`, keeping its name.
/// Returns the path of the copy.
pub fn copy(src: &Path, dst_dir: &Path, progress: &Progress) -> Result<PathBuf, OpError> {
    let target = target_for(src, dst_dir)?;
    if src.is_dir() && target.starts_with(src) {
        return Err(OpError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nie można kopiować katalogu do jego wnętrza",
        )));
    }
    copy_recursive(src, &target, progress)?;
    Ok(target)
}

fn copy_recursive(src: &Path, dst: &Path, progress: &Progress) -> Result<(), OpError> {
    progress.check()?;
    if src.is_dir() {
        fs::create_dir(dst)?;
        for item in fs::read_dir(src)? {
            let item = item?;
            copy_recursive(&item.path(), &dst.join(item.file_name()), progress)?;
        }
    } else {
        let bytes = fs::copy(src, dst)?;
        progress.add(bytes);
    }
    Ok(())
}

/// Moves a file or directory into `dst_dir`. Falls back to copy + delete when a plain
/// rename is impossible (for example across volumes).
pub fn move_to(src: &Path, dst_dir: &Path, progress: &Progress) -> Result<PathBuf, OpError> {
    let target = target_for(src, dst_dir)?;
    if fs::rename(src, &target).is_ok() {
        return Ok(target);
    }
    copy(src, dst_dir, progress)?;
    if src.is_dir() {
        fs::remove_dir_all(src)?;
    } else {
        fs::remove_file(src)?;
    }
    Ok(target)
}

/// Creates a directory called `name` inside `parent`.
pub fn mkdir(parent: &Path, name: &str) -> Result<PathBuf, OpError> {
    let name = name.trim();
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || name.contains(':') && cfg!(windows);
    if bad {
        return Err(OpError::InvalidName(name.to_string()));
    }
    let path = parent.join(name);
    if path.exists() {
        return Err(OpError::Exists(path));
    }
    fs::create_dir(&path)?;
    Ok(path)
}

/// Moves a file or directory to the system trash (Recycle Bin on Windows, XDG trash on
/// Linux). Nothing in bluepane deletes permanently by default.
pub fn delete_to_trash(path: &Path) -> Result<(), OpError> {
    trash::delete(path).map_err(|e| OpError::Trash(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
        fs::write(tmp.path().join("src/a.txt"), "hello").unwrap();
        fs::write(tmp.path().join("src/sub/b.txt"), "world!").unwrap();
        fs::create_dir(tmp.path().join("dst")).unwrap();
        tmp
    }

    #[test]
    fn copies_a_tree_and_counts_bytes() {
        let tmp = tree();
        let progress = Progress::new();
        let out = copy(&tmp.path().join("src"), &tmp.path().join("dst"), &progress).unwrap();
        assert_eq!(fs::read_to_string(out.join("sub/b.txt")).unwrap(), "world!");
        assert_eq!(progress.bytes_done(), 11);
        assert!(tmp.path().join("src/a.txt").exists());
    }

    #[test]
    fn copy_refuses_to_overwrite() {
        let tmp = tree();
        let src = tmp.path().join("src/a.txt");
        let dst = tmp.path().join("dst");
        copy(&src, &dst, &Progress::new()).unwrap();
        assert!(matches!(
            copy(&src, &dst, &Progress::new()),
            Err(OpError::Exists(_))
        ));
    }

    #[test]
    fn copy_into_itself_is_rejected() {
        let tmp = tree();
        let src = tmp.path().join("src");
        assert!(copy(&src, &src.join("sub"), &Progress::new()).is_err());
    }

    #[test]
    fn move_removes_the_source() {
        let tmp = tree();
        let src = tmp.path().join("src");
        let out = move_to(&src, &tmp.path().join("dst"), &Progress::new()).unwrap();
        assert!(!src.exists());
        assert!(out.join("a.txt").exists());
    }

    #[test]
    fn cancelled_copy_stops() {
        let tmp = tree();
        let progress = Progress::new();
        progress.cancel();
        let res = copy(&tmp.path().join("src"), &tmp.path().join("dst"), &progress);
        assert!(matches!(res, Err(OpError::Cancelled)));
    }

    #[test]
    fn mkdir_validates_names() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(mkdir(tmp.path(), "ok").unwrap().is_dir());
        for bad in ["", "..", "a/b", "a\\b"] {
            assert!(matches!(
                mkdir(tmp.path(), bad),
                Err(OpError::InvalidName(_))
            ));
        }
        assert!(matches!(mkdir(tmp.path(), "ok"), Err(OpError::Exists(_))));
    }
}
