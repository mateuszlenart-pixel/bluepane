//! Reading one side of a comparison into a flat map keyed by relative path.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use bluepane_ops::{OpError, Progress};

use crate::filter::Filter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    /// Bytes for files, 0 for directories.
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Everything below a folder. Paths use `/` on every system.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub nodes: BTreeMap<String, Node>,
    /// Symbolic links, devices and the like: never followed, never synchronized.
    pub skipped_links: u64,
    /// Entries that could not be read, with the reason.
    pub unreadable: Vec<String>,
}

/// Reads `root` recursively. Directories that hold nothing allowed by the filter are
/// dropped when the filter has an include list (an empty `src/` should not appear just
/// because `*.rs` was asked for).
pub fn scan(root: &Path, filter: &Filter, progress: &Progress) -> Result<Tree, OpError> {
    let mut tree = Tree::default();
    walk(root, "", filter, progress, &mut tree)?;
    if filter.has_include() {
        prune_empty_dirs(&mut tree);
    }
    Ok(tree)
}

fn walk(
    dir: &Path,
    prefix: &str,
    filter: &Filter,
    progress: &Progress,
    tree: &mut Tree,
) -> Result<(), OpError> {
    progress.check()?;
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tree.unreadable
                .push(format!("{}: {e}", display(prefix, dir)));
            return Ok(());
        }
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if prefix.is_empty() && name == crate::state::STATE_FILE {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let meta = match fs::symlink_metadata(entry.path()) {
            Ok(meta) => meta,
            Err(e) => {
                tree.unreadable.push(format!("{path}: {e}"));
                continue;
            }
        };
        let file_type = meta.file_type();
        if file_type.is_dir() {
            if !filter.allows(&path, true) {
                continue;
            }
            tree.nodes.insert(
                path.clone(),
                Node {
                    kind: Kind::Dir,
                    size: 0,
                    modified: meta.modified().ok(),
                },
            );
            if filter.enters_dir(&path) {
                progress.set_current(&path);
                walk(&entry.path(), &path, filter, progress, tree)?;
            }
        } else if file_type.is_file() {
            if !filter.allows(&path, false) {
                continue;
            }
            tree.nodes.insert(
                path,
                Node {
                    kind: Kind::File,
                    size: meta.len(),
                    modified: meta.modified().ok(),
                },
            );
        } else if filter.allows(&path, false) {
            tree.skipped_links += 1;
        }
    }
    Ok(())
}

fn display(prefix: &str, dir: &Path) -> String {
    if prefix.is_empty() {
        dir.display().to_string()
    } else {
        prefix.to_string()
    }
}

fn prune_empty_dirs(tree: &mut Tree) {
    let files: Vec<String> = tree
        .nodes
        .iter()
        .filter(|(_, n)| n.kind == Kind::File)
        .map(|(p, _)| p.clone())
        .collect();
    tree.nodes.retain(|path, node| {
        node.kind == Kind::File
            || files.iter().any(|f| {
                f.strip_prefix(path.as_str())
                    .is_some_and(|r| r.starts_with('/'))
            })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn lists_files_and_directories_with_slash_paths() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("a.txt"), "abc");
        write(&tmp.path().join("d/e/f.txt"), "z");
        fs::create_dir(tmp.path().join("empty")).unwrap();
        let tree = scan(tmp.path(), &Filter::default(), &Progress::new()).unwrap();
        let keys: Vec<_> = tree.nodes.keys().map(String::as_str).collect();
        assert_eq!(keys, ["a.txt", "d", "d/e", "d/e/f.txt", "empty"]);
        assert_eq!(tree.nodes["a.txt"].size, 3);
        assert_eq!(tree.nodes["d"].kind, Kind::Dir);
    }

    #[test]
    fn the_filter_hides_files_and_whole_directories() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("keep.txt"), "1");
        write(&tmp.path().join("skip.tmp"), "1");
        write(&tmp.path().join("cache/x.txt"), "1");
        let tree = scan(
            tmp.path(),
            &Filter::new("", "*.tmp;cache/"),
            &Progress::new(),
        )
        .unwrap();
        let keys: Vec<_> = tree.nodes.keys().map(String::as_str).collect();
        assert_eq!(keys, ["keep.txt"]);
    }

    #[test]
    fn an_include_list_drops_directories_with_nothing_wanted() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("src/a.rs"), "1");
        write(&tmp.path().join("img/a.png"), "1");
        let tree = scan(tmp.path(), &Filter::new("*.rs", ""), &Progress::new()).unwrap();
        let keys: Vec<_> = tree.nodes.keys().map(String::as_str).collect();
        assert_eq!(keys, ["src", "src/a.rs"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_counted_and_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("real/f.txt"), "1");
        std::os::unix::fs::symlink(tmp.path().join("real"), tmp.path().join("link")).unwrap();
        let tree = scan(tmp.path(), &Filter::default(), &Progress::new()).unwrap();
        assert!(!tree.nodes.contains_key("link"));
        assert!(!tree.nodes.contains_key("link/f.txt"));
        assert_eq!(tree.skipped_links, 1);
    }

    #[test]
    fn a_missing_root_is_reported_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let tree = scan(
            &tmp.path().join("nope"),
            &Filter::default(),
            &Progress::new(),
        )
        .unwrap();
        assert!(tree.nodes.is_empty());
        assert_eq!(tree.unreadable.len(), 1);
    }

    #[test]
    fn cancelling_stops_the_scan() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("a/b.txt"), "1");
        let progress = Progress::new();
        progress.cancel();
        assert!(matches!(
            scan(tmp.path(), &Filter::default(), &progress),
            Err(OpError::Cancelled)
        ));
    }
}
