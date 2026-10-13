use std::path::PathBuf;

use crate::EngineError;

/// Names accepted at the command line (aliases excluded).
pub const COMMAND_NAMES: &[&str] = &[
    "switch", "up", "enter", "cursor", "toggle", "select", "cd", "back", "forward", "volume",
    "filter", "refresh", "copy", "move", "mkdir", "delete",
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
    Copy,
    /// Move the selection to the other panel (F6).
    Move,
    /// Create a directory in the active panel (F7).
    Mkdir(String),
    /// Move the selection to the trash (F8).
    Delete,
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
            "mkdir" => need_arg("nazwa katalogu").map(Command::Mkdir),
            "select" => need_arg("nazwa").map(Command::Select),
            "cd" => need_arg("ścieżka").map(|p| Command::Cd(PathBuf::from(p))),
            other => Err(EngineError(format!("nieznana komenda: {other}"))),
        }
    }
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
            Ok(Command::Mkdir("New folder".into()))
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
    fn parses_history_and_filter_clearing() {
        assert_eq!(Command::parse("back"), Ok(Command::Back));
        assert_eq!(Command::parse("forward"), Ok(Command::Forward));
        assert_eq!(Command::parse("filter"), Ok(Command::Filter(String::new())));
        assert!(Command::parse("volume").is_err());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Command::parse("").is_err());
        assert!(Command::parse("mkdir").is_err());
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
