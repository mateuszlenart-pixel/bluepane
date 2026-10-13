//! The table of contents of an archive.

use std::collections::{BTreeMap, HashMap};
use std::time::SystemTime;

use crate::format::Format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Path inside the archive: `/` separated, no leading or trailing slash, never containing
    /// `.` or `..` components.
    pub path: String,
    pub is_dir: bool,
    /// Uncompressed size (0 for directories).
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub encrypted: bool,
    /// Unix permission bits when the archive stores them.
    pub mode: Option<u32>,
    /// A symlink, hard link or device: listed, but never extracted.
    pub special: bool,
}

impl ArchiveEntry {
    /// The last path component.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// A cleaned-up archive entry name, or `None` for an unsafe one (absolute path, `..`, a
/// drive prefix, NUL). Backslashes count as separators, as many Windows tools write them.
pub fn normalize_name(name: &str) -> Option<String> {
    if name.contains('\0') {
        return None;
    }
    let unified = name.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            other => parts.push(other),
        }
    }
    let first = parts.first()?;
    // `C:` and friends would make Windows treat the entry as an absolute path.
    if first.len() >= 2 && first.as_bytes()[1] == b':' && first.as_bytes()[0].is_ascii_alphabetic()
    {
        return None;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return None;
    }
    Some(parts.join("/"))
}

