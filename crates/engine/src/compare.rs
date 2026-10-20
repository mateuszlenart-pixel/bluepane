//! Comparing two files (`compare`) and checksum files (`checksum`), both shown in one tab.
//!
//! Text files are compared line by line and laid out side by side with the differences marked;
//! binary files report the first differing byte. Checksums are made and verified for `.sha256`
//! and `.md5` files in the usual `sum  name` format. Reading happens in a background thread
//! and only after the command; nothing is compared or hashed on its own.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use md5::Md5;
use sha2::{Digest, Sha256};

use crate::panel::is_parent_entry;
use crate::session::Session;
use crate::EngineError;

/// Text files above this size are compared as bytes only.
const TEXT_LIMIT: u64 = 4 << 20;
/// The line-matching table of the differing middle part may have this many cells at most.
const MATCH_CELLS: usize = 4_000_000;

/// What `compare` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareAction {
    /// Compare the two marked files, or the file under the cursor with the one in the other panel.
    Files,
    Close,
    /// Go to the next / previous difference.
    Next,
    Prev,
}

/// The sum algorithms of `.sha256` and `.md5` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Sha256,
    Md5,
}

impl Algo {
    pub fn extension(self) -> &'static str {
        match self {
            Algo::Sha256 => "sha256",
            Algo::Md5 => "md5",
        }
    }
}

/// What `checksum` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAction {
    /// Write the sums of the marked files (or the one under the cursor) next to them.
    Create(Algo),
    /// Check the files listed in the checksum file under the cursor (or the one named after
    /// the file under the cursor).
    Verify,
}

/// How one row of a side-by-side comparison relates the two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Same,
    /// A line changed (both sides present).
    Changed,
    OnlyLeft,
    OnlyRight,
}

/// One row: a line number and the text on each side (`None` where the side has no line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: RowKind,
    pub left: Option<(usize, String)>,
    pub right: Option<(usize, String)>,
}

/// What came of a checksum line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SumStatus {
    Ok,
    Mismatch,
    /// The listed file does not exist.
    Missing,
    Unreadable,
    /// The sum was written to the checksum file.
    Created,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumLine {
    pub name: String,
    pub status: SumStatus,
    pub expected: String,
    pub actual: String,
}

/// The finished result shown in the tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Identical {
        size: u64,
    },
    Text {
        rows: Vec<DiffRow>,
        /// Index of the first row of every block of differences.
        starts: Vec<usize>,
    },
    Binary {
        left_size: u64,
        right_size: u64,
        /// Offset of the first differing byte; `None` when one file is a prefix of the other.
        first: Option<u64>,
        left_bytes: Vec<u8>,
        right_bytes: Vec<u8>,
    },
    Sums {
        file: String,
        lines: Vec<SumLine>,
    },
    Failed(String),
}

/// The open tab. The working thread fills `outcome`.
#[derive(Debug)]
pub struct CompareView {
    pub title: String,
    pub left: String,
    pub right: String,
    outcome: Arc<Mutex<Option<Arc<Outcome>>>>,
    cancel: Arc<AtomicBool>,
    version: Arc<AtomicU64>,
    /// Which block of differences is current.
    pub current: usize,
    scroll_to: Option<usize>,
}

impl CompareView {
    fn start<F>(title: String, left: String, right: String, work: F) -> CompareView
    where
        F: FnOnce(&AtomicBool) -> Outcome + Send + 'static,
    {
        let view = CompareView {
            title,
            left,
            right,
            outcome: Arc::default(),
            cancel: Arc::default(),
            version: Arc::default(),
            current: 0,
            scroll_to: None,
        };
        let (outcome, cancel, version) = (
            view.outcome.clone(),
            view.cancel.clone(),
            view.version.clone(),
        );
        std::thread::spawn(move || {
            let result = work(&cancel);
            if !cancel.load(Ordering::Relaxed) {
                if let Ok(mut slot) = outcome.lock() {
                    *slot = Some(Arc::new(result));
                }
                version.fetch_add(1, Ordering::Relaxed);
            }
        });
        view
    }

    /// The result, once the work is done.
    pub fn outcome(&self) -> Option<Arc<Outcome>> {
        self.outcome.lock().ok()?.clone()
    }

