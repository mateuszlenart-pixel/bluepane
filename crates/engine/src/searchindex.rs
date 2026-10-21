//! The search index (`index`): names and content of the files under the folders the user
//! adds, searched at once instead of walking the disk. Nothing is indexed unless a folder is
//! added with `index add`; building runs in the background, can be stopped (`index stop`) and is
//! never started by itself. The index lives in `index.dat` in the configuration folder;
//! `index update` reads again only the files whose size or date changed.
//!
//! Results appear as a list of files in the active panel, like the results of `find`. A search
//! can be saved under a name (`index save`) and opened again later (`index open`): it is asked
//! of the index anew each time, so the "folder" always shows the current files.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bluepane_search::{Index, IndexQuery};

use crate::session::Session;
use crate::EngineError;

/// Documents larger than this are not opened for their text.
const DOCUMENT_MAX: u64 = 64 * 1024 * 1024;

/// What `index` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexAction {
    Status,
    /// Index this folder (`here` is the folder of the active panel).
    Add(String),
    Remove(String),
    Update,
    Stop,
    Clear,
    Find(String),
    /// Save a search under a name; without a query the last one is saved.
    Save(String, Option<String>),
    Open(String),
    List,
    Forget(String),
}

#[derive(Default)]
struct Shared {
    index: Mutex<Option<Arc<Index>>>,
    /// The index file was looked at.
    loaded: AtomicBool,
    building: AtomicBool,
    cancel: AtomicBool,
    files_seen: AtomicU64,
    /// Raised when a build ends, with its message in `message`.
    version: AtomicU64,
    message: Mutex<String>,
}

/// The index and what happens to it in the background. Cheap to clone (shared).
#[derive(Clone, Default)]
pub struct SearchIndex {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for SearchIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchIndex")
            .field("building", &self.is_building())
            .finish()
    }
}

impl SearchIndex {
    pub fn is_building(&self) -> bool {
        self.shared.building.load(Ordering::Relaxed)
    }

    /// How many files the running build has seen so far.
    pub fn files_seen(&self) -> u64 {
        self.shared.files_seen.load(Ordering::Relaxed)
    }

    fn current(&self) -> Option<Arc<Index>> {
        self.shared.index.lock().ok()?.clone()
    }

    fn set(&self, index: Option<Arc<Index>>) {
        if let Ok(mut slot) = self.shared.index.lock() {
            *slot = index;
        }
        self.shared.loaded.store(true, Ordering::Relaxed);
    }

    /// Reads the index file once.
    fn ensure_loaded(&self, file: Option<&Path>) -> Result<(), EngineError> {
        if self.shared.loaded.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.shared.loaded.store(true, Ordering::Relaxed);
        if let Some(file) = file {
            match Index::load(file) {
                Ok(index) => self.set(index.map(Arc::new)),
                Err(e) => {
                    return Err(EngineError(format!(
                        "nie udało się wczytać indeksu ({}): {e}; `index clear` go usunie",
                        file.display()
                    )))
                }
            }
        }
        Ok(())
    }

