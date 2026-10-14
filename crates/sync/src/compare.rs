//! Comparing two scanned folders into a list of differences.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};

use bluepane_ops::{OpError, Progress};

use crate::scan::{Kind, Node, Tree};

/// How two files are judged equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareBy {
    /// Same size and modification times within `tolerance` (FAT stores 2 s resolution,
    /// and time zones or DST can shift timestamps by whole hours: the caller decides).
    TimeAndSize { tolerance: Duration },
    /// Same size only.
    Size,
    /// Same bytes, read from both sides (slow, but ignores dates altogether).
    Content,
}

impl Default for CompareBy {
    fn default() -> Self {
        CompareBy::TimeAndSize {
            tolerance: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    Equal,
    LeftOnly,
    RightOnly,
    /// Both exist and differ; the left one has the later modification time.
    LeftNewer,
    RightNewer,
    /// Both exist and differ, but dates do not say which is newer.
    Different,
    /// Cannot be classified (a file on one side, a directory on the other).
    Conflict(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    /// Relative path, `/` separated.
    pub path: String,
    pub left: Option<Node>,
    pub right: Option<Node>,
    pub category: Category,
}

/// Compares two trees. `left_root` and `right_root` are only read for [`CompareBy::Content`].
pub fn compare(
    left_root: &Path,
    right_root: &Path,
    left: &Tree,
    right: &Tree,
    by: CompareBy,
    progress: &Progress,
) -> Result<Vec<Diff>, OpError> {
    let paths: BTreeSet<&String> = left.nodes.keys().chain(right.nodes.keys()).collect();

    if by == CompareBy::Content {
        let (bytes, files) = paths
            .iter()
            .filter_map(|p| match (left.nodes.get(*p), right.nodes.get(*p)) {
                (Some(a), Some(b)) if both_files(a, b) && a.size == b.size => Some(a.size),
                _ => None,
            })
            .fold((0u64, 0u64), |(b, f), size| (b + size * 2, f + 1));
        progress.set_total(bytes, files);
    }

    let mut out = Vec::new();
    for path in paths {
        progress.check()?;
        let (l, r) = (
            left.nodes.get(path).copied(),
            right.nodes.get(path).copied(),
        );
        let category = match (&l, &r) {
            (Some(_), None) => Category::LeftOnly,
            (None, Some(_)) => Category::RightOnly,
            (Some(a), Some(b)) => classify(path, a, b, left_root, right_root, by, progress)?,
            (None, None) => unreachable!("the path came from one of the trees"),
        };
        out.push(Diff {
            path: path.clone(),
            left: l,
            right: r,
            category,
        });
    }
    Ok(out)
}

fn both_files(a: &Node, b: &Node) -> bool {
    a.kind == Kind::File && b.kind == Kind::File
}

fn classify(
    path: &str,
    a: &Node,
    b: &Node,
    left_root: &Path,
    right_root: &Path,
    by: CompareBy,
    progress: &Progress,
) -> Result<Category, OpError> {
    match (a.kind, b.kind) {
        (Kind::Dir, Kind::Dir) => return Ok(Category::Equal),
        (Kind::File, Kind::Dir) | (Kind::Dir, Kind::File) => {
            return Ok(Category::Conflict(
                "po jednej stronie plik, po drugiej katalog".into(),
            ))
        }
        (Kind::File, Kind::File) => {}
    }
    let equal = match by {
        CompareBy::Size => a.size == b.size,
        CompareBy::TimeAndSize { tolerance } => {
            a.size == b.size && order(a.modified, b.modified, tolerance) == Order::Same
        }
        CompareBy::Content => {
            a.size == b.size && same_bytes(&left_root.join(path), &right_root.join(path), progress)?
        }
    };
    if equal {
        return Ok(Category::Equal);
    }
    // Which one is newer is shown for every mode that can tell.
    let tolerance = match by {
        CompareBy::TimeAndSize { tolerance } => tolerance,
        _ => Duration::ZERO,
    };
    Ok(match order(a.modified, b.modified, tolerance) {
        Order::LeftLater => Category::LeftNewer,
        Order::RightLater => Category::RightNewer,
        Order::Same | Order::Unknown => Category::Different,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Order {
    Same,
    LeftLater,
    RightLater,
    Unknown,
}

fn order(a: Option<SystemTime>, b: Option<SystemTime>, tolerance: Duration) -> Order {
    let (Some(a), Some(b)) = (a, b) else {
        return Order::Unknown;
    };
    match a.duration_since(b) {
        Ok(ahead) if ahead > tolerance => Order::LeftLater,
        Ok(_) => Order::Same,
        Err(e) if e.duration() > tolerance => Order::RightLater,
        Err(_) => Order::Same,
    }
}

fn same_bytes(a: &Path, b: &Path, progress: &Progress) -> Result<bool, OpError> {
    progress.set_current(&a.display().to_string());
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        progress.check()?;
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        progress.add((na + nb) as u64);
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            progress.finish_item();
            return Ok(true);
        }
    }
}

fn read_full(file: &mut File, buf: &mut [u8]) -> Result<usize, OpError> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Filter;
    use crate::scan::scan;
    use std::fs;

    fn put(root: &Path, rel: &str, text: &str, secs: u64) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
    }

    fn run(by: CompareBy) -> (tempfile::TempDir, Vec<(String, Category)>) {
        let tmp = tempfile::tempdir().unwrap();
        let (l, r) = (tmp.path().join("l"), tmp.path().join("r"));
        put(&l, "same.txt", "same", 0);
        put(&r, "same.txt", "same", 0);
        put(&l, "new_left.txt", "n", 0);
        put(&r, "new_right.txt", "n", 0);
        put(&l, "edited.txt", "old", 0);
        put(&r, "edited.txt", "newer!", 100);
        put(&l, "edited_left.txt", "newer!", 100);
        put(&r, "edited_left.txt", "old", 0);
        put(&l, "dir/inner.txt", "x", 0);
        put(&r, "dir/inner.txt", "x", 1); // within the tolerance
        put(&l, "thing", "file", 0);
        fs::create_dir_all(r.join("thing")).unwrap();
        // Same size, same time, other bytes.
        put(&l, "samesize.txt", "abcd", 5);
        put(&r, "samesize.txt", "abce", 5);
        let (tl, tr) = (
            scan(&l, &Filter::default(), &Progress::new()).unwrap(),
            scan(&r, &Filter::default(), &Progress::new()).unwrap(),
        );
        let diffs = compare(&l, &r, &tl, &tr, by, &Progress::new()).unwrap();
        let rows = diffs.into_iter().map(|d| (d.path, d.category)).collect();
        (tmp, rows)
    }

    fn cat(rows: &[(String, Category)], path: &str) -> Category {
        rows.iter().find(|(p, _)| p == path).unwrap().1.clone()
    }

    #[test]
    fn time_and_size_classifies_every_case() {
        let (_tmp, rows) = run(CompareBy::default());
        assert_eq!(cat(&rows, "same.txt"), Category::Equal);
        assert_eq!(cat(&rows, "new_left.txt"), Category::LeftOnly);
        assert_eq!(cat(&rows, "new_right.txt"), Category::RightOnly);
        assert_eq!(cat(&rows, "edited.txt"), Category::RightNewer);
        assert_eq!(cat(&rows, "edited_left.txt"), Category::LeftNewer);
        assert_eq!(cat(&rows, "dir"), Category::Equal);
        assert_eq!(
            cat(&rows, "dir/inner.txt"),
            Category::Equal,
            "1 s is within 2 s"
        );
        assert!(matches!(cat(&rows, "thing"), Category::Conflict(_)));
        // Same size and same time: this mode cannot see the difference.
        assert_eq!(cat(&rows, "samesize.txt"), Category::Equal);
    }

    #[test]
    fn a_zero_tolerance_sees_one_second() {
        let (_tmp, rows) = run(CompareBy::TimeAndSize {
            tolerance: Duration::ZERO,
        });
        assert_eq!(cat(&rows, "dir/inner.txt"), Category::RightNewer);
    }

    #[test]
    fn content_mode_finds_other_bytes_with_equal_dates() {
        let (_tmp, rows) = run(CompareBy::Content);
        assert_eq!(cat(&rows, "samesize.txt"), Category::Different);
        assert_eq!(cat(&rows, "same.txt"), Category::Equal);
        assert_eq!(
            cat(&rows, "dir/inner.txt"),
            Category::Equal,
            "dates do not matter when the bytes match"
        );
    }

    #[test]
    fn size_mode_ignores_dates() {
        let (_tmp, rows) = run(CompareBy::Size);
        assert_eq!(cat(&rows, "dir/inner.txt"), Category::Equal);
        assert_eq!(cat(&rows, "edited.txt"), Category::RightNewer);
        assert_eq!(cat(&rows, "samesize.txt"), Category::Equal);
    }

    #[test]
    fn cancelling_a_content_comparison_stops_it() {
        let tmp = tempfile::tempdir().unwrap();
        let (l, r) = (tmp.path().join("l"), tmp.path().join("r"));
        put(&l, "a", "same", 0);
        put(&r, "a", "same", 0);
        let (tl, tr) = (
            scan(&l, &Filter::default(), &Progress::new()).unwrap(),
            scan(&r, &Filter::default(), &Progress::new()).unwrap(),
        );
        let progress = Progress::new();
        progress.cancel();
        assert!(matches!(
            compare(&l, &r, &tl, &tr, CompareBy::Content, &progress),
            Err(OpError::Cancelled)
        ));
    }
}