    pub fn is_running(&self) -> bool {
        self.outcome().is_none()
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// The row to scroll to, once.
    pub fn take_scroll(&mut self) -> Option<usize> {
        self.scroll_to.take()
    }
}

impl Drop for CompareView {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------- comparing

/// Compares two files; the cancel flag is only checked while reading bytes.
pub fn compare_files(left: &Path, right: &Path, cancel: &AtomicBool) -> Outcome {
    match compare_inner(left, right, cancel) {
        Ok(outcome) => outcome,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

fn compare_inner(left: &Path, right: &Path, cancel: &AtomicBool) -> std::io::Result<Outcome> {
    let (ls, rs) = (
        std::fs::metadata(left)?.len(),
        std::fs::metadata(right)?.len(),
    );
    if ls <= TEXT_LIMIT && rs <= TEXT_LIMIT {
        let (lb, rb) = (std::fs::read(left)?, std::fs::read(right)?);
        if lb == rb {
            return Ok(Outcome::Identical { size: ls });
        }
        if !is_binary(&lb) && !is_binary(&rb) {
            let (lt, rt) = (
                String::from_utf8_lossy(&lb).into_owned(),
                String::from_utf8_lossy(&rb).into_owned(),
            );
            let (a, b): (Vec<&str>, Vec<&str>) = (lt.lines().collect(), rt.lines().collect());
            let rows = diff_rows(&a, &b);
            let starts = block_starts(&rows);
            if starts.is_empty() {
                // Only line endings (or a final newline) differ.
                return Ok(Outcome::Identical { size: ls });
            }
            return Ok(Outcome::Text { rows, starts });
        }
    }
    binary_compare(left, right, ls, rs, cancel)
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

fn binary_compare(
    left: &Path,
    right: &Path,
    left_size: u64,
    right_size: u64,
    cancel: &AtomicBool,
) -> std::io::Result<Outcome> {
    let (mut fl, mut fr) = (std::fs::File::open(left)?, std::fs::File::open(right)?);
    let (mut bl, mut br) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    let mut offset = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Outcome::Failed("przerwano".into()));
        }
        let (nl, nr) = (read_full(&mut fl, &mut bl)?, read_full(&mut fr, &mut br)?);
        let common = nl.min(nr);
        if let Some(i) = (0..common).find(|&i| bl[i] != br[i]) {
            let start = i.saturating_sub(8);
            return Ok(Outcome::Binary {
                left_size,
                right_size,
                first: Some(offset + i as u64),
                left_bytes: bl[start..nl.min(i + 8)].to_vec(),
                right_bytes: br[start..nr.min(i + 8)].to_vec(),
            });
        }
        if nl != nr {
            // One file ends first: the difference starts where the shorter one stops.
            return Ok(Outcome::Binary {
                left_size,
                right_size,
                first: None,
                left_bytes: Vec::new(),
                right_bytes: Vec::new(),
            });
        }
        if nl == 0 {
            return Ok(Outcome::Identical { size: left_size });
        }
        offset += nl as u64;
    }
}

fn read_full(file: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Side-by-side rows of two line lists: common start and end are trimmed, the differing middle
/// is matched by the longest common subsequence (or shown as one changed block when it is too
/// large to match), and a removed run facing an added run becomes `Changed` rows.
pub fn diff_rows(a: &[&str], b: &[&str]) -> Vec<DiffRow> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);

