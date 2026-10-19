//! The panel's quick filter (`filter`, typing over the list): name fragments plus optional
//! criteria in the same text, e.g. `raport typ:dokument >1m nowsze:7d`. Every part must
//! hold. Name fragments ignore letter case and Polish accents.

use std::time::{Duration, SystemTime};

use bluepane_vfs::Entry;

use crate::palette::fold;
use crate::{class_of, FileClass};

/// Keep only folders or only files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Only {
    Dirs,
    Files,
}

/// A filter text taken apart.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilterSpec {
    /// Fragments the name must contain (folded: lower case, no accents).
    pub words: Vec<String>,
    pub kinds: Vec<FileClass>,
    /// Extensions without the dot, lower case.
    pub exts: Vec<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// Changed within this long.
    pub newer: Option<Duration>,
    /// Changed longer ago than this.
    pub older: Option<Duration>,
    pub only: Option<Only>,
}

fn kind_of_word(word: &str) -> Option<FileClass> {
    Some(match word {
        "katalog" | "dir" => FileClass::Dir,
        "obraz" | "obrazy" | "image" => FileClass::Image,
        "dokument" | "dokumenty" | "doc" => FileClass::Document,
        "archiwum" | "archiwa" | "archive" => FileClass::Archive,
        "audio" | "muzyka" => FileClass::Audio,
        "wideo" | "film" | "video" => FileClass::Video,
        "kod" | "code" => FileClass::Code,
        "tekst" | "text" => FileClass::Text,
        "program" | "programy" | "exe" => FileClass::Executable,
        _ => return None,
    })
}

/// `7d`, `12h`, `2w` (days, hours, weeks).
fn duration_of(text: &str) -> Option<Duration> {
    let (number, unit) = text.split_at(text.find(|c: char| !c.is_ascii_digit())?);
    let n: u64 = number.parse().ok()?;
    let hours = match unit {
        "h" => n,
        "d" => n * 24,
        "w" | "t" => n * 24 * 7,
        _ => return None,
    };
    Some(Duration::from_secs(hours * 3600))
}

impl FilterSpec {
    /// Takes a filter text apart; anything that is not a known criterion is a name fragment.
    pub fn parse(text: &str) -> FilterSpec {
        let mut spec = FilterSpec::default();
        for raw in text.split_whitespace() {
            let word = raw.to_lowercase();
            let criterion = if let Some(kind) =
                word.strip_prefix("typ:").or(word.strip_prefix("type:"))
            {
                kind_of_word(kind).map(|k| spec.kinds.push(k))
            } else if let Some(exts) = word.strip_prefix("ext:") {
                let exts: Vec<String> = exts
                    .split(',')
                    .map(|e| e.trim_start_matches('.').to_string())
                    .filter(|e| !e.is_empty())
                    .collect();
                (!exts.is_empty()).then(|| spec.exts.extend(exts))
            } else if let Some(size) = word.strip_prefix('>') {
                crate::parse_size(size)
                    .ok()
                    .map(|s| spec.min_size = Some(s))
            } else if let Some(size) = word.strip_prefix('<') {
                crate::parse_size(size)
                    .ok()
                    .map(|s| spec.max_size = Some(s))
            } else if let Some(age) = word.strip_prefix("nowsze:").or(word.strip_prefix("newer:")) {
                duration_of(age).map(|d| spec.newer = Some(d))
            } else if let Some(age) = word
                .strip_prefix("starsze:")
                .or(word.strip_prefix("older:"))
            {
                duration_of(age).map(|d| spec.older = Some(d))
            } else if matches!(word.as_str(), "katalogi" | "dirs") {
                spec.only = Some(Only::Dirs);
                Some(())
            } else if matches!(word.as_str(), "pliki" | "files") {
                spec.only = Some(Only::Files);
                Some(())
            } else {
                None
            };
            if criterion.is_none() {
                spec.words.push(fold(raw));
            }
        }
        spec
    }

    pub fn is_empty(&self) -> bool {
        *self == FilterSpec::default()
    }

