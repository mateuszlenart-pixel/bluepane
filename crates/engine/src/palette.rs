//! The command palette (F1): a search box over every command with its Polish description.
//! The ranking and the list of recently used commands live here; the front end only draws the
//! rows and sends `palette ...` command lines.

use crate::command_help::{CommandHelp, COMMAND_HELP};
use crate::session::Session;
use crate::EngineError;

/// How many recently used commands are remembered.
const RECENT_MAX: usize = 12;
const RECENT_FILE: &str = "recent_commands.txt";

/// The open palette: what was typed and which row is highlighted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

/// One row of the palette.
#[derive(Debug, Clone, Copy)]
pub struct PaletteEntry {
    pub help: &'static CommandHelp,
    /// Used recently (the front end may mark it).
    pub recent: bool,
}

/// Lowercase without Polish diacritics, so `sciezka` finds `ścieżka`.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            c => c,
        })
        .collect()
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut hay = hay.chars();
    needle.chars().all(|n| hay.any(|h| h == n))
}

/// How well one query word fits a command (0 = not at all).
fn word_score(word: &str, help: &CommandHelp) -> u32 {
    let name = fold(help.name);
    if name == word {
        200
    } else if name.starts_with(word) {
        120
    } else if name.contains(word) {
        70
    } else if fold(help.description).contains(word) || fold(help.usage).contains(word) {
        30
    } else if fold(help.group).contains(word) {
        10
    } else if word.len() >= 2 && is_subsequence(word, &name) {
        5
    } else {
        0
    }
}

/// The commands that match `query`, best first. Without a query: the recent ones, then the
/// whole table in the order of the docs.
pub fn rank(query: &str, recent: &[String]) -> Vec<PaletteEntry> {
    let folded = fold(query);
    let words: Vec<&str> = folded.split_whitespace().collect();
    let recent_rank = |name: &str| recent.iter().position(|r| r == name);
    let mut scored: Vec<(u32, usize, PaletteEntry)> = Vec::new();
    for (index, help) in COMMAND_HELP.iter().enumerate() {
        let mut total = 0;
        let mut all = true;
        for word in &words {
            let score = word_score(word, help);
            if score == 0 {
                all = false;
                break;
            }
            total += score;
        }
        if !all {
            continue;
        }
        let rank = recent_rank(help.name);
        // Recent commands win ties and, with no query, come first.
        let bonus = rank.map_or(0, |r| 20 + (RECENT_MAX - r.min(RECENT_MAX)) as u32);
        scored.push((
            total + bonus,
            index,
            PaletteEntry {
                help,
                recent: rank.is_some(),
            },
        ));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, entry)| entry).collect()
}

/// True when the command cannot run without an argument typed by the user.
fn needs_argument(help: &CommandHelp) -> bool {
    help.usage
        .split_whitespace()
        .nth(1)
        .is_some_and(|word| !word.starts_with('['))
}

impl Session {
    /// The rows for the open palette (empty when it is closed).
    pub fn palette_entries(&self) -> Vec<PaletteEntry> {
        self.palette
            .as_ref()
            .map(|p| rank(&p.query, &self.recent_commands))
            .unwrap_or_default()
    }

    pub(crate) fn palette_open(&mut self, query: String) {
        self.palette = Some(Palette { query, selected: 0 });
    }

    pub(crate) fn palette_set_query(&mut self, query: String) {
        match &mut self.palette {
            Some(p) => {
                p.query = query;
                p.selected = 0;
            }
            None => self.palette_open(query),
        }
    }

    pub(crate) fn palette_move(&mut self, by: i32) {
        let count = self.palette_entries().len();
        if let Some(p) = &mut self.palette {
            let last = count.saturating_sub(1) as i64;
            p.selected = (p.selected as i64 + i64::from(by)).clamp(0, last) as usize;
        }
    }

    /// Runs the highlighted command. One that needs an argument is put in the command line
    /// instead (`command_prefill`), as a key bound to `"name "` does.
    pub(crate) fn palette_run(&mut self) -> Result<(), EngineError> {
        let entries = self.palette_entries();
        let Some(selected) = self.palette.as_ref().map(|p| p.selected) else {
            return Ok(());
        };
        let Some(entry) = entries.get(selected) else {
            self.status = "Nie ma takiej komendy".into();
            self.status_is_error = true;
            return Ok(());
        };
        let name = entry.help.name;
        let needs_argument = needs_argument(entry.help);
        self.palette = None;
        self.remember_command(name);
        if needs_argument {
            self.command_prefill = Some(format!("{name} "));
            Ok(())
        } else {
            self.run_line(name)
        }
    }