    // Edit script of the middle: (kind, index in a, index in b).
    let mut ops: Vec<(char, usize, usize)> = Vec::new();
    if ma.len() * mb.len() <= MATCH_CELLS {
        let (n, m) = (ma.len(), mb.len());
        let mut table = vec![0u32; (n + 1) * (m + 1)];
        let at = |i: usize, j: usize| i * (m + 1) + j;
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                table[at(i, j)] = if ma[i] == mb[j] {
                    table[at(i + 1, j + 1)] + 1
                } else {
                    table[at(i + 1, j)].max(table[at(i, j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && ma[i] == mb[j] {
                ops.push(('=', i, j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || table[at(i + 1, j)] >= table[at(i, j + 1)]) {
                ops.push(('-', i, j));
                i += 1;
            } else {
                ops.push(('+', i, j));
                j += 1;
            }
        }
    } else {
        ops.extend((0..ma.len()).map(|i| ('-', i, 0)));
        ops.extend((0..mb.len()).map(|j| ('+', 0, j)));
    }

    let mut rows: Vec<DiffRow> = Vec::new();
    for k in 0..prefix {
        rows.push(same(a, k, k));
    }
    let mut idx = 0;
    while idx < ops.len() {
        if ops[idx].0 == '=' {
            rows.push(same(a, prefix + ops[idx].1, prefix + ops[idx].2));
            idx += 1;
            continue;
        }
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        while idx < ops.len() && ops[idx].0 != '=' {
            if ops[idx].0 == '-' {
                removed.push(prefix + ops[idx].1);
            } else {
                added.push(prefix + ops[idx].2);
            }
            idx += 1;
        }
        for k in 0..removed.len().max(added.len()) {
            let left = removed.get(k).map(|&i| (i + 1, a[i].to_string()));
            let right = added.get(k).map(|&j| (j + 1, b[j].to_string()));
            let kind = match (&left, &right) {
                (Some(_), Some(_)) => RowKind::Changed,
                (Some(_), None) => RowKind::OnlyLeft,
                _ => RowKind::OnlyRight,
            };
            rows.push(DiffRow { kind, left, right });
        }
    }
    for k in 0..suffix {
        rows.push(same(a, a.len() - suffix + k, b.len() - suffix + k));
    }
    rows
}

fn same(a: &[&str], i: usize, j: usize) -> DiffRow {
    DiffRow {
        kind: RowKind::Same,
        left: Some((i + 1, a[i].to_string())),
        right: Some((j + 1, a[i].to_string())),
    }
}

/// Index of the first row of every run of rows that are not `Same`.
pub fn block_starts(rows: &[DiffRow]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut inside = false;
    for (i, row) in rows.iter().enumerate() {
        let differs = row.kind != RowKind::Same;
        if differs && !inside {
            starts.push(i);
        }
        inside = differs;
    }
    starts
}

// ---------------------------------------------------------------- checksums

/// The sum of a file as lowercase hex; `Ok(None)` when cancelled.
pub fn digest_of(path: &Path, algo: Algo, cancel: &AtomicBool) -> std::io::Result<Option<String>> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut sha = Sha256::new();
    let mut md5 = Md5::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        match algo {
            Algo::Sha256 => sha.update(&buf[..n]),
            Algo::Md5 => md5.update(&buf[..n]),
        }
    }
    let bytes: Vec<u8> = match algo {
        Algo::Sha256 => sha.finalize().to_vec(),
        Algo::Md5 => md5.finalize().to_vec(),
    };
    Ok(Some(bytes.iter().map(|b| format!("{b:02x}")).collect()))
}

/// The algorithm a checksum file name announces.
fn algo_of(path: &Path) -> Option<Algo> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.ends_with(".sha256") || name.ends_with(".sha256sum") || name == "sha256sums" {
        Some(Algo::Sha256)
    } else if name.ends_with(".md5") || name.ends_with(".md5sum") || name == "md5sums" {
        Some(Algo::Md5)
    } else {
        None
    }
}

/// One entry of a checksum file: the expected sum and the file name (`None`: the line holds
/// only a sum, for the file the checksum file is named after).
fn parse_sum_line(line: &str) -> Option<(String, Option<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    // BSD style: `SHA256 (name) = sum`.
    if let Some((head, sum)) = line.rsplit_once(" = ") {
        if let Some((_, rest)) = head.split_once(" (") {
            if let Some(name) = rest.strip_suffix(')') {
                return hex(sum).map(|s| (s, Some(name.to_string())));
            }
        }
    }
    let (sum, rest) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(s, r)| (s, r));
    let sum = hex(sum)?;
    let name = rest.trim_start().trim_start_matches('*').trim();
    Some((sum, (!name.is_empty()).then(|| name.to_string())))
}

fn hex(text: &str) -> Option<String> {
    let ok = matches!(text.len(), 32 | 64) && text.chars().all(|c| c.is_ascii_hexdigit());
    ok.then(|| text.to_ascii_lowercase())
}

/// Checks every file a checksum file lists; names are relative to the checksum file's folder.
pub fn verify_sums(sum_file: &Path, algo: Algo, cancel: &AtomicBool) -> Outcome {
    let shown = sum_file
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let text = match std::fs::read(sum_file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) => return Outcome::Failed(format!("{shown}: {e}")),
    };
    let dir = sum_file.parent().unwrap_or_else(|| Path::new("."));
    let mut lines = Vec::new();
    for entry in text.lines().filter_map(parse_sum_line) {
        let (expected, name) = entry;
        // A line with only a sum belongs to the file the checksum file is named after.
        let name = name.unwrap_or_else(|| {
            sum_file
                .file_stem()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        });
        let target = dir.join(&name);
        let (status, actual) = if !target.is_file() {
            (SumStatus::Missing, String::new())
        } else {
            match digest_of(&target, algo, cancel) {
                Ok(Some(sum)) if sum == expected => (SumStatus::Ok, sum),
                Ok(Some(sum)) => (SumStatus::Mismatch, sum),
                Ok(None) => return Outcome::Failed("przerwano".into()),
                Err(_) => (SumStatus::Unreadable, String::new()),
            }
        };
        lines.push(SumLine {
            name,
            status,
            expected,
            actual,
        });
    }
    if lines.is_empty() {
        return Outcome::Failed(format!("{shown}: brak wierszy z sumami"));
    }
    Outcome::Sums { file: shown, lines }
}

/// Writes `sum  name` lines for `files` to `target`, which must not exist yet.
pub fn create_sums(files: &[PathBuf], target: &Path, algo: Algo, cancel: &AtomicBool) -> Outcome {
    let shown = target
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mut lines = Vec::new();
    let mut text = String::new();
    for path in files {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match digest_of(path, algo, cancel) {
            Ok(Some(sum)) => {
                text.push_str(&format!("{sum}  {name}\n"));
                lines.push(SumLine {
                    name,
                    status: SumStatus::Created,
                    expected: String::new(),
                    actual: sum,
                });
            }
            Ok(None) => return Outcome::Failed("przerwano".into()),
            Err(e) => return Outcome::Failed(format!("{name}: {e}")),
        }
    }
    // `create_new` refuses to overwrite a file that appeared meanwhile.
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .and_then(|mut f| f.write_all(text.as_bytes()));
    match written {
        Ok(()) => Outcome::Sums { file: shown, lines },
        Err(e) => Outcome::Failed(format!("{shown}: {e}")),
    }
}

// ---------------------------------------------------------------- session

impl Session {
    pub(crate) fn compare_action(&mut self, action: CompareAction) -> Result<(), EngineError> {
        match action {
            CompareAction::Close => self.compare = None,
            CompareAction::Next | CompareAction::Prev => {
                let view = self
                    .compare
                    .as_mut()
                    .ok_or_else(|| EngineError("porównanie nie jest otwarte (compare)".into()))?;
                let Some(outcome) = view.outcome() else {
                    return Ok(());
                };
                let Outcome::Text { starts, .. } = &*outcome else {
                    return Ok(());
                };
                let count = starts.len();
                view.current = if action == CompareAction::Next {
                    (view.current + 1) % count
                } else {
                    (view.current + count - 1) % count
                };
                view.scroll_to = starts.get(view.current).copied();
                self.status = format!("Różnica {} z {count}", view.current + 1);
            }
            CompareAction::Files => {
                let (left, right) = self.compare_pair()?;
                let (ln, rn) = (file_name(&left), file_name(&right));
                self.status = format!("Porównuję {ln} i {rn}…");
                self.compare = Some(CompareView::start(
                    format!("{ln} ↔ {rn}"),
                    left.display().to_string(),
                    right.display().to_string(),
                    move |cancel| compare_files(&left, &right, cancel),
                ));
            }
        }
        Ok(())
    }

