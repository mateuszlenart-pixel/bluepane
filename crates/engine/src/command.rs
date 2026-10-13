use std::path::PathBuf;

use bluepane_ops::Resolution;

use crate::{EngineError, SortKey};

/// Names accepted at the command line (aliases excluded).
pub const COMMAND_NAMES: &[&str] = &[
    "switch",
    "up",
    "enter",
    "cursor",
    "toggle",
    "select",
    "cd",
    "back",
    "forward",
    "volume",
    "filter",
    "refresh",
    "copy",
    "move",
    "mkdir",
    "delete",
    "confirm",
    "cancel",
    "selmask",
    "unselmask",
    "selectall",
    "unselectall",
    "invert",
    "sort",
];

/// One user action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Move focus to the other panel (Tab).
    SwitchPanel,
    /// Go to the parent directory (Backspace). With a quick filter active, removes its
    /// last character instead.
    Up,
    /// Enter the directory under the cursor (Enter).
    Enter,
    /// Move the cursor by a number of rows (arrows).
    Cursor(i32),
    /// Toggle selection of the entry under the cursor and move down (Space, Insert).
    ToggleSelect,
    /// Put the cursor on the entry with this name and select it.
    Select(String),
    /// Change directory in the active panel.
    Cd(PathBuf),
    /// Previous directory in the active panel's history (Alt+Left).
    Back,
    /// Next directory in the active panel's history (Alt+Right).
    Forward,
    /// Jump the active panel to a volume, given as a 1-based number or a name (Alt+1..9).
    Volume(String),
    /// Set the quick filter of the active panel; empty text clears it (Escape).
    Filter(String),
    Refresh,
    /// Copy the selection (or the entry under the cursor) to the other panel (F5).
    /// When both panels show the same directory this proposes duplicates instead.
    Copy,
    /// Select the visible entries matching a mask such as `*.txt;*.md` (Ctrl+M).
    SelectMask(String),
    /// Unselect the visible entries matching a mask (Ctrl+Shift+M).
    UnselectMask(String),
    /// Select every visible entry (Ctrl+A).
    SelectAll,
    /// Clear the selection (Ctrl+Shift+A).
    UnselectAll,
    /// Invert the selection of the visible entries (Ctrl+I).
    InvertSelection,
    /// Order the active panel; `None` direction flips on a repeated key (Ctrl+F3..F6).
    Sort(SortKey, Option<bool>),
    /// Accept the pending proposal (Enter while a question is shown).
    Confirm,
    /// Reject the pending proposal (Escape while a question is shown).
    Cancel,
    /// Move the selection to the other panel (F6).
    Move,
    /// Create a directory in the active panel (F7). Without a name a dialog asks for it.
    Mkdir(Option<String>),
    /// Rename the entry under the cursor (F2). Without a name a dialog asks for it.
    Rename(Option<String>),
    /// Move the selection to the trash (F8), after a confirmation.
    Delete,
    /// Answer a file conflict; the flag applies the answer to every later conflict of the job.
    Resolve(Resolution, bool),
    /// Cancel the running job and drop the queued ones.
    StopJob,
}

impl Command {
    /// Parses a command line such as `copy`, `mkdir Reports` or `cd /tmp`.
    pub fn parse(line: &str) -> Result<Command, EngineError> {
        let line = line.trim();
        let (head, rest) = match line.split_once(char::is_whitespace) {
            Some((head, rest)) => (head, rest.trim()),
            None => (line, ""),
        };
        let need_arg = |what: &str| {
            if rest.is_empty() {
                Err(EngineError(format!("{head}: brak argumentu ({what})")))
            } else {
                Ok(rest.to_string())
            }
        };
        match head.to_ascii_lowercase().as_str() {
            "" => Err(EngineError("pusta komenda".into())),
            "switch" => Ok(Command::SwitchPanel),
            "up" => Ok(Command::Up),
            "enter" => Ok(Command::Enter),
            "toggle" => Ok(Command::ToggleSelect),
            "selmask" => need_arg("maska, np. *.txt").map(Command::SelectMask),
            "unselmask" => need_arg("maska, np. *.txt").map(Command::UnselectMask),
            "selectall" => Ok(Command::SelectAll),
            "unselectall" => Ok(Command::UnselectAll),
            "invert" => Ok(Command::InvertSelection),
            "sort" => parse_sort(rest),
            "confirm" | "yes" => Ok(Command::Confirm),
            "cancel" | "no" => Ok(Command::Cancel),
            "back" => Ok(Command::Back),
            "forward" => Ok(Command::Forward),
            "refresh" => Ok(Command::Refresh),
            "filter" => Ok(Command::Filter(rest.to_string())),
            "volume" | "drive" => need_arg("numer lub nazwa").map(Command::Volume),
            "copy" | "cp" => Ok(Command::Copy),
            "move" | "mv" => Ok(Command::Move),
            "delete" | "rm" => Ok(Command::Delete),
            "cursor" => rest
                .parse::<i32>()
                .map(Command::Cursor)
                .map_err(|_| EngineError("cursor: oczekiwano liczby, np. cursor -1".into())),
            "mkdir" => Ok(Command::Mkdir(optional(rest))),
            "rename" => Ok(Command::Rename(optional(rest))),
            "stop" => Ok(Command::StopJob),
            "select" => need_arg("nazwa").map(Command::Select),
            "cd" => need_arg("ścieżka").map(|p| Command::Cd(PathBuf::from(p))),
            other => parse_resolution(other)
                .map(|(resolution, all)| Command::Resolve(resolution, all))
                .ok_or_else(|| EngineError(format!("nieznana komenda: {other}"))),
        }
    }
}

