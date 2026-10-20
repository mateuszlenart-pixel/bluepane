//! The one search box (Ctrl+K): commands, places and the files of the current folder in a single
//! list. `>` restricts it to commands, `?` starts the deep search (`go ?pattern`, which can be
//! cancelled). Nothing is scanned in the background: the rows come from the command table, the
//! visit history and the folder already listed in the panel.

use std::path::PathBuf;

use crate::jump::unix_now;
use crate::palette::{fold, rank, Palette};
use crate::session::Session;
use crate::EngineError;

/// Rows per group while typing.
const COMMANDS_SHOWN: usize = 5;
const PLACES_SHOWN: usize = 8;
const FILES_SHOWN: usize = 10;
/// Rows of recent commands and visited folders offered before anything is typed.
const EMPTY_COMMANDS: usize = 3;
const EMPTY_PLACES: usize = 5;

/// What Enter does with a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxTarget {
    /// Run this command line.
    Line(String),
    /// Put this text in the command line (the command needs an argument).
    Prefill(String),
    /// Put the cursor of the active panel on this file or folder.
    File(PathBuf),
}

/// One row of the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxEntry {
    /// "Komenda", "Przycisk", "Katalog", "Zakładka", "Serwer", "Plik", "Folder" or "Szukaj".
    pub kind: &'static str,
    pub label: String,
    pub target: BoxTarget,
}

fn needs_argument(usage: &str) -> bool {
    usage
        .split_whitespace()
        .nth(1)
        .is_some_and(|word| !word.starts_with('['))
}

impl Session {
    /// The rows for the open box (empty when it is closed).
    pub fn box_entries(&self) -> Vec<BoxEntry> {
        let Some(open) = &self.quickbox else {
            return Vec::new();
        };
        self.box_rows(&open.query, unix_now())
    }

    pub(crate) fn box_rows(&self, query: &str, now: u64) -> Vec<BoxEntry> {
        let query = query.trim_start();
        if let Some(pattern) = query.strip_prefix('?') {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                return Vec::new();
            }
            return vec![BoxEntry {
                kind: "Szukaj",
                label: format!(
                    "{pattern}  (w {}; Enter szuka w tle, można anulować)",
                    self.active_panel().location().display()
                ),
                target: BoxTarget::Line(format!("go ?{pattern}")),
            }];
        }
        let (only_commands, text) = match query.strip_prefix('>') {
            Some(rest) => (true, rest.trim()),
            None => (false, query.trim()),
        };
        let empty = text.is_empty();
        let mut out = Vec::new();

        let commands = rank(text, &self.recent_commands);
        let command_limit = match (only_commands, empty) {
            (true, _) => usize::MAX,
            (false, true) => EMPTY_COMMANDS,
            (false, false) => COMMANDS_SHOWN,
        };
        out.extend(
            commands
                .iter()
                .filter(|e| !(empty && !only_commands) || e.recent)
                .take(command_limit)
                .map(|e| BoxEntry {
                    kind: "Komenda",
                    label: format!(
                        "{}  {}",
                        e.help.usage,
                        e.help.description.split(';').next().unwrap_or("")
                    ),
                    target: if needs_argument(e.help.usage) {
                        BoxTarget::Prefill(format!("{} ", e.help.name))
                    } else {
                        BoxTarget::Line(e.help.name.to_string())
                    },
                }),
        );
        if only_commands {
            return out;
        }

        if !empty {
            let folded = fold(text);
            out.extend(
                self.buttons
                    .iter()
                    .filter(|b| fold(&b.name).contains(&folded))
                    .take(COMMANDS_SHOWN)
                    .map(|b| BoxEntry {
                        kind: "Przycisk",
                        label: format!("{}  ({})", b.name, b.line),
                        target: BoxTarget::Line(format!("button run {}", b.name)),
                    }),
            );
        }

        let place_limit = if empty { EMPTY_PLACES } else { PLACES_SHOWN };
        out.extend(
            self.place_rows(text, now)
                .into_iter()
                .take(place_limit)
                .map(|p| BoxEntry {
                    kind: p.kind,
                    label: p.label,
                    target: BoxTarget::Line(p.line),
                }),
        );

