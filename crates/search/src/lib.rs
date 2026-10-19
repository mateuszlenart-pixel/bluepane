//! Search: file names and masks, full text, many folders and drives at once, and inside
//! archives (ZIP, TAR, TAR.GZ, 7z).
//!
//! [`search`] walks the roots and hands every [`Hit`] to a callback as soon as it is found,
//! so a window can show results while the search still runs. It honours cancellation and
//! reports progress through `bluepane_ops::Progress`. Nothing is ever written.

mod encoding;
mod matcher;
mod text;
mod walk;

use std::path::PathBuf;
use std::time::SystemTime;

pub use encoding::decode_cp1250;
pub use walk::search;

/// How the bytes of a file are read as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// A byte-order mark decides (UTF-8 / UTF-16); otherwise every line that is valid UTF-8
    /// is read as such and any other line as Windows-1250 (Central European).
    #[default]
    Auto,
    Utf8,
    Cp1250,
    /// UTF-16 without a byte-order mark is taken as little-endian.
    Utf16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextQuery {
    pub pattern: String,
    /// `pattern` is a regular expression.
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_words: bool,
    pub encoding: Encoding,
    /// Also look into files that look binary (they are skipped by default).
    pub search_binary: bool,
}

impl TextQuery {
    pub fn new(pattern: impl Into<String>) -> TextQuery {
        TextQuery {
            pattern: pattern.into(),
            regex: false,
            case_sensitive: false,
            whole_words: false,
            encoding: Encoding::Auto,
            search_binary: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// Folders (or drives) to search; a root inside another root is not searched twice.
    pub roots: Vec<PathBuf>,
    /// Name masks such as `*.rs;*.md` (`?` = one character), or a regular expression when
    /// `name_regex` is set. Empty matches every name.
    pub name: String,
    pub name_regex: bool,
    pub name_case_sensitive: bool,
    /// Name masks match with or without Polish accents (`zolw` finds `żółw`); not for
    /// regular expressions.
    pub name_fold_accents: bool,
    /// Masks of folder names that are not entered, e.g. `target;node_modules;.git`.
    pub exclude_dirs: String,
    /// When set, only files whose content matches are found.
    pub text: Option<TextQuery>,
    /// Look inside archives too.
    pub archives: bool,
    pub archive_password: Option<String>,
    pub include_hidden: bool,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<SystemTime>,
    pub modified_before: Option<SystemTime>,
    /// Stop after this many hits (0 = no limit).
    pub max_results: usize,
    /// Files inside archives larger than this are not read for text.
    pub max_archive_entry: u64,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            roots: Vec::new(),
            name: String::new(),
            name_regex: false,
            name_case_sensitive: false,
            name_fold_accents: true,
            exclude_dirs: String::new(),
            text: None,
            archives: false,
            archive_password: None,
            include_hidden: true,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            max_results: 10_000,
            max_archive_entry: 64 * 1024 * 1024,
        }
    }
}

/// Where a hit is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    File(PathBuf),
    /// A file inside an archive; `inner` is `/` separated.
    InArchive {
        archive: PathBuf,
        inner: String,
    },
}

impl Location {
    /// The last component of the path.
    pub fn name(&self) -> String {
        match self {
            Location::File(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            Location::InArchive { inner, .. } => {
                inner.rsplit('/').next().unwrap_or(inner).to_string()
            }
        }
    }

    /// A readable path: `dir/file`, or `archive.zip > dir/file`.
    pub fn display(&self) -> String {
        match self {
            Location::File(path) => path.display().to_string(),
            Location::InArchive { archive, inner } => {
                format!("{} > {inner}", archive.display())
            }
        }
    }
}

/// One matching line of a text search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHit {
    /// 1-based line number.
    pub line: u64,
    /// The line (cut around the match when very long).
    pub text: String,
    /// Character offsets of the first match inside `text`.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub location: Location,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Number of matching lines (text search only; counting stops at [`MAX_COUNTED_LINES`]).
    pub matching_lines: u64,
    /// The first matching lines ([`MAX_SHOWN_LINES`] at most).
    pub lines: Vec<LineHit>,
}

/// Matching lines per file shown in a [`Hit`].
pub const MAX_SHOWN_LINES: usize = 50;
/// A file is not read past this many matching lines.
pub const MAX_COUNTED_LINES: u64 = 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub folders: u64,
    pub files: u64,
    pub archives: u64,
    pub hits: u64,
    /// Folders, files or archives that could not be read.
    pub unreadable: u64,
    /// Encrypted archive content that was left out for lack of a (correct) password.
    pub encrypted: u64,
    /// Files skipped because they look binary.
    pub binary: u64,
    /// The search stopped at `max_results`.
    pub truncated: bool,
    pub cancelled: bool,
}