    /// The two files to compare: two marked ones, or the one under the cursor and the one under
    /// the cursor of the other panel.
    fn compare_pair(&self) -> Result<(PathBuf, PathBuf), EngineError> {
        let panel = self.active_panel();
        if panel.archive.is_some() || panel.remote.is_some() {
            return Err(EngineError(
                "porównanie działa tylko na plikach na dysku".into(),
            ));
        }
        let marked: Vec<&bluepane_vfs::Entry> = panel
            .target_entries()
            .into_iter()
            .filter(|e| !e.is_dir() && !is_parent_entry(e))
            .collect();
        if marked.len() == 2 && panel.target_entries().len() == 2 {
            return Ok((marked[0].path.clone(), marked[1].path.clone()));
        }
        let other = &self.panels[1 - self.active];
        let usable = |p: &crate::Panel| p.archive.is_none() && p.remote.is_none();
        let here = panel
            .current()
            .filter(|e| !e.is_dir() && !is_parent_entry(e));
        let there = other
            .current()
            .filter(|e| !e.is_dir() && !is_parent_entry(e));
        match (here, there) {
            (Some(a), Some(b)) if usable(other) && panel.target_entries().len() <= 1 => {
                Ok((a.path.clone(), b.path.clone()))
            }
            _ => Err(EngineError(
                "zaznacz dwa pliki albo ustaw kursor na pliku w każdym panelu".into(),
            )),
        }
    }

