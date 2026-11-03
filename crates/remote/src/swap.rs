//! Replacing a file with a finished `.bpart` without ever leaving the target missing.
//!
//! The old code deleted the target and then renamed the part file in; when the rename failed
//! (server refusal, dropped connection, a file held open on Windows) the old version was gone
//! and the new one not in place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Suffix of the previous version while a replacement is in progress.
pub(crate) const BACKUP: &str = ".bpold";

pub(crate) fn backup_of(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(BACKUP);
    PathBuf::from(name)
}

/// Moves `part` over `target`. A plain rename replaces the target in one step where the
/// platform allows it; otherwise the old file is moved aside first and put back if the new
/// one cannot be moved in. On failure the old file is still at `target` (or, if even the
/// restore failed, at the path named in the error).
pub(crate) fn replace_file(part: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(part, target) {
        Ok(()) => Ok(()),
        Err(first) => {
            if fs::symlink_metadata(target).is_err() || fs::symlink_metadata(part).is_err() {
                return Err(first);
            }
            let backup = backup_of(target);
            if fs::symlink_metadata(&backup).is_ok() {
                return Err(first);
            }
            fs::rename(target, &backup)?;
            if let Err(e) = fs::rename(part, target) {
                return match fs::rename(&backup, target) {
                    Ok(()) => Err(e),
                    Err(_) => Err(io::Error::new(
                        e.kind(),
                        format!("{e}; poprzednia wersja pozostała jako {}", backup.display()),
                    )),
                };
            }
            let _ = fs::remove_file(&backup);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_part_file_leaves_the_old_target_alone() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        fs::write(&target, "old").unwrap();
        assert!(replace_file(&dir.path().join("a.txt.bpart"), &target).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    }

    #[test]
    fn the_part_file_replaces_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let (target, part) = (dir.path().join("zażółć.txt"), dir.path().join("p.bpart"));
        fs::write(&target, "old").unwrap();
        fs::write(&part, "new").unwrap();
        replace_file(&part, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(!part.exists() && !backup_of(&target).exists());
    }
}