    fn remember_command(&mut self, name: &str) {
        self.recent_commands.retain(|r| r != name);
        self.recent_commands.insert(0, name.to_string());
        self.recent_commands.truncate(RECENT_MAX);
        if let Some(dir) = &self.config_dir {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(RECENT_FILE), self.recent_commands.join("\n"));
        }
    }

    pub(crate) fn load_recent_commands(&mut self, dir: &std::path::Path) {
        self.recent_commands = std::fs::read_to_string(dir.join(RECENT_FILE))
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| COMMAND_HELP.iter().any(|h| h.name == *l))
            .take(RECENT_MAX)
            .map(String::from)
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[PaletteEntry]) -> Vec<&'static str> {
        entries.iter().map(|e| e.help.name).collect()
    }

    #[test]
    fn an_empty_query_lists_everything_with_recent_commands_first() {
        let all = rank("", &[]);
        assert_eq!(all.len(), COMMAND_HELP.len());
        assert_eq!(all[0].help.name, COMMAND_HELP[0].name);
        let recent = vec!["queue".to_string(), "sync".to_string()];
        let ranked = rank("", &recent);
        assert_eq!(names(&ranked)[..2], ["queue", "sync"]);
        assert!(ranked[0].recent && !ranked[2].recent);
    }

    #[test]
    fn the_name_beats_the_description_and_every_word_must_match() {
        let found = rank("sort", &[]);
        assert_eq!(found[0].help.name, "sort");
        assert!(rank("sort nonexistentword", &[]).is_empty());
        let two = rank("kolejk wstrzym", &[]);
        assert_eq!(two[0].help.name, "queuepause");
    }

    #[test]
    fn polish_letters_are_matched_without_diacritics() {
        let found = rank("sciezk", &[]);
        assert!(names(&found).contains(&"cd"));
        assert!(!rank("ŚCIEŻK", &[]).is_empty());
    }

    #[test]
    fn a_recent_command_wins_a_tie() {
        let plain = rank("kolejk", &[]);
        let last = plain.last().unwrap().help.name.to_string();
        let boosted = rank("kolejk", std::slice::from_ref(&last));
        assert_eq!(boosted[0].help.name, last);
    }

    #[test]
    fn arguments_decide_between_running_and_filling_the_command_line() {
        let help = |n| crate::command_help(n).unwrap();
        assert!(needs_argument(help("cd")));
        assert!(needs_argument(help("selmask")));
        assert!(!needs_argument(help("queue")));
        assert!(!needs_argument(help("mkdir")));
        assert!(!needs_argument(help("filter")));
    }

    fn session(tmp: &std::path::Path) -> Session {
        std::fs::create_dir_all(tmp.join("left")).unwrap();
        std::fs::create_dir_all(tmp.join("right")).unwrap();
        let mut s = Session::new(tmp.join("left"), tmp.join("right"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn the_palette_opens_filters_moves_and_runs_a_command() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        assert!(s.palette.is_none() && s.palette_entries().is_empty());
        s.run_line("palette").unwrap();
        assert_eq!(s.palette_entries().len(), COMMAND_HELP.len());
        s.run_line("palette query kolejka").unwrap();
        assert_eq!(s.palette.as_ref().unwrap().query, "kolejka");
        s.run_line("palette move 1").unwrap();
        assert_eq!(s.palette.as_ref().unwrap().selected, 1);
        s.run_line("palette move -5").unwrap();
        assert_eq!(s.palette.as_ref().unwrap().selected, 0);
        s.run_line("palette query queue").unwrap();
        s.run_line("palette run").unwrap();
        assert!(s.palette.is_none());
        assert!(s.queue_open, "the chosen command ran");
        assert_eq!(s.recent_commands[0], "queue");
    }

    #[test]
    fn a_command_needing_an_argument_goes_to_the_command_line() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("palette selmask").unwrap();
        s.run_line("palette run").unwrap();
        assert_eq!(s.command_prefill.take().as_deref(), Some("selmask "));
        assert!(s.palette.is_none());
    }

    #[test]
    fn recent_commands_survive_a_restart_and_unknown_names_are_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("palette query sync").unwrap();
        s.run_line("palette run").unwrap();
        s.run_line("syncclose").unwrap();
        let config = tmp.path().join("config");
        std::fs::write(
            config.join(RECENT_FILE),
            format!(
                "{}\nno-such-command",
                std::fs::read_to_string(config.join(RECENT_FILE)).unwrap()
            ),
        )
        .unwrap();
        let mut again = session(tmp.path());
        again.use_config_dir(&config);
        assert_eq!(again.recent_commands, ["sync"]);
    }

    #[test]
    fn closing_and_running_with_no_match_are_harmless() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("palette query zzzzzz").unwrap();
        s.run_line("palette run").unwrap();
        assert!(s.palette.is_some(), "nothing was chosen, the palette stays");
        s.run_line("palette close").unwrap();
        assert!(s.palette.is_none());
        s.run_line("palette run").unwrap();
    }
}