        if !empty {
            let folded = fold(text);
            let words: Vec<&str> = folded.split_whitespace().collect();
            out.extend(
                self.active_panel()
                    .entries
                    .iter()
                    .filter(|e| {
                        e.name != ".." && {
                            let name = fold(&e.name);
                            words.iter().all(|w| name.contains(w))
                        }
                    })
                    .take(FILES_SHOWN)
                    .map(|e| BoxEntry {
                        kind: if e.is_dir() { "Folder" } else { "Plik" },
                        label: e.name.clone(),
                        target: BoxTarget::File(e.path.clone()),
                    }),
            );
        }
        out
    }

    pub(crate) fn box_open(&mut self, query: String) {
        self.palette = None;
        self.jump = None;
        self.quickbox = Some(Palette { query, selected: 0 });
    }

    pub(crate) fn box_set_query(&mut self, query: String) {
        match &mut self.quickbox {
            Some(open) => {
                open.query = query;
                open.selected = 0;
            }
            None => self.box_open(query),
        }
    }

    pub(crate) fn box_move(&mut self, by: i32) {
        let count = self.box_entries().len();
        if let Some(open) = &mut self.quickbox {
            let last = count.saturating_sub(1) as i64;
            open.selected = (open.selected as i64 + i64::from(by)).clamp(0, last) as usize;
        }
    }

    pub(crate) fn box_run(&mut self) -> Result<(), EngineError> {
        let entries = self.box_entries();
        let Some(selected) = self.quickbox.as_ref().map(|b| b.selected) else {
            return Ok(());
        };
        let Some(entry) = entries.get(selected) else {
            self.status = "Nic nie pasuje".into();
            self.status_is_error = true;
            return Ok(());
        };
        let target = entry.target.clone();
        self.quickbox = None;
        match target {
            BoxTarget::Line(line) => {
                // Commands picked here count as recent, like in the palette.
                if let Some(help) = crate::command_help::COMMAND_HELP
                    .iter()
                    .find(|h| h.name == line)
                {
                    self.remember_command(help.name);
                }
                self.run_line(&line)
            }
            BoxTarget::Prefill(text) => {
                self.remember_command(text.trim());
                self.command_prefill = Some(text);
                Ok(())
            }
            BoxTarget::File(path) => {
                self.active_panel_mut().put_cursor_on(&path);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Zażółć.txt"), "x").unwrap();
        std::fs::write(tmp.path().join("raport.pdf"), "x").unwrap();
        std::fs::create_dir(tmp.path().join("Raporty")).unwrap();
        let s = Session::new(tmp.path().to_path_buf(), tmp.path().to_path_buf());
        (tmp, s)
    }

    fn kinds(entries: &[BoxEntry]) -> Vec<&'static str> {
        entries.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn a_word_finds_commands_and_files_of_the_folder_in_one_list() {
        let (_tmp, mut s) = session();
        s.run_line("box rapor").unwrap();
        let rows = s.box_entries();
        let labels: Vec<&str> = rows.iter().map(|e| e.label.as_str()).collect();
        assert!(
            labels.contains(&"raport.pdf") && labels.contains(&"Raporty"),
            "{labels:?}"
        );
        assert!(kinds(&rows).contains(&"Plik") && kinds(&rows).contains(&"Folder"));
        // Commands come before files.
        let first_file = rows.iter().position(|e| e.kind == "Plik").unwrap();
        assert!(rows[..first_file].iter().all(|e| e.kind != "Plik"));
    }

    #[test]
    fn polish_letters_can_be_left_out_but_the_name_is_kept() {
        let (_tmp, mut s) = session();
        s.run_line("box zazolc").unwrap();
        let rows = s.box_entries();
        assert_eq!(rows.iter().filter(|e| e.kind == "Plik").count(), 1);
        assert_eq!(
            rows.iter().find(|e| e.kind == "Plik").unwrap().label,
            "Zażółć.txt"
        );
    }

    #[test]
    fn greater_than_lists_only_commands_and_question_mark_offers_the_deep_search() {
        let (_tmp, mut s) = session();
        s.run_line("box >sort").unwrap();
        assert!(s.box_entries().iter().all(|e| e.kind == "Komenda"));
        s.run_line("box ?raport").unwrap();
        let rows = s.box_entries();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "Szukaj");
        assert_eq!(rows[0].target, BoxTarget::Line("go ?raport".into()));
        s.run_line("box ?").unwrap();
        assert!(s.box_entries().is_empty());
    }

    #[test]
    fn enter_on_a_file_puts_the_cursor_there_and_a_command_runs() {
        let (tmp, mut s) = session();
        s.run_line("box raport.pdf").unwrap();
        let at = s
            .box_entries()
            .iter()
            .position(|e| e.kind == "Plik")
            .unwrap();
        s.run_line(&format!("box move {at}")).unwrap();
        s.run_line("box run").unwrap();
        assert!(s.quickbox.is_none());
        assert_eq!(
            s.active_panel().current().unwrap().path,
            tmp.path().join("raport.pdf")
        );

        s.run_line("box >queue").unwrap();
        s.run_line("box run").unwrap();
        assert!(s.queue_open);
    }

    #[test]
    fn a_command_with_an_argument_goes_to_the_command_line() {
        let (_tmp, mut s) = session();
        s.run_line("box >findset").unwrap();
        s.run_line("box run").unwrap();
        assert!(s
            .command_prefill
            .as_deref()
            .is_some_and(|t| t.starts_with("findset")));
    }

    #[test]
    fn nothing_matching_says_so_and_close_closes() {
        let (_tmp, mut s) = session();
        s.run_line("box qqqqzzzz").unwrap();
        assert!(s.box_entries().is_empty());
        s.run_line("box run").unwrap();
        assert!(s.quickbox.is_some(), "nothing was chosen, the box stays");
        s.run_line("box close").unwrap();
        assert!(s.quickbox.is_none());
    }
}