    /// True when the entry passes every part of the filter (`now` for the age criteria).
    pub fn matches(&self, entry: &Entry, now: SystemTime) -> bool {
        let name = fold(&entry.name);
        if !self.words.iter().all(|w| name.contains(w.as_str())) {
            return false;
        }
        match self.only {
            Some(Only::Dirs) if !entry.is_dir() => return false,
            Some(Only::Files) if entry.is_dir() => return false,
            _ => {}
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&class_of(entry)) {
            return false;
        }
        if !self.exts.is_empty() {
            let ext = (!entry.is_dir())
                .then(|| entry.name.rsplit_once('.').map(|(_, e)| e.to_lowercase()))
                .flatten();
            if !ext.is_some_and(|e| self.exts.contains(&e)) {
                return false;
            }
        }
        // Sizes are only known for files: a size criterion keeps folders out.
        if self.min_size.is_some() || self.max_size.is_some() {
            if entry.is_dir() {
                return false;
            }
            if self.min_size.is_some_and(|min| entry.size < min)
                || self.max_size.is_some_and(|max| entry.size > max)
            {
                return false;
            }
        }
        if self.newer.is_some() || self.older.is_some() {
            let Some(age) = entry.modified.and_then(|m| now.duration_since(m).ok()) else {
                return false;
            };
            if self.newer.is_some_and(|d| age > d) || self.older.is_some_and(|d| age < d) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bluepane_vfs::EntryKind;
    use std::path::PathBuf;

    const DAY: Duration = Duration::from_secs(86_400);

    fn entry(name: &str, kind: EntryKind, size: u64, days_old: u64) -> Entry {
        Entry {
            name: name.into(),
            path: PathBuf::from(name),
            kind,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + DAY * 1000 - DAY * days_old as u32),
            created: None,
            hidden: false,
        }
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + DAY * 1000
    }

    #[test]
    fn criteria_are_taken_apart_and_the_rest_is_name() {
        let spec =
            FilterSpec::parse("Raport typ:dokument ext:pdf,.DOCX >1m <2g nowsze:7d pliki xyz:1");
        assert_eq!(spec.words, ["raport", "xyz:1"]);
        assert_eq!(spec.kinds, [FileClass::Document]);
        assert_eq!(spec.exts, ["pdf", "docx"]);
        assert_eq!(spec.min_size, Some(1024 * 1024));
        assert_eq!(spec.max_size, Some(2 * 1024 * 1024 * 1024));
        assert_eq!(spec.newer, Some(DAY * 7));
        assert_eq!(spec.only, Some(Only::Files));
        assert!(FilterSpec::parse("  ").is_empty());
        // An unknown type or a bad size is just text to look for in the name.
        assert_eq!(FilterSpec::parse("typ:xyz >abc").words, ["typ:xyz", ">abc"]);
    }

    #[test]
    fn every_part_must_hold() {
        let photo = entry("Wakacje.JPG", EntryKind::File, 3_000_000, 2);
        let old_doc = entry("raport.pdf", EntryKind::File, 50_000, 40);
        let folder = entry("zdjęcia", EntryKind::Dir, 0, 1);
        let at = now();
        let check = |text: &str, e: &Entry| FilterSpec::parse(text).matches(e, at);
        assert!(check("typ:obraz", &photo) && !check("typ:obraz", &old_doc));
        assert!(check("ext:jpg", &photo), "extensions ignore case");
        assert!(check(">1m", &photo) && !check(">1m", &old_doc));
        assert!(!check(">1m", &folder), "a size criterion keeps folders out");
        assert!(check("nowsze:7d", &photo) && !check("nowsze:7d", &old_doc));
        assert!(check("starsze:30d", &old_doc) && !check("starsze:30d", &photo));
        assert!(check("katalogi", &folder) && !check("katalogi", &photo));
        assert!(check("pliki", &photo) && !check("pliki", &folder));
        assert!(check("zdjecia", &folder), "names ignore Polish accents");
        assert!(check("wak typ:obraz nowsze:7d", &photo));
        assert!(!check("wak typ:dokument", &photo));
    }

    #[test]
    fn the_panel_shows_what_passes_and_the_footer_counts_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("l");
        std::fs::create_dir_all(dir.join("zdjęcia")).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        std::fs::write(dir.join("duży.bin"), vec![0u8; 2 * 1024 * 1024]).unwrap();
        let mut s = crate::Session::new(&dir, &dir);
        s.run_line("filter >1m").unwrap();
        let names: Vec<&str> = s.panels[0]
            .entries
            .iter()
            .filter(|e| !crate::is_parent_entry(e))
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["duży.bin"]);
        assert!(
            s.panel_summary(0).starts_with("Widoczne 1 z 3 · "),
            "{}",
            s.panel_summary(0)
        );
        s.run_line("filter").unwrap();
        assert!(!s.panel_summary(0).starts_with("Widoczne"));
    }
}
