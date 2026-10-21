//! A search index: the names of the files under some folders and the words in their content,
//! kept in memory (and in one file) so a search answers at once instead of reading the disk.
//! Nothing is indexed unless the user adds a folder; building walks the folders once, can be
//! cancelled, and an update re-reads only the files whose size or date changed.
//!
//! Words are lower-cased and have the Polish (and other Latin) accents taken off, so `zolw`
//! finds `żółw`; a word in a query also matches longer words that start with it (`dokum`
//! finds `dokument`). File names are matched as folded substrings. Several terms all have
//! to match. `name:x` looks only in names, `text:x` only in content, `ext:pdf` at the
//! extension.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The first line of the index file; the number is the format version.
const HEADER: &str = "BLUEPANE-INDEX 1";

/// Words shorter or longer than this are not indexed.
const WORD_MIN: usize = 2;
const WORD_MAX: usize = 40;

/// A file is not read for words past this many bytes (text) ...
const TEXT_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// ... and a file keeps at most this many different words.
const WORDS_PER_FILE: usize = 20_000;

/// Folders that are not entered.
const SKIPPED_DIRS: &[&str] = &[".git", "node_modules"];

/// Lower case with accents taken off: `Żółć` becomes `zolc`.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'ą' | 'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => out.push('a'),
            'ć' | 'ç' | 'č' => out.push('c'),
            'ę' | 'é' | 'è' | 'ë' | 'ê' | 'ě' => out.push('e'),
            'í' | 'ì' | 'ï' | 'î' => out.push('i'),
            'ł' => out.push('l'),
            'ń' | 'ñ' | 'ň' => out.push('n'),
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => out.push('o'),
            'ś' | 'š' => out.push('s'),
            'ú' | 'ù' | 'ü' | 'û' | 'ů' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ź' | 'ż' | 'ž' => out.push('z'),
            'ß' => out.push_str("ss"),
            other => out.push(other),
        }
    }
    out
}

/// The different words of a text, folded.
pub fn words_of(text: &str) -> HashSet<String> {
    let mut words = HashSet::new();
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let count = word.chars().count();
        if (WORD_MIN..=WORD_MAX).contains(&count) {
            words.insert(fold(word));
            if words.len() >= WORDS_PER_FILE {
                break;
            }
        }
    }
    words
}

/// One file in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: u64,
    /// The folded name.
    name: String,
    /// The folded extension (without the dot).
    ext: String,
}

impl IndexedFile {
    fn new(path: PathBuf, size: u64, modified: u64) -> IndexedFile {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| fold(&e.to_string_lossy()))
            .unwrap_or_default();
        IndexedFile {
            name: fold(&file_name),
            ext,
            path,
            size,
            modified,
        }
    }
}

/// Where a term of a query looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The name or the content.
    Any,
    Name,
    Text,
    Ext,
}

/// A parsed query: every term has to match.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IndexQuery {
    pub terms: Vec<(Field, String)>,
}