    /// Starts building over `roots` in the background; the result replaces the index when done
    /// and is saved to `file`.
    fn start(&self, roots: Vec<PathBuf>, previous: Option<Arc<Index>>, file: Option<PathBuf>) {
        let shared = Arc::clone(&self.shared);
        shared.building.store(true, Ordering::Relaxed);
        shared.cancel.store(false, Ordering::Relaxed);
        shared.files_seen.store(0, Ordering::Relaxed);
        std::thread::spawn(move || {
            let documents = |path: &Path| {
                bluepane_docs::kind_of(path)?;
                let size = std::fs::metadata(path).ok()?.len();
                if size > DOCUMENT_MAX {
                    return None;
                }
                bluepane_docs::extract(path).ok().map(|t| t.text)
            };
            let (index, summary) = Index::build(
                &roots,
                previous.as_deref(),
                &documents,
                &|| shared.cancel.load(Ordering::Relaxed),
                &|n| shared.files_seen.store(n, Ordering::Relaxed),
            );
            let message = if summary.cancelled {
                "Budowanie indeksu przerwane (został poprzedni)".to_string()
            } else {
                let saved = file
                    .as_deref()
                    .map(|f| index.save(f).map_err(|e| e.to_string()))
                    .transpose();
                let words = index.word_count();
                let mut message = format!(
                    "Indeks gotowy: {} plików, {words} słów (przeczytano {}, nieczytelnych {})",
                    summary.files, summary.read, summary.unreadable
                );
                if let Err(e) = saved {
                    message.push_str(&format!("; nie udało się go zapisać: {e}"));
                }
                if let Ok(mut slot) = shared.index.lock() {
                    *slot = Some(Arc::new(index));
                }
                message
            };
            if let Ok(mut slot) = shared.message.lock() {
                *slot = message;
            }
            shared.building.store(false, Ordering::Relaxed);
            shared.version.fetch_add(1, Ordering::Relaxed);
        });
    }

    fn stop(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }
}

/// `5 min`, `3 godz.`, `2 dni` since `built` (seconds since the epoch).
fn age(built: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let secs = now.saturating_sub(built);
    match secs {
        0..=89 => "przed chwilą".into(),
        90..=5399 => format!("{} min temu", secs / 60),
        5400..=129_599 => format!("{} godz. temu", secs / 3600),
        _ => format!("{} dni temu", secs / 86_400),
    }
}

fn check_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim().trim_matches('"').trim();
    if name.is_empty() || name.contains(['\t', '\n', '\r']) {
        return Err(EngineError("podaj nazwę zapisanego wyszukiwania".into()));
    }
    Ok(name.to_string())
}

/// `name` or `"name with spaces"` at the start of `text`, and what follows it.
fn take_name(text: &str) -> (String, String) {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('"') {
        if let Some((name, tail)) = rest.split_once('"') {
            return (name.to_string(), tail.trim().to_string());
        }
    }
    match text.split_once(char::is_whitespace) {
        Some((name, tail)) => (name.to_string(), tail.trim().to_string()),
        None => (text.to_string(), String::new()),
    }
}

/// `index [status|add|remove|update|stop|clear|find|save|open|list|forget]`
pub(crate) fn parse_index(rest: &str) -> Result<IndexAction, EngineError> {
    let rest = rest.trim();
    let (word, tail) = match rest.split_once(char::is_whitespace) {
        Some((word, tail)) => (word, tail.trim()),
        None => (rest, ""),
    };
    let need = |what: &str| -> Result<String, EngineError> {
        if tail.is_empty() {
            Err(EngineError(format!("index {word}: podaj {what}")))
        } else {
            Ok(tail.trim_matches('"').to_string())
        }
    };
    match word.to_lowercase().as_str() {
        "" | "status" => Ok(IndexAction::Status),
        "add" => need("katalog (albo here)").map(IndexAction::Add),
        "remove" => need("katalog").map(IndexAction::Remove),
        "update" | "rebuild" => Ok(IndexAction::Update),
        "stop" => Ok(IndexAction::Stop),
        "clear" => Ok(IndexAction::Clear),
        "find" => need("słowa do znalezienia").map(IndexAction::Find),
        "save" => {
            let (name, query) = take_name(tail);
            Ok(IndexAction::Save(
                check_name(&name)?,
                (!query.is_empty()).then_some(query),
            ))
        }
        "open" => need("nazwę").and_then(|n| check_name(&n)).map(IndexAction::Open),
        "list" => Ok(IndexAction::List),
        "forget" => need("nazwę")
            .and_then(|n| check_name(&n))
            .map(IndexAction::Forget),
        _ => Err(EngineError(
            "index: status | add <katalog> | remove <katalog> | update | stop | clear | find <słowa> | save <nazwa> [słowa] | open <nazwa> | list | forget <nazwa>"
                .into(),
        )),
    }
}