fn optional(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

/// `overwrite`, `newer`, `skip`, `keepboth`, each optionally followed by `-all`.
fn parse_resolution(word: &str) -> Option<(Resolution, bool)> {
    let (base, all) = match word.strip_suffix("-all") {
        Some(base) => (base, true),
        None => (word, false),
    };
    let resolution = match base {
        "overwrite" => Resolution::Overwrite,
        "newer" => Resolution::OverwriteOlder,
        "skip" => Resolution::Skip,
        "keepboth" => Resolution::KeepBoth,
        _ => return None,
    };
    Some((resolution, all))
}

fn parse_sort(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("sort: użycie: sort <name|ext|size|date> [asc|desc]".into());
    let mut words = args.split_whitespace();
    let key = words.next().and_then(SortKey::parse).ok_or_else(usage)?;
    let descending = match words.next().map(str::to_ascii_lowercase).as_deref() {
        None => None,
        Some("asc") => Some(false),
        Some("desc") => Some(true),
        Some(_) => return Err(usage()),
    };
    if words.next().is_some() {
        return Err(usage());
    }
    Ok(Command::Sort(key, descending))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_commands_and_aliases() {
        assert_eq!(Command::parse("copy"), Ok(Command::Copy));
        assert_eq!(Command::parse("  CP "), Ok(Command::Copy));
        assert_eq!(Command::parse("rm"), Ok(Command::Delete));
        assert_eq!(Command::parse("cursor -3"), Ok(Command::Cursor(-3)));
    }

    #[test]
    fn parses_commands_with_arguments() {
        assert_eq!(
            Command::parse("mkdir New folder"),
            Ok(Command::Mkdir(Some("New folder".into())))
        );
        assert_eq!(
            Command::parse("cd /tmp"),
            Ok(Command::Cd(PathBuf::from("/tmp")))
        );
        assert_eq!(Command::parse("volume 2"), Ok(Command::Volume("2".into())));
        assert_eq!(
            Command::parse("filter my file"),
            Ok(Command::Filter("my file".into()))
        );
    }

    #[test]
    fn parses_selection_and_sort_commands() {
        assert_eq!(
            Command::parse("selmask *.txt;*.md"),
            Ok(Command::SelectMask("*.txt;*.md".into()))
        );
        assert_eq!(Command::parse("selectall"), Ok(Command::SelectAll));
        assert_eq!(Command::parse("invert"), Ok(Command::InvertSelection));
        assert_eq!(
            Command::parse("sort size"),
            Ok(Command::Sort(SortKey::Size, None))
        );
        assert_eq!(
            Command::parse("sort DATE desc"),
            Ok(Command::Sort(SortKey::Date, Some(true)))
        );
        assert!(Command::parse("selmask").is_err());
        assert!(Command::parse("sort").is_err());
        assert!(Command::parse("sort color").is_err());
        assert!(Command::parse("sort name up").is_err());
        assert!(Command::parse("sort name asc extra").is_err());
    }

    #[test]
    fn mkdir_and_rename_take_an_optional_name() {
        assert_eq!(Command::parse("mkdir"), Ok(Command::Mkdir(None)));
        assert_eq!(
            Command::parse("mkdir New folder"),
            Ok(Command::Mkdir(Some("New folder".into())))
        );
        assert_eq!(Command::parse("rename"), Ok(Command::Rename(None)));
        assert_eq!(
            Command::parse("rename b.txt"),
            Ok(Command::Rename(Some("b.txt".into())))
        );
    }

    #[test]
    fn parses_conflict_answers() {
        assert_eq!(
            Command::parse("overwrite"),
            Ok(Command::Resolve(Resolution::Overwrite, false))
        );
        assert_eq!(
            Command::parse("newer-all"),
            Ok(Command::Resolve(Resolution::OverwriteOlder, true))
        );
        assert_eq!(
            Command::parse("SKIP"),
            Ok(Command::Resolve(Resolution::Skip, false))
        );
        assert_eq!(
            Command::parse("keepboth-all"),
            Ok(Command::Resolve(Resolution::KeepBoth, true))
        );
        assert_eq!(Command::parse("stop"), Ok(Command::StopJob));
        assert!(Command::parse("skip-everything").is_err());
    }

    #[test]
    fn parses_confirm_and_cancel() {
        assert_eq!(Command::parse("confirm"), Ok(Command::Confirm));
        assert_eq!(Command::parse("yes"), Ok(Command::Confirm));
        assert_eq!(Command::parse("cancel"), Ok(Command::Cancel));
    }

    #[test]
    fn parses_history_and_filter_clearing() {
        assert_eq!(Command::parse("back"), Ok(Command::Back));
        assert_eq!(Command::parse("forward"), Ok(Command::Forward));
        assert_eq!(Command::parse("filter"), Ok(Command::Filter(String::new())));
        assert!(Command::parse("volume").is_err());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Command::parse("").is_err());
        assert!(Command::parse("cursor x").is_err());
        assert!(Command::parse("frobnicate").is_err());
    }

    #[test]
    fn every_listed_name_parses_or_needs_an_argument() {
        for name in COMMAND_NAMES {
            match Command::parse(name) {
                Ok(_) => {}
                Err(e) => assert!(!e.0.contains("nieznana"), "{name} is not recognised"),
            }
        }
    }
}