    pub(crate) fn checksum_action(&mut self, action: ChecksumAction) -> Result<(), EngineError> {
        let panel = self.active_panel();
        if panel.archive.is_some() || panel.remote.is_some() {
            return Err(EngineError(
                "sumy kontrolne działają tylko na plikach na dysku".into(),
            ));
        }
        let files: Vec<PathBuf> = panel
            .target_entries()
            .into_iter()
            .filter(|e| !e.is_dir() && !is_parent_entry(e))
            .map(|e| e.path.clone())
            .collect();
        let Some(first) = files.first().cloned() else {
            return Err(EngineError("wskaż plik albo zaznacz pliki".into()));
        };
        match action {
            ChecksumAction::Verify => {
                // The file under the cursor is a checksum file, or has one beside it.
                let (sum_file, algo) = match algo_of(&first) {
                    Some(algo) => (first.clone(), algo),
                    None => [Algo::Sha256, Algo::Md5]
                        .into_iter()
                        .find_map(|algo| {
                            let mut name = first.clone().into_os_string();
                            name.push(format!(".{}", algo.extension()));
                            let candidate = PathBuf::from(name);
                            candidate.is_file().then_some((candidate, algo))
                        })
                        .ok_or_else(|| {
                            EngineError(
                                "to nie jest plik .sha256 ani .md5 i nie ma takiego obok".into(),
                            )
                        })?,
                };
                let shown = file_name(&sum_file);
                self.status = format!("Sprawdzam sumy z {shown}…");
                self.compare = Some(CompareView::start(
                    format!("Sumy: {shown}"),
                    sum_file.display().to_string(),
                    String::new(),
                    move |cancel| verify_sums(&sum_file, algo, cancel),
                ));
            }
            ChecksumAction::Create(algo) => {
                let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
                let target = if files.len() == 1 {
                    let mut name = first.clone().into_os_string();
                    name.push(format!(".{}", algo.extension()));
                    PathBuf::from(name)
                } else {
                    dir.join(match algo {
                        Algo::Sha256 => "SHA256SUMS",
                        Algo::Md5 => "MD5SUMS",
                    })
                };
                if target.exists() {
                    return Err(EngineError(format!(
                        "{} już istnieje; usuń go albo zmień nazwę",
                        file_name(&target)
                    )));
                }
                let shown = file_name(&target);
                self.status = format!("Liczę sumy do {shown}…");
                self.compare = Some(CompareView::start(
                    format!("Sumy: {shown}"),
                    target.display().to_string(),
                    String::new(),
                    move |cancel| create_sums(&files, &target, algo, cancel),
                ));
            }
        }
        Ok(())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// `compare [close|next|prev]`
pub(crate) fn parse_compare(rest: &str) -> Result<CompareAction, EngineError> {
    match rest.trim().to_lowercase().as_str() {
        "" => Ok(CompareAction::Files),
        "close" => Ok(CompareAction::Close),
        "next" => Ok(CompareAction::Next),
        "prev" => Ok(CompareAction::Prev),
        _ => Err(EngineError(
            "compare: użycie: compare [close|next|prev]".into(),
        )),
    }
}

/// `checksum create sha256|md5` or `checksum verify`
pub(crate) fn parse_checksum(rest: &str) -> Result<ChecksumAction, EngineError> {
    let usage = "checksum: użycie: checksum create sha256|md5 albo checksum verify";
    let words: Vec<String> = rest.split_whitespace().map(str::to_lowercase).collect();
    match words.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["verify"] => Ok(ChecksumAction::Verify),
        ["create", "sha256"] => Ok(ChecksumAction::Create(Algo::Sha256)),
        ["create", "md5"] => Ok(ChecksumAction::Create(Algo::Md5)),
        _ => Err(EngineError(usage.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn lines(text: &str) -> Vec<&str> {
        text.lines().collect()
    }

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        let s = Session::new(&left, &right);
        (tmp, s)
    }

    fn wait(s: &Session) -> Arc<Outcome> {
        let start = Instant::now();
        loop {
            if let Some(o) = s.compare.as_ref().and_then(CompareView::outcome) {
                return o;
            }
            assert!(start.elapsed() < Duration::from_secs(20), "no result");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn lines_are_matched_and_changes_pair_up() {
        let a = lines("jeden\ndwa\ntrzy\nczter\npięć");
        let b = lines("jeden\nDWA\ntrzy\npięć\nsześć");
        let rows = diff_rows(&a, &b);
        let kinds: Vec<RowKind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [
                RowKind::Same,
                RowKind::Changed,
                RowKind::Same,
                RowKind::OnlyLeft,
                RowKind::Same,
                RowKind::OnlyRight
            ]
        );
        assert_eq!(rows[1].left, Some((2, "dwa".into())));
        assert_eq!(rows[1].right, Some((2, "DWA".into())));
        // Line numbers of the right side follow the right file.
        assert_eq!(rows[4].left, Some((5, "pięć".into())));
        assert_eq!(rows[4].right, Some((4, "pięć".into())));
        assert_eq!(block_starts(&rows), [1, 3, 5]);
    }

    #[test]
    fn identical_and_empty_inputs() {
        assert!(block_starts(&diff_rows(&lines("a\nb"), &lines("a\nb"))).is_empty());
        let rows = diff_rows(&[], &lines("x"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RowKind::OnlyRight);
        assert!(diff_rows(&[], &[]).is_empty());
    }

    #[test]
    fn two_text_files_open_a_comparison_and_next_walks_the_differences() {
        let (tmp, mut s) = session();
        let (l, r) = (tmp.path().join("left"), tmp.path().join("right"));
        fs::write(l.join("zażółć.txt"), "a\nb\nc\nd\ne\n").unwrap();
        fs::write(r.join("zażółć.txt"), "a\nB\nc\nd\nE\n").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select zażółć.txt").unwrap();
        s.run_line("switch").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select zażółć.txt").unwrap();
        s.run_line("switch").unwrap();
        s.run_line("compare").unwrap();
        let outcome = wait(&s);
        let Outcome::Text { rows, starts } = &*outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!((rows.len(), starts.clone()), (5, vec![1, 4]));
        s.run_line("compare next").unwrap();
        assert_eq!(s.compare.as_mut().unwrap().take_scroll(), Some(4));
        s.run_line("compare next").unwrap();
        assert_eq!(s.compare.as_mut().unwrap().take_scroll(), Some(1));
        s.run_line("compare prev").unwrap();
        assert_eq!(s.compare.as_mut().unwrap().take_scroll(), Some(4));
        s.run_line("compare close").unwrap();
        assert!(s.compare.is_none());
    }

    #[test]
    fn equal_files_are_identical_and_binary_files_report_the_first_difference() {
        let tmp = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let mut a = vec![1u8; 200_000];
        a[10] = 0;
        let mut b = a.clone();
        let (pa, pb) = (tmp.path().join("a.bin"), tmp.path().join("b.bin"));
        fs::write(&pa, &a).unwrap();
        fs::write(&pb, &b).unwrap();
        assert_eq!(
            compare_files(&pa, &pb, &cancel),
            Outcome::Identical { size: 200_000 }
        );
        b[150_000] = 9;
        fs::write(&pb, &b).unwrap();
        let Outcome::Binary {
            first,
            left_bytes,
            right_bytes,
            ..
        } = compare_files(&pa, &pb, &cancel)
        else {
            panic!("binary expected");
        };
        assert_eq!(first, Some(150_000));
        assert_eq!((left_bytes.len(), right_bytes[8]), (16, 9));
        // One file is a prefix of the other.
        fs::write(&pb, &a[..1000]).unwrap();
        assert!(matches!(
            compare_files(&pa, &pb, &cancel),
            Outcome::Binary { first: None, .. }
        ));
        assert!(matches!(
            compare_files(&pa, &tmp.path().join("missing"), &cancel),
            Outcome::Failed(_)
        ));
    }

    #[test]
    fn checksums_are_created_verified_and_a_changed_file_is_caught() {
        let (tmp, mut s) = session();
        let l = tmp.path().join("left");
        fs::write(l.join("zażółć.txt"), "abc").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select zażółć.txt").unwrap();
        s.run_line("checksum create sha256").unwrap();
        let outcome = wait(&s);
        let Outcome::Sums { lines, .. } = &*outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            lines[0].actual,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let written = fs::read_to_string(l.join("zażółć.txt.sha256")).unwrap();
        assert_eq!(
            written,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  zażółć.txt\n"
        );
        // It never overwrites.
        assert!(s.run_line("checksum create sha256").is_err());

        s.run_line("checksum verify").unwrap();
        let outcome = wait(&s);
        let Outcome::Sums { lines, .. } = &*outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(lines[0].status, SumStatus::Ok);

        fs::write(l.join("zażółć.txt"), "abd").unwrap();
        s.run_line("checksum verify").unwrap();
        let outcome = wait(&s);
        let Outcome::Sums { lines, .. } = &*outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(lines[0].status, SumStatus::Mismatch);
    }

    #[test]
    fn md5_files_and_the_bsd_and_sum_only_formats_are_read() {
        let tmp = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        fs::write(tmp.path().join("a.txt"), "abc").unwrap();
        let md5 = "900150983cd24fb0d6963f7d28e17f72";
        // Sum only: the file is the one the checksum file is named after.
        fs::write(tmp.path().join("a.txt.md5"), format!("{md5}\n")).unwrap();
        let Outcome::Sums { lines, .. } =
            verify_sums(&tmp.path().join("a.txt.md5"), Algo::Md5, &cancel)
        else {
            panic!();
        };
        assert_eq!(
            (lines[0].name.as_str(), lines[0].status),
            ("a.txt", SumStatus::Ok)
        );
        // BSD style, a binary-mode star, a missing file and a comment.
        fs::write(
            tmp.path().join("MD5SUMS"),
            format!("# komentarz\nMD5 (a.txt) = {md5}\n{md5} *a.txt\n{md5}  nie-ma.txt\n"),
        )
        .unwrap();
        let Outcome::Sums { lines, .. } =
            verify_sums(&tmp.path().join("MD5SUMS"), Algo::Md5, &cancel)
        else {
            panic!();
        };
        let statuses: Vec<SumStatus> = lines.iter().map(|l| l.status).collect();
        assert_eq!(statuses, [SumStatus::Ok, SumStatus::Ok, SumStatus::Missing]);
        fs::write(tmp.path().join("pusty.md5"), "to nie suma\n").unwrap();
        assert!(matches!(
            verify_sums(&tmp.path().join("pusty.md5"), Algo::Md5, &cancel),
            Outcome::Failed(_)
        ));
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_compare("").unwrap(), CompareAction::Files);
        assert_eq!(parse_compare("NEXT").unwrap(), CompareAction::Next);
        assert!(parse_compare("x").is_err());
        assert_eq!(
            parse_checksum("create MD5").unwrap(),
            ChecksumAction::Create(Algo::Md5)
        );
        assert_eq!(parse_checksum("verify").unwrap(), ChecksumAction::Verify);
        assert!(parse_checksum("create sha1").is_err());
    }
}