impl Session {
    fn index_file(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("index.dat"))
    }

    fn searches_file(&self) -> Result<PathBuf, EngineError> {
        self.config_dir()
            .map(|d| d.join("searches.conf"))
            .ok_or_else(|| EngineError("brak katalogu konfiguracji: nie ma gdzie zapisać".into()))
    }

    /// Called every frame: says in the status line when a build has finished.
    pub(crate) fn poll_index(&mut self) {
        let version = self.search_index.shared.version.load(Ordering::Relaxed);
        if version != self.index_version {
            self.index_version = version;
            if let Ok(message) = self.search_index.shared.message.lock() {
                self.status = message.clone();
            }
            self.status_is_error = false;
        }
    }

    fn saved_searches(&self) -> Result<Vec<(String, String)>, EngineError> {
        let text = match std::fs::read_to_string(self.searches_file()?) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return Err(EngineError(format!(
                    "nie udało się wczytać wyszukiwań: {e}"
                )))
            }
        };
        Ok(text
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(n, q)| (n.to_string(), q.to_string()))
            .collect())
    }

    fn write_searches(&self, list: &[(String, String)]) -> Result<(), EngineError> {
        let file = self.searches_file()?;
        let text: String = list.iter().map(|(n, q)| format!("{n}\t{q}\n")).collect();
        std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")))
            .and_then(|()| std::fs::write(&file, text))
            .map_err(|e| EngineError(format!("nie udało się zapisać wyszukiwań: {e}")))
    }

    /// A folder given on the command line: `here`, an absolute path or one relative to the
    /// active panel.
    fn index_folder(&self, text: &str) -> Result<PathBuf, EngineError> {
        let panel = self.active_panel();
        if panel.remote.is_some() || panel.archive.is_some() || panel.virtual_list.is_some() {
            return Err(EngineError(
                "indeks obejmuje katalogi na dysku (nie archiwa, serwery ani listy wyników)".into(),
            ));
        }
        let path = if text.eq_ignore_ascii_case("here") {
            panel.cwd.clone()
        } else {
            let given = PathBuf::from(text);
            if given.is_absolute() {
                given
            } else {
                panel.cwd.join(given)
            }
        };
        if !path.is_dir() {
            return Err(EngineError(format!("to nie katalog: {}", path.display())));
        }
        Ok(path)
    }

    fn guard_not_building(&self) -> Result<(), EngineError> {
        if self.search_index.is_building() {
            return Err(EngineError(
                "indeks jest właśnie budowany: poczekaj albo `index stop`".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn index_action(&mut self, action: IndexAction) -> Result<(), EngineError> {
        let file = self.index_file();
        self.search_index.ensure_loaded(file.as_deref())?;
        match action {
            IndexAction::Status => self.index_status(),
            IndexAction::Add(text) => {
                self.guard_not_building()?;
                let folder = self.index_folder(&text)?;
                let mut base = self
                    .search_index
                    .current()
                    .map(|i| (*i).clone())
                    .unwrap_or_default();
                if !base.add_root(folder.clone()) {
                    return Err(EngineError(format!(
                        "{} jest już w indeksie (sam albo przez katalog wyżej)",
                        folder.display()
                    )));
                }
                let roots = base.roots().to_vec();
                self.search_index.start(roots, Some(Arc::new(base)), file);
                self.status = format!(
                    "Budowanie indeksu dla {} w tle (index stop przerywa; index pokazuje postęp)",
                    folder.display()
                );
            }
            IndexAction::Remove(text) => {
                self.guard_not_building()?;
                let folder = PathBuf::from(text.trim_matches('"'));
                let folder = if folder.is_absolute() {
                    folder
                } else {
                    self.active_panel().cwd.join(folder)
                };
                let mut index = self
                    .search_index
                    .current()
                    .map(|i| (*i).clone())
                    .ok_or_else(|| EngineError("indeks jest pusty".into()))?;
                if !index.remove_root(&folder) {
                    return Err(EngineError(format!(
                        "{} nie jest katalogiem głównym indeksu (index pokazuje listę)",
                        folder.display()
                    )));
                }
                if let Some(file) = &file {
                    index
                        .save(file)
                        .map_err(|e| EngineError(format!("nie udało się zapisać indeksu: {e}")))?;
                }
                let left = index.file_count();
                self.search_index.set(Some(Arc::new(index)));
                self.status = format!(
                    "Usunięto {} z indeksu (zostało plików: {left})",
                    folder.display()
                );
            }
            IndexAction::Update => {
                self.guard_not_building()?;
                let index = self.search_index.current().ok_or_else(|| {
                    EngineError("indeks jest pusty: najpierw index add <katalog>".into())
                })?;
                let roots = index.roots().to_vec();
                self.search_index.start(roots, Some(index), file);
                self.status =
                    "Aktualizacja indeksu w tle (czytane są tylko zmienione pliki)".into();
            }
            IndexAction::Stop => {
                if !self.search_index.is_building() {
                    return Err(EngineError("indeks nie jest teraz budowany".into()));
                }
                self.search_index.stop();
                self.status = "Przerywam budowanie indeksu…".into();
            }
            IndexAction::Clear => {
                self.search_index.stop();
                self.search_index.set(None);
                if let Some(file) = &file {
                    let _ = std::fs::remove_file(file);
                }
                self.status = "Indeks usunięty (zapisane wyszukiwania zostają)".into();
            }
            IndexAction::Find(query) => self.index_find(&query, None)?,
            IndexAction::Save(name, query) => {
                let query = query
                    .or_else(|| {
                        (!self.index_last_query.is_empty()).then(|| self.index_last_query.clone())
                    })
                    .ok_or_else(|| {
                        EngineError(
                            "brak zapytania: index save <nazwa> <słowa> albo najpierw index find"
                                .into(),
                        )
                    })?;
                if query.contains(['\t', '\n']) {
                    return Err(EngineError(
                        "zapytanie nie może zawierać tabulatorów".into(),
                    ));
                }
                let mut list = self.saved_searches()?;
                list.retain(|(n, _)| *n != name);
                list.push((name.clone(), query.clone()));
                list.sort();
                self.write_searches(&list)?;
                self.status = format!("Zapisano wyszukiwanie „{name}”: {query}");
            }
            IndexAction::Open(name) => {
                let query = self
                    .saved_searches()?
                    .into_iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, q)| q)
                    .ok_or_else(|| {
                        EngineError(format!(
                            "nie ma zapisanego wyszukiwania „{name}” (index list)"
                        ))
                    })?;
                self.index_find(&query, Some(&name))?;
            }
            IndexAction::List => {
                let list = self.saved_searches()?;
                self.status = if list.is_empty() {
                    "Brak zapisanych wyszukiwań (index save <nazwa> <słowa>)".into()
                } else {
                    let items: Vec<String> =
                        list.iter().map(|(n, q)| format!("{n}: {q}")).collect();
                    format!("Zapisane wyszukiwania: {}", items.join("; "))
                };
            }
            IndexAction::Forget(name) => {
                let mut list = self.saved_searches()?;
                let before = list.len();
                list.retain(|(n, _)| *n != name);
                if list.len() == before {
                    return Err(EngineError(format!(
                        "nie ma zapisanego wyszukiwania „{name}”"
                    )));
                }
                self.write_searches(&list)?;
                self.status = format!("Usunięto zapisane wyszukiwanie „{name}”");
            }
        }
        Ok(())
    }

    fn index_status(&mut self) {
        let building = self.search_index.is_building();
        self.status = match self.search_index.current() {
            None if building => format!(
                "Budowanie indeksu: przejrzano {} plików…",
                self.search_index.files_seen()
            ),
            None => "Indeks jest pusty: index add <katalog> (albo here) budowanie zaczyna się dopiero wtedy".into(),
            Some(index) => {
                let roots: Vec<String> = index.roots().iter().map(|r| r.display().to_string()).collect();
                let mut text = format!(
                    "Indeks: {} plików, {} słów, zbudowany {}; katalogi: {}",
                    index.file_count(),
                    index.word_count(),
                    age(index.built_at),
                    roots.join(", ")
                );
                if building {
                    text.push_str(&format!(
                        "; trwa aktualizacja ({} plików)",
                        self.search_index.files_seen()
                    ));
                }
                text
            }
        };
    }

    /// Asks the index and shows the files in the active panel.
    fn index_find(&mut self, query: &str, saved_as: Option<&str>) -> Result<(), EngineError> {
        let index = self.search_index.current().ok_or_else(|| {
            EngineError("indeks jest pusty: najpierw index add <katalog> (albo here)".into())
        })?;
        let parsed = IndexQuery::parse(query);
        if parsed.is_empty() {
            return Err(EngineError("podaj słowa do znalezienia".into()));
        }
        let found = index.search(&parsed);
        self.index_last_query = query.to_string();
        if found.is_empty() {
            self.status = format!(
                "Indeks: nic nie pasuje do „{query}” ({})",
                age(index.built_at)
            );
            return Ok(());
        }
        let paths: Vec<PathBuf> = found.iter().map(|f| f.path.clone()).collect();
        let count = paths.len();
        let title = match saved_as {
            Some(name) => format!("Wyszukiwanie „{name}” ({count})"),
            None => format!("Indeks: {query} ({count})"),
        };
        self.active_panel_mut().show_paths(title, paths);
        self.status = format!(
            "Indeks: {count} z {} plików, zbudowany {}. Backspace wraca do katalogu",
            index.file_count(),
            age(index.built_at)
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{Duration, Instant};

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        std::fs::create_dir_all(left.join("Dokumenty")).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(
            left.join("Dokumenty/umowa.txt"),
            "Umowa o dzieło: żółw i gęś",
        )
        .unwrap();
        std::fs::write(left.join("Zażółć gęślą.txt"), "nic").unwrap();
        std::fs::write(left.join("notatki.md"), "zakupy: mleko, chleb").unwrap();
        let mut s = Session::new(&left, &right);
        s.use_config_dir(&tmp.path().join("config"));
        (tmp, s)
    }

    /// Polls like the window does until the background build has ended.
    fn wait_built(s: &mut Session) {
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            s.poll();
            if !s.search_index.is_building() {
                s.poll();
                return;
            }
            assert!(Instant::now() < end, "the index was not built in time");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn panel_names(s: &Session) -> Vec<String> {
        s.active_panel()
            .entries
            .iter()
            .filter(|e| e.name != "..")
            .map(|e| e.name.clone())
            .collect()
    }

    fn docx(dir: &Path) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(dir.join("raport.docx")).unwrap());
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(
            "<w:document><w:body><w:p><w:r><w:t>Kwartalny raport sprzedaży</w:t></w:r></w:p></w:body></w:document>"
                .as_bytes(),
        )
        .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn nothing_is_indexed_until_a_folder_is_added() {
        let (_tmp, mut s) = session();
        s.run_line("index").unwrap();
        assert!(s.status.contains("pusty"), "{}", s.status);
        assert!(!s.search_index.is_building());
        let err = s.run_line("index find umowa").unwrap_err();
        assert!(err.0.contains("index add"), "{err}");
        assert!(s.run_line("index update").is_err());
        assert!(s.run_line("index stop").is_err());
    }

    #[test]
    fn an_added_folder_is_built_in_the_background_and_searched_without_accents() {
        let (tmp, mut s) = session();
        docx(&tmp.path().join("left"));
        s.run_line("index add here").unwrap();
        assert!(s.status.contains("w tle"), "{}", s.status);
        wait_built(&mut s);
        assert!(
            s.status.starts_with("Indeks gotowy: 4 plików"),
            "{}",
            s.status
        );

        s.run_line("index find zolw").unwrap();
        assert_eq!(panel_names(&s), ["umowa.txt"]);
        assert!(s.active_panel().virtual_list.is_some());
        s.run_line("index find sprzedazy").unwrap();
        assert_eq!(
            panel_names(&s),
            ["raport.docx"],
            "the text of a Word document"
        );
        s.run_line("index find zazolc").unwrap();
        assert_eq!(panel_names(&s), ["Zażółć gęślą.txt"], "a file name");
        s.run_line("index find name:umowa ext:txt").unwrap();
        assert_eq!(panel_names(&s), ["umowa.txt"]);
        s.run_line("index find tego nie ma").unwrap();
        assert!(s.status.contains("nic nie pasuje"), "{}", s.status);
        // The list can be worked on like any other: the file under the cursor opens its folder.
        s.run_line("index find mleko").unwrap();
        assert_eq!(panel_names(&s), ["notatki.md"]);
    }

    #[test]
    fn the_index_is_kept_between_sessions_and_updated_for_changed_files_only() {
        let (tmp, mut s) = session();
        s.run_line("index add here").unwrap();
        wait_built(&mut s);
        assert!(tmp.path().join("config/index.dat").exists());

        let mut again = Session::new(tmp.path().join("left"), tmp.path().join("right"));
        again.use_config_dir(&tmp.path().join("config"));
        again.run_line("index").unwrap();
        assert!(again.status.contains("3 plików"), "{}", again.status);
        again.run_line("index find chleb").unwrap();
        assert_eq!(panel_names(&again), ["notatki.md"]);

        std::fs::write(tmp.path().join("left/notatki.md"), "zakupy: ser, masło").unwrap();
        std::fs::write(tmp.path().join("left/nowy.txt"), "ser żółty").unwrap();
        again.run_line("index update").unwrap();
        wait_built(&mut again);
        assert!(again.status.contains("4 plików"), "{}", again.status);
        assert!(
            again.status.contains("przeczytano 2"),
            "only the changed ones: {}",
            again.status
        );
        again.run_line("index find maslo").unwrap();
        assert_eq!(panel_names(&again), ["notatki.md"]);
        again.run_line("index find chleb").unwrap();
        assert!(again.status.contains("nic nie pasuje"), "{}", again.status);
    }

    #[test]
    fn folders_can_be_removed_and_the_whole_index_cleared() {
        let (tmp, mut s) = session();
        s.run_line("index add here").unwrap();
        wait_built(&mut s);
        let err = s.run_line("index add here").unwrap_err();
        assert!(err.0.contains("już w indeksie"), "{err}");
        let err = s.run_line("index add Dokumenty").unwrap_err();
        assert!(
            err.0.contains("już w indeksie"),
            "inside a folder that is in: {err}"
        );
        assert!(s.run_line("index add nie-ma-takiego").is_err());
        assert!(s.run_line("index add").is_err());

        let right = tmp.path().join("right");
        std::fs::write(right.join("prawy.txt"), "ser").unwrap();
        s.run_line(&format!("index add \"{}\"", right.display()))
            .unwrap();
        wait_built(&mut s);
        s.run_line("index find ser").unwrap();
        assert_eq!(panel_names(&s), ["prawy.txt"]);
        s.run_line(&format!("index remove {}", right.display()))
            .unwrap();
        assert!(s.status.contains("zostało plików: 3"), "{}", s.status);
        s.run_line("index find ser").unwrap();
        assert!(s.status.contains("nic nie pasuje"), "{}", s.status);
        assert!(s
            .run_line(&format!("index remove {}", right.display()))
            .is_err());

        s.run_line("index clear").unwrap();
        assert!(!tmp.path().join("config/index.dat").exists());
        assert!(s.run_line("index find umowa").is_err());
    }

    #[test]
    fn a_build_can_be_stopped_and_keeps_the_old_index() {
        let (tmp, mut s) = session();
        s.run_line("index add here").unwrap();
        wait_built(&mut s);
        // A big folder takes a moment, and stopping is asked for at once.
        let big = tmp.path().join("big");
        std::fs::create_dir_all(&big).unwrap();
        for i in 0..3000 {
            std::fs::write(big.join(format!("p{i}.txt")), format!("slowo{i}")).unwrap();
        }
        s.run_line(&format!("index add {}", big.display())).unwrap();
        assert!(s.run_line("index add here").is_err(), "one build at a time");
        s.run_line("index stop").unwrap();
        wait_built(&mut s);
        // Either it was fast enough to finish or it was stopped: the index answers either way.
        s.run_line("index find umowa").unwrap();
        assert_eq!(panel_names(&s), ["umowa.txt"]);
        let _ = tmp;
    }

    #[test]
    fn a_saved_search_is_a_folder_that_is_asked_again_each_time() {
        let (tmp, mut s) = session();
        s.run_line("index add here").unwrap();
        wait_built(&mut s);
        assert!(s.run_line("index save puste").is_err(), "no query yet");
        s.run_line("index find zakupy").unwrap();
        s.run_line("index save \"Moje zakupy\"").unwrap();
        s.run_line("index save dokumenty umowa ext:txt").unwrap();
        s.run_line("index list").unwrap();
        assert!(
            s.status.contains("Moje zakupy: zakupy")
                && s.status.contains("dokumenty: umowa ext:txt"),
            "{}",
            s.status
        );

        s.run_line("index open dokumenty").unwrap();
        assert_eq!(panel_names(&s), ["umowa.txt"]);
        let title = s
            .active_panel()
            .virtual_list
            .as_ref()
            .unwrap()
            .title
            .clone();
        assert!(title.contains("dokumenty"), "{title}");
        // New content, then an update: the same name shows the new files.
        std::fs::write(tmp.path().join("left/zakupy2.txt"), "zakupy jutro").unwrap();
        s.run_line("index update").unwrap();
        wait_built(&mut s);
        s.run_line("index open \"Moje zakupy\"").unwrap();
        assert_eq!(panel_names(&s), ["notatki.md", "zakupy2.txt"]);

        // Saved searches outlive the session; forgetting removes one.
        let mut again = Session::new(tmp.path().join("left"), tmp.path().join("right"));
        again.use_config_dir(&tmp.path().join("config"));
        again.run_line("index list").unwrap();
        assert!(again.status.contains("dokumenty"), "{}", again.status);
        again.run_line("index forget dokumenty").unwrap();
        assert!(again.run_line("index open dokumenty").is_err());
        assert!(again.run_line("index forget dokumenty").is_err());
        again.run_line("index list").unwrap();
        assert!(!again.status.contains("dokumenty:"), "{}", again.status);
    }

    #[test]
    fn the_index_refuses_archives_and_servers_and_parses_strictly() {
        assert_eq!(parse_index("").unwrap(), IndexAction::Status);
        assert_eq!(
            parse_index("save \"Moje zakupy\" mleko ext:md").unwrap(),
            IndexAction::Save("Moje zakupy".into(), Some("mleko ext:md".into()))
        );
        assert_eq!(
            parse_index("Save nazwa").unwrap(),
            IndexAction::Save("nazwa".into(), None)
        );
        assert_eq!(
            parse_index("find zażółć gęślą").unwrap(),
            IndexAction::Find("zażółć gęślą".into())
        );
        for bad in ["find", "add", "open", "forget", "save", "wat"] {
            assert!(parse_index(bad).is_err(), "{bad:?}");
        }
        let (tmp, mut s) = session();
        s.panels[0].show_paths("wyniki".into(), vec![tmp.path().join("left/notatki.md")]);
        let err = s.run_line("index add here").unwrap_err();
        assert!(err.0.contains("na dysku"), "{err}");
    }
}
