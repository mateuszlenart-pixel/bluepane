//! The search window's state and commands (`find`, `findset`, `findrun`, `findstop`,
//! `findgo`, `findview`, `findclose`). The searching itself is `bluepane-search`; it runs on
//! its own thread and fills a shared list the window reads every frame.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use bluepane_ops::Progress;
use bluepane_search::{Encoding, Hit, Location, Query, Summary, TextQuery};
use chrono::{Local, NaiveDate, TimeZone};

use crate::session::Session;
use crate::{parse_size, EngineError, Viewer};

/// What the user typed in the search window; turned into a `Query` by `findrun`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindForm {
    /// Folders separated by `;`, or the words `here` (the active panel), `both` (both
    /// panels) and `all` (every drive).
    pub roots: String,
    pub name: String,
    pub name_regex: bool,
    pub name_case: bool,
    pub exclude: String,
    pub text: String,
    pub text_regex: bool,
    pub text_case: bool,
    pub words: bool,
    pub encoding: Encoding,
    pub binary: bool,
    pub archives: bool,
    pub password: String,
    pub hidden: bool,
    pub min: String,
    pub max: String,
    /// `YYYY-MM-DD`
    pub after: String,
    pub before: String,
    pub limit: String,
}

impl FindForm {
    fn new(here: &std::path::Path) -> FindForm {
        FindForm {
            roots: here.display().to_string(),
            name: String::new(),
            name_regex: false,
            name_case: false,
            exclude: String::new(),
            text: String::new(),
            text_regex: false,
            text_case: false,
            words: false,
            encoding: Encoding::Auto,
            binary: false,
            archives: false,
            password: String::new(),
            hidden: true,
            min: String::new(),
            max: String::new(),
            after: String::new(),
            before: String::new(),
            limit: "10000".into(),
        }
    }
}

/// What the worker thread produces.
#[derive(Debug, Default)]
pub struct SearchData {
    pub hits: Vec<Hit>,
    /// Set when the search ended (finished, stopped or hit the result limit).
    pub summary: Option<Summary>,
    /// Set when the query could not run at all.
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct FindView {
    pub form: FindForm,
    /// False after `findclose` / `findgo`: the results are kept but not shown.
    pub visible: bool,
    data: Arc<Mutex<SearchData>>,
    progress: Option<Progress>,
    /// A search was started at least once.
    pub started: bool,
}

impl FindView {
    /// The hits found so far and how the search ended.
    pub fn data(&self) -> MutexGuard<'_, SearchData> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_running(&self) -> bool {
        self.started && {
            let data = self.data();
            data.summary.is_none() && data.error.is_none()
        }
    }

    /// The folder or archive being searched and how many files were looked at.
    pub fn progress_line(&self) -> Option<(String, u64)> {
        let progress = self.progress.as_ref()?;
        Some((progress.current(), progress.items_done()))
    }
}

fn parse_bool(field: &str, value: &str) -> Result<bool, EngineError> {
    match value.to_lowercase().as_str() {
        "" | "on" | "1" | "true" | "yes" | "tak" => Ok(true),
        "off" | "0" | "false" | "no" | "nie" => Ok(false),
        _ => Err(EngineError(format!(
            "findset {field}: oczekiwano on albo off"
        ))),
    }
}

fn parse_date(field: &str, text: &str) -> Result<Option<SystemTime>, EngineError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let day = NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").map_err(|_| {
        EngineError(format!(
            "findset {field}: data w postaci RRRR-MM-DD, np. 2026-10-14"
        ))
    })?;
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| Some(t.into()))
        .ok_or_else(|| EngineError(format!("findset {field}: niepoprawna data")))
}

fn parse_limit(text: &str) -> Result<usize, EngineError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    text.parse()
        .map_err(|_| EngineError("findset limit: oczekiwano liczby (0 = bez limitu)".into()))
}

impl Session {
    /// `find`: open the window (the last one again if there is one).
    pub(crate) fn open_find(&mut self) {
        let here = self.panel_dir(self.active);
        match &mut self.find {
            Some(view) => view.visible = true,
            None => {
                self.find = Some(FindView {
                    form: FindForm::new(&here),
                    visible: true,
                    data: Arc::default(),
                    progress: None,
                    started: false,
                });
            }
        }
        self.status = "Wyszukiwanie: ustaw kryteria i naciśnij Szukaj".into();
    }