impl IndexQuery {
    /// `umowa name:2025 ext:pdf`. Words are folded; `name:`, `text:` and `ext:` pick where.
    pub fn parse(text: &str) -> IndexQuery {
        let mut terms = Vec::new();
        for word in text.split_whitespace() {
            let (field, rest) = match word.split_once(':') {
                Some((f, rest)) if f.eq_ignore_ascii_case("name") => (Field::Name, rest),
                Some((f, rest)) if f.eq_ignore_ascii_case("text") => (Field::Text, rest),
                Some((f, rest)) if f.eq_ignore_ascii_case("ext") => (Field::Ext, rest),
                _ => (Field::Any, word),
            };
            let rest = fold(rest.trim_start_matches('.'));
            if !rest.is_empty() {
                terms.push((field, rest));
            }
        }
        IndexQuery { terms }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// What a build did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildSummary {
    pub files: u64,
    /// Files whose words were read (the others were unchanged or have none).
    pub read: u64,
    pub folders: u64,
    /// Folders and files that could not be read.
    pub unreadable: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Index {
    roots: Vec<PathBuf>,
    files: Vec<IndexedFile>,
    /// word → ids of the files holding it (ascending).
    words: BTreeMap<String, Vec<u32>>,
    /// When the index was built, seconds since the Unix epoch.
    pub built_at: u64,
}

fn seconds(time: std::time::SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Reads the text of a file that looks like text: up to 8 MB; `None` for a binary file or one
/// that cannot be read. UTF-8 is read as such, anything else as Windows-1250.
fn read_text(path: &Path, size: u64) -> Option<String> {
    if size == 0 {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(TEXT_MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    // A NUL byte in the first block means a binary file (UTF-16 text is not indexed).
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return None;
    }
    Some(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => bluepane_text::decode_cp1250(e.as_bytes()),
    })
}

impl Index {
    pub fn new(roots: Vec<PathBuf>) -> Index {
        Index {
            roots,
            ..Index::default()
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn files(&self) -> &[IndexedFile] {
        &self.files
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    /// Adds a folder to the roots; false when it (or a folder above it) already is one.
    pub fn add_root(&mut self, root: PathBuf) -> bool {
        if self.roots.iter().any(|r| root.starts_with(r)) {
            return false;
        }
        // Folders below the new root are covered by it from now on.
        self.roots.retain(|r| !r.starts_with(&root));
        self.roots.push(root);
        self.roots.sort();
        true
    }

    /// The index without the files under `root` (which stops being a root); false when it
    /// was not a root.
    pub fn remove_root(&mut self, root: &Path) -> bool {
        if !self.roots.iter().any(|r| r == root) {
            return false;
        }
        self.roots.retain(|r| r != root);
        let keep: Vec<bool> = self
            .files
            .iter()
            .map(|f| !f.path.starts_with(root))
            .collect();
        let mut remap: Vec<Option<u32>> = Vec::with_capacity(keep.len());
        let mut next = 0u32;
        for &k in &keep {
            remap.push(k.then(|| {
                next += 1;
                next - 1
            }));
        }
        let mut files = Vec::with_capacity(next as usize);
        for (file, &k) in std::mem::take(&mut self.files).into_iter().zip(&keep) {
            if k {
                files.push(file);
            }
        }
        self.files = files;
        self.words.retain(|_, ids| {
            *ids = ids.iter().filter_map(|&id| remap[id as usize]).collect();
            !ids.is_empty()
        });
        true
    }

    /// The words of the files that are unchanged, per file id of this index.
    fn words_by_file(&self) -> HashMap<u32, Vec<&str>> {
        let mut by_file: HashMap<u32, Vec<&str>> = HashMap::new();
        for (word, ids) in &self.words {
            for &id in ids {
                by_file.entry(id).or_default().push(word);
            }
        }
        by_file
    }

    /// Walks the roots and returns a new index. Files whose size and date match `previous` keep
    /// the words they had; the others are read: `documents` gives the text of a document
    /// (PDF, Word, Excel...) or `None` when the file is no document, and any other file that
    /// looks like text is read directly. `cancelled` is asked between files; `progress` gets the
    /// number of files seen so far.
    pub fn build(
        roots: &[PathBuf],
        previous: Option<&Index>,
        documents: &dyn Fn(&Path) -> Option<String>,
        cancelled: &dyn Fn() -> bool,
        progress: &dyn Fn(u64),
    ) -> (Index, BuildSummary) {
        let mut index = Index::new(roots.to_vec());
        let mut summary = BuildSummary::default();
        let old_by_path: HashMap<&Path, u32> = previous
            .map(|p| {
                p.files
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.path.as_path(), i as u32))
                    .collect()
            })
            .unwrap_or_default();
        let old_words = previous.map(Index::words_by_file).unwrap_or_default();
        let mut postings: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut stack: Vec<PathBuf> = roots.iter().rev().cloned().collect();
        'walk: while let Some(dir) = stack.pop() {
            if cancelled() {
                summary.cancelled = true;
                break;
            }
            let Ok(entries) = bluepane_vfs::list_dir(&dir) else {
                summary.unreadable += 1;
                continue;
            };
            summary.folders += 1;
            let mut subfolders = Vec::new();
            for entry in entries {
                if entry.hidden || entry.path.to_string_lossy().contains(['\t', '\n']) {
                    continue;
                }
                match entry.kind {
                    bluepane_vfs::EntryKind::Dir => {
                        // Links are not followed (no loops, no leaving the roots).
                        let is_link = std::fs::symlink_metadata(&entry.path)
                            .is_ok_and(|m| m.file_type().is_symlink());
                        if !is_link && !SKIPPED_DIRS.iter().any(|s| entry.name == *s) {
                            subfolders.push(entry.path);
                        }
                    }
                    bluepane_vfs::EntryKind::File => {
                        if cancelled() {
                            summary.cancelled = true;
                            break 'walk;
                        }
                        let modified = entry.modified.map_or(0, seconds);
                        let id = index.files.len() as u32;
                        let unchanged = old_by_path.get(entry.path.as_path()).filter(|&&old| {
                            previous.is_some_and(|p| {
                                let f = &p.files[old as usize];
                                f.size == entry.size && f.modified == modified
                            })
                        });
                        let words: Vec<String> = match unchanged {
                            Some(old) => old_words
                                .get(old)
                                .map(|w| w.iter().map(|s| (*s).to_string()).collect())
                                .unwrap_or_default(),
                            None => {
                                summary.read += 1;
                                let text = documents(&entry.path)
                                    .or_else(|| read_text(&entry.path, entry.size));
                                text.map(|t| words_of(&t).into_iter().collect())
                                    .unwrap_or_default()
                            }
                        };
                        for word in words {
                            postings.entry(word).or_default().push(id);
                        }
                        index
                            .files
                            .push(IndexedFile::new(entry.path, entry.size, modified));
                        summary.files += 1;
                        if summary.files % 200 == 0 {
                            progress(summary.files);
                        }
                    }
                    _ => {}
                }
            }
            // Visited in name order, files of a folder before the folders inside it.
            stack.extend(subfolders.into_iter().rev());
        }
        progress(summary.files);
        index.words = postings;
        index.built_at = seconds(std::time::SystemTime::now());
        (index, summary)
    }

    /// The files matching every term of the query, by path.
    pub fn search(&self, query: &IndexQuery) -> Vec<&IndexedFile> {
        if query.is_empty() {
            return Vec::new();
        }
        let mut alive: Vec<bool> = vec![true; self.files.len()];
        for (field, term) in &query.terms {
            let mut hit = vec![false; self.files.len()];
            if matches!(field, Field::Any | Field::Name) {
                for (i, file) in self.files.iter().enumerate() {
                    if file.name.contains(term.as_str()) {
                        hit[i] = true;
                    }
                }
            }
            if *field == Field::Ext {
                for (i, file) in self.files.iter().enumerate() {
                    if file.ext == *term {
                        hit[i] = true;
                    }
                }
            }
            // A single letter would match half the words: names only.
            if matches!(field, Field::Any | Field::Text) && term.chars().count() >= WORD_MIN {
                for (word, ids) in self.words.range(term.clone()..) {
                    if !word.starts_with(term.as_str()) {
                        break;
                    }
                    for &id in ids {
                        hit[id as usize] = true;
                    }
                }
            }
            for (a, h) in alive.iter_mut().zip(&hit) {
                *a &= *h;
            }
        }
        let mut found: Vec<&IndexedFile> = self
            .files
            .iter()
            .zip(&alive)
            .filter_map(|(f, &a)| a.then_some(f))
            .collect();
        found.sort_by(|a, b| a.path.cmp(&b.path));
        found
    }

    /// Writes the index to a file (through a temporary one, so a crash keeps the old index).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        out.push_str(&format!("built {}\n", self.built_at));
        for root in &self.roots {
            out.push_str(&format!("root {}\n", root.display()));
        }
        out.push_str(&format!("files {}\n", self.files.len()));
        for f in &self.files {
            out.push_str(&format!(
                "{}\t{}\t{}\n",
                f.size,
                f.modified,
                f.path.display()
            ));
        }
        out.push_str(&format!("words {}\n", self.words.len()));
        for (word, ids) in &self.words {
            out.push_str(word);
            out.push('\t');
            let mut last = 0u32;
            for (i, &id) in ids.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&(id - last).to_string());
                last = id;
            }
            out.push('\n');
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        std::fs::write(&temporary, out)?;
        std::fs::rename(&temporary, path)
    }

    /// Reads an index file; `Ok(None)` when there is none. A damaged or newer file is an error.
    pub fn load(path: &Path) -> std::io::Result<Option<Index>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let bad = |what: &str| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("plik indeksu jest uszkodzony albo z innej wersji ({what})"),
            )
        };
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err(bad("nagłówek"));
        }
        let mut index = Index::default();
        let mut files_left = None;
        let mut words_left = None;
        for line in lines {
            if let Some(left) = files_left.as_mut().filter(|n: &&mut usize| **n > 0) {
                *left -= 1;
                let mut fields = line.splitn(3, '\t');
                let (size, modified, path) = (fields.next(), fields.next(), fields.next());
                let (Some(Ok(size)), Some(Ok(modified)), Some(path)) = (
                    size.map(str::parse::<u64>),
                    modified.map(str::parse::<u64>),
                    path,
                ) else {
                    return Err(bad("plik"));
                };
                index
                    .files
                    .push(IndexedFile::new(PathBuf::from(path), size, modified));
            } else if let Some(left) = words_left.as_mut().filter(|n: &&mut usize| **n > 0) {
                *left -= 1;
                let (word, ids) = line.split_once('\t').ok_or_else(|| bad("słowo"))?;
                let mut last = 0u32;
                let mut list = Vec::new();
                for delta in ids.split(',') {
                    last += delta.parse::<u32>().map_err(|_| bad("numer"))?;
                    if last as usize >= index.files.len() {
                        return Err(bad("numer pliku"));
                    }
                    list.push(last);
                }
                index.words.insert(word.to_string(), list);
            } else if let Some(n) = line.strip_prefix("built ") {
                index.built_at = n.parse().map_err(|_| bad("data"))?;
            } else if let Some(root) = line.strip_prefix("root ") {
                index.roots.push(PathBuf::from(root));
            } else if let Some(n) = line.strip_prefix("files ") {
                files_left = Some(n.parse().map_err(|_| bad("liczba plików"))?);
            } else if let Some(n) = line.strip_prefix("words ") {
                words_left = Some(n.parse().map_err(|_| bad("liczba słów"))?);
            } else if !line.is_empty() {
                return Err(bad("nieznany wiersz"));
            }
        }
        if files_left != Some(0) || words_left != Some(0) {
            return Err(bad("urwany plik"));
        }
        Ok(Some(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_documents(_: &Path) -> Option<String> {
        None
    }

    fn build(roots: &[PathBuf], previous: Option<&Index>) -> (Index, BuildSummary) {
        Index::build(roots, previous, &no_documents, &|| false, &|_| {})
    }

    fn names(found: Vec<&IndexedFile>) -> Vec<String> {
        found
            .into_iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn query(text: &str) -> IndexQuery {
        IndexQuery::parse(text)
    }

    #[test]
    fn folding_takes_polish_and_other_accents_off() {
        assert_eq!(fold("Zażółć gęślą jaźń"), "zazolc gesla jazn");
        assert_eq!(fold("Straße Café Ñandú"), "strasse cafe nandu");
        let words = words_of("Żółw, żółw! a ab Zażółć 2025-r");
        let mut sorted: Vec<_> = words.into_iter().collect();
        sorted.sort();
        assert_eq!(sorted, ["2025", "ab", "zazolc", "zolw"].map(String::from));
    }

    #[test]
    fn a_query_is_parsed_into_folded_terms_with_fields() {
        assert_eq!(
            query("Umowa name:Żółw ext:.PDF text:x").terms,
            [
                (Field::Any, "umowa".to_string()),
                (Field::Name, "zolw".to_string()),
                (Field::Ext, "pdf".to_string()),
                (Field::Text, "x".to_string()),
            ]
        );
        assert!(query("  name: ").is_empty());
    }

    #[test]
    fn names_and_content_are_found_without_accents_and_by_word_start() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("Dokumenty/głęboko")).unwrap();
        std::fs::write(
            dir.join("Dokumenty/umowa.txt"),
            "Umowa o dzieło: żółw i gęś",
        )
        .unwrap();
        std::fs::write(dir.join("Dokumenty/głęboko/Zażółć.txt"), "nic ciekawego").unwrap();
        std::fs::write(dir.join("notatki.md"), "zakupy: mleko").unwrap();
        // Windows-1250 text and a binary file.
        std::fs::write(dir.join("stary.txt"), b"\xa3\xf3d\x9f na \xea\xf3\xe6").unwrap();
        std::fs::write(dir.join("blob.bin"), [0u8, 1, 2, b'z', b'o', b'l', b'w']).unwrap();
        let (index, summary) = build(&[dir.to_path_buf()], None);
        assert_eq!((summary.files, summary.unreadable), (5, 0));
        assert!(index.word_count() > 5);

        let find = |q: &str| names(index.search(&query(q)));
        assert_eq!(find("zolw"), ["umowa.txt"], "content, accent-less");
        assert_eq!(find("żółw"), ["umowa.txt"], "content, with accents");
        assert_eq!(find("dzie"), ["umowa.txt"], "the start of a word");
        assert_eq!(find("zazolc"), ["Zażółć.txt"], "a name");
        assert_eq!(find("lodz"), ["stary.txt"], "Windows-1250 content (Łódź)");
        assert_eq!(find("umowa zolw"), ["umowa.txt"], "all terms");
        assert!(find("umowa zakupy").is_empty());
        assert!(find("name:zolw").is_empty(), "name only: no such name");
        assert_eq!(find("name:umowa"), ["umowa.txt"]);
        assert_eq!(find("text:mleko ext:md"), ["notatki.md"]);
        assert!(find("text:notatki").is_empty(), "content only");
        assert!(
            find("blob zolw").is_empty(),
            "binary content is not indexed"
        );
        assert_eq!(find("blob"), ["blob.bin"], "but its name is");
        assert!(find("").is_empty());
        assert!(find("text:z").is_empty(), "one letter is not a word");
    }

    #[test]
    fn hidden_files_links_and_skipped_folders_stay_out() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::write(dir.join(".git/config"), "tajne").unwrap();
        std::fs::write(dir.join("node_modules/x.js"), "tajne").unwrap();
        std::fs::write(dir.join(".ukryty"), "tajne").unwrap();
        std::fs::write(dir.join("widoczny.txt"), "tajne").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir, dir.join("petla")).unwrap();
        let (index, _) = build(&[dir.to_path_buf()], None);
        assert_eq!(names(index.search(&query("tajne"))), ["widoczny.txt"]);
    }

    #[test]
    fn an_update_reads_only_what_changed_and_forgets_what_is_gone() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("a.txt"), "alfa").unwrap();
        std::fs::write(dir.join("b.txt"), "beta").unwrap();
        std::fs::write(dir.join("c.txt"), "gamma").unwrap();
        let (first, summary) = build(&[dir.to_path_buf()], None);
        assert_eq!((summary.files, summary.read), (3, 3));

        std::fs::write(dir.join("b.txt"), "delta wiecej").unwrap();
        std::fs::remove_file(dir.join("c.txt")).unwrap();
        std::fs::write(dir.join("d.txt"), "epsilon").unwrap();
        let (second, summary) = build(&[dir.to_path_buf()], Some(&first));
        assert_eq!((summary.files, summary.read), (3, 2), "b changed, d is new");
        let find = |q: &str| names(second.search(&query(q)));
        assert_eq!(find("alfa"), ["a.txt"], "unchanged: words carried over");
        assert!(find("beta").is_empty());
        assert_eq!(find("delta"), ["b.txt"]);
        assert!(find("gamma").is_empty());
        assert_eq!(find("epsilon"), ["d.txt"]);
    }

    #[test]
    fn documents_come_from_the_callback_and_a_build_can_be_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("raport.pdf"), b"%PDF-binary").unwrap();
        std::fs::write(dir.join("zwykly.txt"), "zwykly tekst").unwrap();
        let documents = |p: &Path| {
            (p.extension().is_some_and(|e| e == "pdf")).then(|| "Kwartalny raport sprzedaży".into())
        };
        let (index, _) = Index::build(&[dir.to_path_buf()], None, &documents, &|| false, &|_| {});
        assert_eq!(names(index.search(&query("sprzedazy"))), ["raport.pdf"]);

        let (partial, summary) =
            Index::build(&[dir.to_path_buf()], None, &documents, &|| true, &|_| {});
        assert!(summary.cancelled && partial.file_count() == 0);
    }

    #[test]
    fn an_index_survives_a_round_trip_and_damage_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dane");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zażółć gęślą.txt"), "jaźń żółw").unwrap();
        std::fs::write(dir.join("b.txt"), "jaźń").unwrap();
        let (index, _) = build(std::slice::from_ref(&dir), None);
        let file = tmp.path().join("config/index.dat");
        index.save(&file).unwrap();
        let loaded = Index::load(&file).unwrap().unwrap();
        assert_eq!(loaded.roots(), index.roots());
        assert_eq!(loaded.file_count(), 2);
        assert_eq!(loaded.word_count(), index.word_count());
        assert_eq!(
            names(loaded.search(&query("jazn"))),
            ["b.txt", "zażółć gęślą.txt"]
        );
        assert_eq!(
            names(loaded.search(&query("name:gesla"))),
            ["zażółć gęślą.txt"]
        );

        assert!(Index::load(&tmp.path().join("none.dat")).unwrap().is_none());
        std::fs::write(&file, "BLUEPANE-INDEX 9\n").unwrap();
        assert!(Index::load(&file).is_err());
        std::fs::write(&file, "BLUEPANE-INDEX 1\nfiles 2\n1\t1\t/a\n").unwrap();
        assert!(Index::load(&file).is_err(), "cut short");
    }

    #[test]
    fn roots_can_be_added_once_and_removed_with_their_files() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        std::fs::create_dir_all(a.join("pod")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("x.txt"), "wspolne a").unwrap();
        std::fs::write(b.join("y.txt"), "wspolne b").unwrap();
        let (mut index, _) = build(&[a.clone(), b.clone()], None);
        assert!(!index.add_root(a.join("pod")), "inside a root already");
        assert!(!index.add_root(a.clone()));
        assert_eq!(names(index.search(&query("wspolne"))), ["x.txt", "y.txt"]);
        assert!(index.remove_root(&a));
        assert!(!index.remove_root(&a));
        assert_eq!(index.roots(), [b]);
        assert_eq!(names(index.search(&query("wspolne"))), ["y.txt"]);
        assert_eq!(index.file_count(), 1);
    }
}
