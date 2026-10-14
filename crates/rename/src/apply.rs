use std::fs;
use std::path::{Path, PathBuf};

use crate::plan::{Plan, Status};

/// One rename that was carried out; kept so it can be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Applies every [`Status::Ready`] row. Refuses to start when any row is a conflict.
/// On failure everything done so far is rolled back, so the result is all or nothing.
pub fn apply(plan: &Plan) -> Result<Vec<Renamed>, String> {
    if let Some(row) = plan
        .rows
        .iter()
        .find(|r| matches!(r.status, Status::Conflict(_)))
    {
        let Status::Conflict(why) = &row.status else {
            unreachable!()
        };
        return Err(format!("konflikt: {} ({why})", row.old_name));
    }
    let pairs: Vec<Renamed> = plan
        .rows
        .iter()
        .filter(|r| r.status == Status::Ready)
        .map(|r| Renamed {
            from: r.from.clone(),
            to: r.from.with_file_name(&r.new_name),
        })
        .collect();
    rename_all(&pairs)?;
    Ok(pairs)
}

/// Reverses a previous [`apply`].
pub fn undo(done: &[Renamed]) -> Result<(), String> {
    let reversed: Vec<Renamed> = done
        .iter()
        .map(|r| Renamed {
            from: r.to.clone(),
            to: r.from.clone(),
        })
        .collect();
    rename_all(&reversed)
}

/// Two phases: everything moves to a unique temporary name first, then to its final name.
/// That makes swaps and chains safe, and case-only changes work on case-insensitive disks.
fn rename_all(pairs: &[Renamed]) -> Result<(), String> {
    let mut temps: Vec<PathBuf> = Vec::with_capacity(pairs.len());
    for (index, pair) in pairs.iter().enumerate() {
        let temp = temp_name(&pair.from, index);
        if let Err(e) = fs::rename(&pair.from, &temp) {
            rollback(pairs, &temps, 0);
            return Err(format!("{}: {e}", pair.from.display()));
        }
        temps.push(temp);
    }
    for (index, pair) in pairs.iter().enumerate() {
        // `fs::rename` replaces files on Unix: make sure nothing is overwritten.
        let result = if fs::symlink_metadata(&pair.to).is_ok() {
            Err(format!("{} już istnieje", pair.to.display()))
        } else {
            fs::rename(&temps[index], &pair.to).map_err(|e| format!("{}: {e}", pair.to.display()))
        };
        if let Err(e) = result {
            rollback(pairs, &temps, index);
            return Err(e);
        }
    }
    Ok(())
}

/// Puts back what was moved: the first `finished` pairs sit at their final names, all
/// of `temps` at temporary ones (or, for the finished ones, no longer).
fn rollback(pairs: &[Renamed], temps: &[PathBuf], finished: usize) {
    for (index, pair) in pairs.iter().enumerate().take(finished) {
        let _ = fs::rename(&pair.to, &temps[index]);
    }
    for (index, temp) in temps.iter().enumerate() {
        let _ = fs::rename(temp, &pairs[index].from);
    }
}

fn temp_name(original: &Path, index: usize) -> PathBuf {
    let parent = original.parent().unwrap_or(Path::new("."));
    let mut attempt = 0u32;
    loop {
        let candidate = parent.join(format!(
            ".bluepane-rename-{}-{index}-{attempt}",
            std::process::id()
        ));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plan, Item, Settings};
    use std::collections::HashMap;

    fn item(path: &Path) -> Item {
        Item {
            path: path.to_path_buf(),
            is_dir: path.is_dir(),
            modified: None,
        }
    }

    fn read(path: PathBuf) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn renames_and_undoes() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a.txt"), tmp.path().join("b.txt"));
        fs::write(&a, "A").unwrap();
        fs::write(&b, "B").unwrap();
        let s = Settings {
            name_mask: "f[C]".into(),
            ..Settings::default()
        };
        let p = plan(&[item(&a), item(&b)], &s, &HashMap::new()).unwrap();
        let done = apply(&p).unwrap();
        assert_eq!(done.len(), 2);
        assert_eq!(read(tmp.path().join("f1.txt")), "A");
        assert_eq!(read(tmp.path().join("f2.txt")), "B");
        assert!(!a.exists() && !b.exists());

        undo(&done).unwrap();
        assert_eq!(read(a), "A");
        assert_eq!(read(b), "B");
        assert_eq!(
            fs::read_dir(tmp.path()).unwrap().count(),
            2,
            "no temp leftovers"
        );
    }

    #[test]
    fn swaps_and_chains_work() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b, c) = (
            tmp.path().join("a"),
            tmp.path().join("b"),
            tmp.path().join("c"),
        );
        for (p, t) in [(&a, "A"), (&b, "B"), (&c, "C")] {
            fs::write(p, t).unwrap();
        }
        let mut manual = HashMap::new();
        manual.insert(a.clone(), "b".to_string());
        manual.insert(b.clone(), "a".to_string());
        manual.insert(c.clone(), "d".to_string());
        let p = plan(
            &[item(&a), item(&b), item(&c)],
            &Settings::default(),
            &manual,
        )
        .unwrap();
        assert_eq!(p.conflicts(), 0);
        apply(&p).unwrap();
        assert_eq!(read(a), "B");
        assert_eq!(read(b), "A");
        assert_eq!(read(tmp.path().join("d")), "C");
        assert!(!c.exists());
    }

    #[test]
    fn case_only_changes_work() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("photo.jpg");
        fs::write(&a, "x").unwrap();
        let s = Settings {
            case: crate::CaseMode::Upper,
            ..Settings::default()
        };
        let p = plan(&[item(&a)], &s, &HashMap::new()).unwrap();
        apply(&p).unwrap();
        let names: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["PHOTO.JPG"]);
    }

    #[test]
    fn directories_are_renamed_too() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("old");
        fs::create_dir(&d).unwrap();
        fs::write(d.join("inner"), "i").unwrap();
        let s = Settings {
            name_mask: "new".into(),
            ..Settings::default()
        };
        let p = plan(&[item(&d)], &s, &HashMap::new()).unwrap();
        apply(&p).unwrap();
        assert_eq!(read(tmp.path().join("new/inner")), "i");
    }

    #[test]
    fn conflicts_block_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fs::write(&a, "A").unwrap();
        fs::write(&b, "B").unwrap();
        let s = Settings {
            name_mask: "same".into(),
            ..Settings::default()
        };
        let p = plan(&[item(&a), item(&b)], &s, &HashMap::new()).unwrap();
        assert!(apply(&p).is_err());
        assert_eq!(read(a), "A");
        assert_eq!(read(b), "B");
    }

    #[test]
    fn a_file_appearing_after_the_preview_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fs::write(&a, "A").unwrap();
        fs::write(&b, "B").unwrap();
        let s = Settings {
            name_mask: "n[C]".into(),
            ..Settings::default()
        };
        let p = plan(&[item(&a), item(&b)], &s, &HashMap::new()).unwrap();
        // Someone creates `n2` between the preview and the apply.
        fs::write(tmp.path().join("n2"), "intruder").unwrap();
        assert!(apply(&p).is_err());
        assert_eq!(read(a), "A", "rolled back");
        assert_eq!(read(b), "B", "rolled back");
        assert_eq!(read(tmp.path().join("n2")), "intruder");
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 3);
    }
}