    /// The folder a panel shows (for an archive, the folder holding the archive file).
    fn panel_dir(&self, side: usize) -> PathBuf {
        let panel = &self.panels[side];
        match &panel.archive {
            Some(view) => view
                .archive
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| panel.cwd.clone()),
            None => panel.cwd.clone(),
        }
    }

    fn find_view_mut(&mut self) -> Result<&mut FindView, EngineError> {
        self.find
            .as_mut()
            .ok_or_else(|| EngineError("najpierw otwórz wyszukiwanie (find)".into()))
    }

    pub(crate) fn find_set(&mut self, field: &str, value: &str) -> Result<(), EngineError> {
        let here = self.panel_dir(self.active);
        let view = self.find_view_mut()?;
        let f = &mut view.form;
        match field {
            "roots" => f.roots = value.to_string(),
            "name" => f.name = value.to_string(),
            "namergx" => f.name_regex = parse_bool(field, value)?,
            "namecase" => f.name_case = parse_bool(field, value)?,
            "exclude" => f.exclude = value.to_string(),
            "text" => f.text = value.to_string(),
            "regex" => f.text_regex = parse_bool(field, value)?,
            "case" => f.text_case = parse_bool(field, value)?,
            "words" => f.words = parse_bool(field, value)?,
            "enc" => {
                f.encoding = match value.to_lowercase().as_str() {
                    "" | "auto" => Encoding::Auto,
                    "utf8" | "utf-8" => Encoding::Utf8,
                    "cp1250" | "1250" => Encoding::Cp1250,
                    "utf16" | "utf-16" => Encoding::Utf16,
                    _ => {
                        return Err(EngineError(
                            "findset enc: auto, utf8, cp1250 albo utf16".into(),
                        ))
                    }
                }
            }
            "binary" => f.binary = parse_bool(field, value)?,
            "archives" => f.archives = parse_bool(field, value)?,
            "password" => f.password = value.to_string(),
            "hidden" => f.hidden = parse_bool(field, value)?,
            "min" => f.min = value.to_string(),
            "max" => f.max = value.to_string(),
            "after" => f.after = value.to_string(),
            "before" => f.before = value.to_string(),
            "limit" => f.limit = value.to_string(),
            "reset" => *f = FindForm::new(&here),
            _ => return Err(EngineError(format!("findset: nieznane pole {field}"))),
        }
        Ok(())
    }

    /// The folders the roots field stands for.
    fn find_roots(&self, text: &str) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_lowercase().as_str() {
                "here" | "tu" => roots.push(self.panel_dir(self.active)),
                "other" | "drugi" => roots.push(self.panel_dir(1 - self.active)),
                "both" | "oba" => {
                    roots.push(self.panel_dir(0));
                    roots.push(self.panel_dir(1));
                }
                "all" | "wszystkie" => {
                    roots.extend(bluepane_vfs::volumes().into_iter().map(|v| v.path));
                }
                _ => roots.push(PathBuf::from(part)),
            }
        }
        roots
    }

    fn build_query(&self, form: &FindForm) -> Result<Query, EngineError> {
        let size = |field: &str, text: &str| -> Result<Option<u64>, EngineError> {
            if text.trim().is_empty() {
                Ok(None)
            } else {
                parse_size(text)
                    .map(Some)
                    .map_err(|e| EngineError(format!("findset {field}: {e}")))
            }
        };
        let mut before = parse_date("before", &form.before)?;
        // "Before 2026-10-14" includes that whole day.
        if let Some(time) = before.as_mut() {
            *time += Duration::from_secs(24 * 3600);
        }
        let text = (!form.text.is_empty()).then(|| TextQuery {
            pattern: form.text.clone(),
            regex: form.text_regex,
            case_sensitive: form.text_case,
            whole_words: form.words,
            encoding: form.encoding,
            search_binary: form.binary,
        });
        Ok(Query {
            roots: self.find_roots(&form.roots),
            name: form.name.clone(),
            name_regex: form.name_regex,
            name_case_sensitive: form.name_case,
            exclude_dirs: form.exclude.clone(),
            text,
            archives: form.archives,
            archive_password: Some(form.password.clone()).filter(|p| !p.is_empty()),
            include_hidden: form.hidden,
            min_size: size("min", &form.min)?,
            max_size: size("max", &form.max)?,
            modified_after: parse_date("after", &form.after)?,
            modified_before: before,
            max_results: parse_limit(&form.limit)?,
            ..Query::default()
        })
    }

    pub(crate) fn find_run(&mut self) -> Result<(), EngineError> {
        let form = self.find_view_mut()?.form.clone();
        let query = self.build_query(&form)?;
        if query.roots.is_empty() {
            return Err(EngineError("nie wskazano, gdzie szukać".into()));
        }
        if query.name.trim().is_empty() && query.text.is_none() {
            return Err(EngineError("podaj nazwę (maskę) albo szukany tekst".into()));
        }
        let view = self.find_view_mut()?;
        // An older search still running is told to stop; it keeps its own, now orphaned,
        // result list.
        if let Some(old) = &view.progress {
            old.cancel();
        }
        let data: Arc<Mutex<SearchData>> = Arc::default();
        let progress = Progress::new();
        view.data = data.clone();
        view.progress = Some(progress.clone());
        view.started = true;
        std::thread::spawn(move || {
            let outcome = bluepane_search::search(&query, &progress, &mut |hit| {
                data.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .hits
                    .push(hit);
            });
            let mut data = data.lock().unwrap_or_else(|e| e.into_inner());
            match outcome {
                Ok(summary) => data.summary = Some(summary),
                Err(text) => data.error = Some(text),
            }
        });
        self.status = "Szukanie…".into();
        Ok(())
    }

    pub(crate) fn find_stop(&mut self) {
        if let Some(progress) = self.find.as_ref().and_then(|v| v.progress.as_ref()) {
            progress.cancel();
        }
    }

    pub(crate) fn find_close(&mut self) {
        if let Some(view) = &mut self.find {
            view.visible = false;
        }
    }

    fn find_hit(&self, n: usize) -> Result<Hit, EngineError> {
        let view = self
            .find
            .as_ref()
            .ok_or_else(|| EngineError("najpierw otwórz wyszukiwanie (find)".into()))?;
        let data = view.data();
        data.hits
            .get(n - 1)
            .cloned()
            .ok_or_else(|| EngineError(format!("nie ma wyniku {n}")))
    }

    /// `findgo n`: show the result in the active panel.
    pub(crate) fn find_go(&mut self, n: usize) -> Result<(), EngineError> {
        let hit = self.find_hit(n)?;
        let target = match &hit.location {
            Location::File(path) => path.clone(),
            Location::InArchive { archive, .. } => archive.clone(),
        };
        let Some(dir) = target.parent().map(PathBuf::from) else {
            return Err(EngineError("wynik nie ma katalogu nadrzędnego".into()));
        };
        let panel = self.active_panel_mut();
        panel.change_dir(dir);
        panel.filter.clear();
        panel.put_cursor_on(&target);
        self.find_close();
        self.status = match &hit.location {
            Location::File(_) => format!("Przejście do: {}", target.display()),
            Location::InArchive { inner, .. } => format!(
                "Archiwum: {} (plik w środku: {inner}; Enter wchodzi do archiwum)",
                target.display()
            ),
        };
        Ok(())
    }

    /// `findview n`: open the result in the viewer, at the first match for a text search.
    pub(crate) fn find_view(&mut self, n: usize) -> Result<(), EngineError> {
        let hit = self.find_hit(n)?;
        let Location::File(path) = &hit.location else {
            return Err(EngineError(
                "podgląd pliku z archiwum: przejdź do archiwum (findgo) i naciśnij F3".into(),
            ));
        };
        if hit.is_dir {
            return Err(EngineError("to katalog, nie plik (użyj findgo)".into()));
        }
        let mut viewer = Viewer::open(path)?;
        let text =
            self.find.as_ref().map(|v| v.form.text.clone()).filter(|t| {
                !t.is_empty() && !self.find.as_ref().is_some_and(|v| v.form.text_regex)
            });
        if let Some(text) = text {
            viewer.find(&text, true);
        }
        self.status = format!("Podgląd: {}", path.display());
        self.viewer = Some(viewer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        fs::create_dir_all(left.join("sub")).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("a.txt"), "one\nneedle here\n").unwrap();
        fs::write(left.join("sub/b.txt"), "nothing\n").unwrap();
        fs::write(left.join("sub/c.md"), "a NEEDLE\n").unwrap();
        fs::write(right.join("d.txt"), "needle too\n").unwrap();
        (tmp, Session::new(left, right))
    }

    /// Waits for the search thread to finish.
    fn finish(s: &Session) {
        for _ in 0..500 {
            if !s.find.as_ref().unwrap().is_running() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the search did not finish");
    }

    fn found(s: &Session) -> Vec<String> {
        let data = s.find.as_ref().unwrap().data();
        let mut names: Vec<String> = data.hits.iter().map(|h| h.location.name()).collect();
        names.sort();
        names
    }

    #[test]
    fn searches_the_active_panel_by_default() {
        let (_tmp, mut s) = session();
        s.run_line("find").unwrap();
        assert!(s.find.as_ref().unwrap().visible);
        s.run_line("findset name *.txt").unwrap();
        s.run_line("findrun").unwrap();
        finish(&s);
        assert_eq!(found(&s), ["a.txt", "b.txt"]);
        let view = s.find.as_ref().unwrap();
        assert_eq!(view.data().summary.as_ref().unwrap().hits, 2);
    }

    #[test]
    fn full_text_search_in_both_panels() {
        let (_tmp, mut s) = session();
        s.run_line("find").unwrap();
        s.run_line("findset roots both").unwrap();
        s.run_line("findset text needle").unwrap();
        s.run_line("findrun").unwrap();
        finish(&s);
        assert_eq!(found(&s), ["a.txt", "c.md", "d.txt"]);
        let case_sensitive = {
            s.run_line("findset case on").unwrap();
            s.run_line("findrun").unwrap();
            finish(&s);
            found(&s)
        };
        assert_eq!(case_sensitive, ["a.txt", "d.txt"]);
    }

    #[test]
    fn roots_can_be_listed_explicitly_and_all_drives_resolve() {
        let (tmp, mut s) = session();
        s.run_line("find").unwrap();
        s.run_line(&format!(
            "findset roots {};{}",
            tmp.path().join("right").display(),
            tmp.path().join("left/sub").display()
        ))
        .unwrap();
        s.run_line("findset name *.*").unwrap();
        s.run_line("findrun").unwrap();
        finish(&s);
        assert_eq!(found(&s), ["b.txt", "c.md", "d.txt"]);
        assert!(!s.find_roots("all").is_empty(), "at least one volume");
    }

    #[test]
    fn go_to_a_result_moves_the_panel_and_hides_the_window() {
        let (tmp, mut s) = session();
        s.run_line("find").unwrap();
        s.run_line("findset text NEEDLE").unwrap();
        s.run_line("findset name *.md").unwrap();
        s.run_line("findrun").unwrap();
        finish(&s);
        s.run_line("findgo 1").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/sub"));
        assert_eq!(s.active_panel().current().unwrap().name, "c.md");
        assert!(!s.find.as_ref().unwrap().visible);
        assert_eq!(found(&s), ["c.md"], "results are kept");
        s.run_line("find").unwrap();
        assert!(s.find.as_ref().unwrap().visible);
        assert!(s.run_line("findgo 9").is_err());
    }

    #[test]
    fn view_opens_the_file_at_the_match() {
        let (_tmp, mut s) = session();
        s.run_line("find").unwrap();
        s.run_line("findset text needle").unwrap();
        s.run_line("findset name a.txt").unwrap();
        s.run_line("findrun").unwrap();
        finish(&s);
        s.run_line("findview 1").unwrap();
        assert!(s.viewer.is_some());
    }

    #[test]
    fn bad_input_is_reported() {
        let (_tmp, mut s) = session();
        assert!(s.run_line("findrun").is_err(), "window is not open");
        s.run_line("find").unwrap();
        assert!(s.run_line("findrun").is_err(), "nothing to look for");
        assert!(s.run_line("findset bogus 1").is_err());
        assert!(s.run_line("findset enc latin9").is_err());
        assert!(
            s.run_line("findset after yesterday").is_ok(),
            "checked at run"
        );
        s.run_line("findset name x").unwrap();
        assert!(s.run_line("findrun").is_err(), "bad date");
        s.run_line("findset after 2020-01-31").unwrap();
        s.run_line("findset min 1x").unwrap();
        assert!(s.run_line("findrun").is_err(), "bad size");
        s.run_line("findset min 1k").unwrap();
        s.run_line("findset regex on").unwrap();
        s.run_line("findset text (").unwrap();
        s.run_line("findrun").unwrap();
        finish(&s);
        let view = s.find.as_ref().unwrap();
        assert!(
            view.data().error.is_some(),
            "a broken regex is reported by the search"
        );
    }

    #[test]
    fn stop_ends_a_search_and_keeps_the_hits() {
        let (_tmp, mut s) = session();
        s.run_line("find").unwrap();
        s.run_line("findset name *").unwrap();
        s.run_line("findrun").unwrap();
        s.run_line("findstop").unwrap();
        finish(&s);
        s.run_line("findclose").unwrap();
        assert!(!s.find.as_ref().unwrap().visible);
    }
}
