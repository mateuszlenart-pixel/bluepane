//! The key cheat sheet (`keys`, `?`, or holding Ctrl / Alt): every key of the current key
//! map, including the user's own `bind`s, with what it does, grouped like the command docs.

use crate::command_help::{command_help, COMMAND_HELP};
use crate::session::Session;

/// One line of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyHelp {
    pub group: &'static str,
    pub key: String,
    /// The command line the key runs.
    pub line: String,
    /// What it does, in a few words.
    pub description: String,
}

/// The short form of a command's description: up to the first `;`, without the key hint in
/// parentheses at the end (the key is shown next to it anyway).
fn short(description: &str) -> String {
    let first = description.split(';').next().unwrap_or(description).trim();
    match first.rfind(" (") {
        Some(at) if first.ends_with(')') => first[..at].to_string(),
        _ => first.to_string(),
    }
}

impl Session {
    /// The keys whose name starts with `prefix` (`"Ctrl+"`, `"Alt+"`; `""` for all), in the
    /// order of the command groups, then by key.
    pub fn key_sheet(&self, prefix: &str) -> Vec<KeyHelp> {
        let group_rank = |group: &str| {
            COMMAND_HELP
                .iter()
                .position(|h| h.group == group)
                .unwrap_or(usize::MAX)
        };
        let mut lines: Vec<KeyHelp> = self
            .keymap
            .bindings()
            .into_iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, line)| {
                let name = line.split_whitespace().next().unwrap_or("");
                let (group, description) = match command_help(name) {
                    Some(help) => (help.group, short(help.description)),
                    None => ("Inne", line.trim().to_string()),
                };
                let description = if line.ends_with(' ') {
                    format!("{description} (wpisz resztę w wierszu poleceń)")
                } else if line.contains(' ') {
                    format!("{description}: `{}`", line.trim())
                } else {
                    description
                };
                KeyHelp {
                    group,
                    key: key.to_string(),
                    line: line.to_string(),
                    description,
                }
            })
            .collect();
        lines.sort_by(|a, b| {
            group_rank(a.group)
                .cmp(&group_rank(b.group))
                .then_with(|| a.key.len().cmp(&b.key.len()))
                .then_with(|| a.key.cmp(&b.key))
        });
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("l")).unwrap();
        std::fs::create_dir_all(tmp.path().join("r")).unwrap();
        let mut s = Session::new(tmp.path().join("l"), tmp.path().join("r"));
        s.use_config_dir(&tmp.path().join("config"));
        (tmp, s)
    }

    #[test]
    fn every_key_is_listed_with_a_description_and_groups_follow_the_docs() {
        let (_tmp, s) = session();
        let all = s.key_sheet("");
        let f5 = all.iter().find(|k| k.key == "F5").unwrap();
        assert_eq!(f5.line, "copy");
        assert_eq!(f5.group, "Operacje na plikach");
        assert!(!f5.description.contains("(F5)"), "{}", f5.description);
        // Navigation comes before file operations, as in KOMENDY.md.
        let tab = all.iter().position(|k| k.key == "Tab").unwrap();
        let copy = all.iter().position(|k| k.key == "F5").unwrap();
        assert!(tab < copy);
        // A key that only starts a command says the rest is typed.
        let chmod = all.iter().find(|k| k.line == "chmod ").unwrap();
        assert!(
            chmod.description.contains("wpisz resztę"),
            "{}",
            chmod.description
        );
        // A key with arguments shows them.
        let down = all.iter().find(|k| k.key == "ArrowDown").unwrap();
        assert!(
            down.description.contains("`cursor 1`"),
            "{}",
            down.description
        );
    }

    #[test]
    fn a_prefix_keeps_only_those_keys_and_own_bindings_show_up() {
        let (_tmp, mut s) = session();
        let ctrl = s.key_sheet("Ctrl+");
        assert!(!ctrl.is_empty());
        assert!(ctrl.iter().all(|k| k.key.starts_with("Ctrl+")));
        assert!(ctrl.iter().any(|k| k.key == "Ctrl+P" && k.line == "jump"));
        s.run_line("bind Ctrl+Y queue").unwrap();
        assert!(s
            .key_sheet("Ctrl+")
            .iter()
            .any(|k| k.key == "Ctrl+Y" && k.line == "queue"));
        s.run_line("unbind Ctrl+P").unwrap();
        assert!(!s.key_sheet("Ctrl+").iter().any(|k| k.key == "Ctrl+P"));
    }

    #[test]
    fn keys_toggles_the_sheet_and_takes_a_modifier() {
        let (_tmp, mut s) = session();
        assert!(s.key_sheet_open.is_none());
        s.run_line("keys").unwrap();
        assert_eq!(s.key_sheet_open.as_deref(), Some(""));
        s.run_line("keys").unwrap();
        assert!(s.key_sheet_open.is_none());
        s.run_line("keys ctrl").unwrap();
        assert_eq!(s.key_sheet_open.as_deref(), Some("Ctrl+"));
        s.run_line("keys alt").unwrap();
        assert_eq!(s.key_sheet_open.as_deref(), Some("Alt+"));
        assert!(s.run_line("keys xyz").is_err());
    }
}