/// Whether `path` is the selection `selected` or lies below it. The empty selection is the
/// whole archive.
pub fn is_selected(path: &str, selected: &str) -> bool {
    selected.is_empty()
        || path == selected
        || (path.starts_with(selected) && path.as_bytes().get(selected.len()) == Some(&b'/'))
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

#[derive(Debug, Clone)]
pub struct Listing {
    pub format: Format,
    /// Number of volumes the archive is split into (1 when it is not split).
    pub volumes: usize,
    /// Size of the archive on disk, all volumes together.
    pub packed_size: u64,
    /// Entries with unsafe names that were left out of the listing.
    pub unsafe_skipped: usize,
    /// Every entry, directories included, sorted by path.
    pub entries: Vec<ArchiveEntry>,
    children: HashMap<String, Vec<usize>>,
}

impl Listing {
    /// Builds the listing from raw entries: duplicates collapse (the last one wins),
    /// directories that are only implied by file paths are added, entries are sorted.
    pub(crate) fn build(
        format: Format,
        volumes: usize,
        packed_size: u64,
        raw: Vec<ArchiveEntry>,
        unsafe_skipped: usize,
    ) -> Listing {
        let mut by_path: BTreeMap<String, ArchiveEntry> = BTreeMap::new();
        for entry in raw {
            by_path.insert(entry.path.clone(), entry);
        }
        let implied: Vec<String> = by_path
            .keys()
            .flat_map(|path| {
                let mut parents = Vec::new();
                let mut dir = parent_of(path);
                while !dir.is_empty() {
                    parents.push(dir.to_string());
                    dir = parent_of(dir);
                }
                parents
            })
            .collect();
        for dir in implied {
            by_path.entry(dir.clone()).or_insert_with(|| ArchiveEntry {
                path: dir,
                is_dir: true,
                size: 0,
                modified: None,
                encrypted: false,
                mode: None,
                special: false,
            });
        }
        let entries: Vec<ArchiveEntry> = by_path.into_values().collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            children
                .entry(parent_of(&entry.path).to_string())
                .or_default()
                .push(index);
        }
        Listing {
            format,
            volumes,
            packed_size,
            unsafe_skipped,
            entries,
            children,
        }
    }

    /// The entries directly inside `dir` (`""` is the top level).
    pub fn children(&self, dir: &str) -> impl Iterator<Item = &ArchiveEntry> {
        self.children
            .get(dir)
            .into_iter()
            .flatten()
            .map(|&i| &self.entries[i])
    }

    pub fn find(&self, path: &str) -> Option<&ArchiveEntry> {
        self.entries
            .binary_search_by(|e| e.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.entries[i])
    }

    pub fn has_encrypted(&self) -> bool {
        self.entries.iter().any(|e| e.encrypted)
    }

    /// Total uncompressed bytes and number of extractable files below the selections
    /// (empty list = everything).
    pub fn measure(&self, selections: &[String]) -> (u64, u64) {
        let mut bytes = 0;
        let mut files = 0;
        for entry in self.entries.iter().filter(|e| !e.is_dir && !e.special) {
            if selections.is_empty() || selections.iter().any(|s| is_selected(&entry.path, s)) {
                bytes += entry.size;
                files += 1;
            }
        }
        (bytes, files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, is_dir: bool, size: u64) -> ArchiveEntry {
        ArchiveEntry {
            path: path.to_string(),
            is_dir,
            size,
            modified: None,
            encrypted: false,
            mode: None,
            special: false,
        }
    }

    fn sample() -> Listing {
        Listing::build(
            Format::Zip,
            1,
            0,
            vec![
                entry("docs/a.txt", false, 10),
                entry("docs/sub/b.txt", false, 20),
                entry("top.txt", false, 5),
                entry("empty", true, 0),
            ],
            0,
        )
    }

    #[test]
    fn names_are_cleaned_and_unsafe_ones_rejected() {
        assert_eq!(normalize_name("a/b.txt").as_deref(), Some("a/b.txt"));
        assert_eq!(normalize_name("./a//b/").as_deref(), Some("a/b"));
        assert_eq!(
            normalize_name(r"win\dir\f.txt").as_deref(),
            Some("win/dir/f.txt")
        );
        for bad in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            r"\share\x",
            "C:/x",
            "c:\\x",
            "a\0b",
            "",
            "./",
            "..",
        ] {
            assert_eq!(normalize_name(bad), None, "{bad:?}");
        }
        // A dot-dot inside a name is fine; only a whole component is dangerous.
        assert_eq!(normalize_name("a..b/c").as_deref(), Some("a..b/c"));
    }

    #[test]
    fn selection_matches_the_item_and_everything_below_it() {
        assert!(is_selected("docs", "docs"));
        assert!(is_selected("docs/a.txt", "docs"));
        assert!(!is_selected("docs2/a.txt", "docs"));
        assert!(!is_selected("doc", "docs"));
        assert!(is_selected("anything/at/all", ""));
    }

    #[test]
    fn missing_parent_directories_are_added() {
        let listing = sample();
        let paths: Vec<_> = listing.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "docs",
                "docs/a.txt",
                "docs/sub",
                "docs/sub/b.txt",
                "empty",
                "top.txt"
            ]
        );
        assert!(listing.find("docs").unwrap().is_dir);
        assert!(listing.find("docs/sub").unwrap().is_dir);
        assert!(listing.find("nope").is_none());
    }

    #[test]
    fn children_lists_one_level() {
        let listing = sample();
        let names = |dir: &str| -> Vec<String> {
            listing
                .children(dir)
                .map(|e| e.name().to_string())
                .collect()
        };
        assert_eq!(names(""), ["docs", "empty", "top.txt"]);
        assert_eq!(names("docs"), ["a.txt", "sub"]);
        assert_eq!(names("docs/sub"), ["b.txt"]);
        assert!(names("missing").is_empty());
    }

    #[test]
    fn duplicates_collapse_and_the_last_one_wins() {
        let listing = Listing::build(
            Format::Tar,
            1,
            0,
            vec![entry("f", false, 1), entry("f", false, 2)],
            0,
        );
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.find("f").unwrap().size, 2);
    }

    #[test]
    fn measure_counts_extractable_files_below_selections() {
        let mut raw = vec![
            entry("d/a", false, 10),
            entry("d/b", false, 20),
            entry("x", false, 5),
        ];
        raw.push(ArchiveEntry {
            special: true,
            ..entry("d/link", false, 99)
        });
        let listing = Listing::build(Format::Tar, 1, 0, raw, 0);
        assert_eq!(listing.measure(&[]), (35, 3));
        assert_eq!(listing.measure(&["d".to_string()]), (30, 2));
        assert_eq!(
            listing.measure(&["x".to_string(), "d/a".to_string()]),
            (15, 2)
        );
    }
}
