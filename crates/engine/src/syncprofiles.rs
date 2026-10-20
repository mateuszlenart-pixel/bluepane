//! Saved synchronization profiles (`syncprofile`): a pair of folders with the settings of the
//! sync tool, kept as a macro named `sync-<name>` so that everything that runs macros runs a
//! profile too: `macro play`, `bluepane-cli run-script`, and the scheduler
//! (`schedule add sync-<name>; daily 03:00`).
//!
//! The file holds plain commands (`panel 1`, `cd ...`, `sync -twoway ...`, `syncrun`) and can
//! be read and edited like any macro. Run by hand, a profile stops after `sync` with the plan on
//! the screen, so nothing is copied or deleted before it was looked at; run from the scheduler
//! it ends with `syncrun` and does the work. Per-file direction changes are not saved.

use bluepane_sync::{CompareBy, Deletion, Variant};

use crate::session::{Session, SyncView};
use crate::EngineError;

const PREFIX: &str = "sync-";

/// What `syncprofile` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileAction {
    List,
    /// Save the folders and settings of the open sync tool.
    Save(String),
    /// Put the profile's folders in the panels and compare them; the plan waits on the screen.
    Run(String),
    Delete(String),
}

/// Text for a command line: `%` would start a macro parameter there.
fn escaped(text: &str) -> String {
    text.replace('%', "%%")
}

fn no_spaces(what: &str, text: &str) -> Result<(), EngineError> {
    if text.chars().any(char::is_whitespace) {
        Err(EngineError(format!(
            "profil nie zapisze {what} ze spacjami („{text}”): zapisz je bez spacji albo użyj `sync` w makrze"
        )))
    } else {
        Ok(())
    }
}

/// The commands that repeat a synchronization with the settings of `view`.
fn profile_lines(name: &str, view: &SyncView) -> Result<Vec<String>, EngineError> {
    let mut flags: Vec<String> = Vec::new();
    flags.push(
        match &view.settings.variant {
            Variant::TwoWay => "-twoway",
            Variant::Mirror => "-mirror",
            Variant::Update => "-update",
            Variant::Custom(_) => {
                return Err(EngineError(
                    "profil nie zapisze własnych reguł kierunków (wybierz dwukierunkową, lustro albo aktualizację)"
                        .into(),
                ))
            }
        }
        .into(),
    );
    match &view.settings.compare {
        CompareBy::TimeAndSize { tolerance } => {
            flags.push("-time".into());
            flags.push(format!("-tol={}", tolerance.as_secs()));
        }
        CompareBy::Size => flags.push("-size".into()),
        CompareBy::Content => flags.push("-content".into()),
    }
    match &view.settings.deletion {
        Deletion::Trash => flags.push("-delete=trash".into()),
        Deletion::Permanent => flags.push("-delete=permanent".into()),
        Deletion::Versioning(folder) => {
            let folder = folder.display().to_string();
            no_spaces("katalogu wersji", &folder)?;
            flags.push(format!("-delete=versions:{}", escaped(&folder)));
        }
    }
    if !view.include.trim().is_empty() {
        no_spaces("wzorców „Tylko”", view.include.trim())?;
        flags.push(format!("-i{}", escaped(view.include.trim())));
    }
    if !view.exclude.trim().is_empty() {
        no_spaces("wzorców „Pomiń”", view.exclude.trim())?;
        flags.push(format!("-x{}", escaped(view.exclude.trim())));
    }
    Ok(vec![
        format!("# Profil synchronizacji bluepane „{name}”"),
        "panel 2".into(),
        format!("cd {}", escaped(&view.right.display().to_string())),
        "panel 1".into(),
        format!("cd {}", escaped(&view.left.display().to_string())),
        format!("sync {}", flags.join(" ")),
        "syncrun".into(),
    ])
}

impl Session {
    /// The names of the saved profiles, sorted.
    pub fn sync_profiles(&self) -> Vec<String> {
        self.saved_macros()
            .into_iter()
            .filter_map(|m| m.strip_prefix(PREFIX).map(str::to_string))
            .collect()
    }

    fn profile_macro(&self, name: &str) -> Result<String, EngineError> {
        let name = self.macro_name(name)?;
        Ok(format!("{PREFIX}{name}"))
    }

    pub(crate) fn profile_action(&mut self, action: ProfileAction) -> Result<(), EngineError> {
        match action {
            ProfileAction::List => {
                let names = self.sync_profiles();
                self.status = format!(
                    "Profile synchronizacji: {} (syncprofile save|run|delete <nazwa>)",
                    if names.is_empty() {
                        "brak".to_string()
                    } else {
                        names.join(", ")
                    }
                );
            }
            ProfileAction::Save(name) => {
                let macro_name = self.profile_macro(&name)?;
                let name = name.trim().to_string();
                let view = self.sync.as_ref().ok_or_else(|| {
                    EngineError("najpierw otwórz synchronizację (sync) i porównaj katalogi".into())
                })?;
                let lines = profile_lines(&name, view)?;
                let path = self.macro_path(&macro_name)?;
                let existed = path.exists();
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| EngineError(format!("nie można zapisać profilu: {e}")))?;
                }
                std::fs::write(&path, lines.join("\n") + "\n")
                    .map_err(|e| EngineError(format!("nie można zapisać profilu: {e}")))?;
                self.status = format!(
                    "{} profil „{name}” (z harmonogramu: schedule add {macro_name}; daily 03:00)",
                    if existed {
                        "Zaktualizowano"
                    } else {
                        "Zapisano"
                    }
                );
            }
            ProfileAction::Run(name) => {
                let macro_name = self.profile_macro(&name)?;
                let text = std::fs::read_to_string(self.macro_path(&macro_name)?)
                    .map_err(|_| EngineError(format!("nie ma profilu „{}”", name.trim())))?;
                // By hand a profile only compares: the plan is looked at before `syncrun`.
                let mut lines = Self::script_commands(&text);
                if lines.last().is_some_and(|l| l == "syncrun") {
                    lines.pop();
                }
                self.play_script(&lines.join("\n"), &[])?;
                self.status = format!(
                    "Profil „{}”: plan czeka na ekranie (Synchronizuj albo syncrun)",
                    name.trim()
                );
            }
            ProfileAction::Delete(name) => {
                let macro_name = self.profile_macro(&name)?;
                std::fs::remove_file(self.macro_path(&macro_name)?)
                    .map_err(|_| EngineError(format!("nie ma profilu „{}”", name.trim())))?;
                self.status = format!(
                    "Usunięto profil „{}” (wpis w harmonogramie: schedule delete {macro_name})",
                    name.trim()
                );
            }
        }
        Ok(())
    }
}

/// `syncprofile [list|save <nazwa>|run <nazwa>|delete <nazwa>]`
pub(crate) fn parse_syncprofile(rest: &str) -> Result<ProfileAction, EngineError> {
    let rest = rest.trim();
    let (verb, tail) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(v, t)| (v, t.trim()));
    let named = |make: fn(String) -> ProfileAction| {
        if tail.is_empty() {
            Err(EngineError(format!("syncprofile {verb}: podaj nazwę")))
        } else {
            Ok(make(tail.to_string()))
        }
    };
    match verb.to_ascii_lowercase().as_str() {
        "" | "list" => Ok(ProfileAction::List),
        "save" => named(ProfileAction::Save),
        "run" | "load" => named(ProfileAction::Run),
        "delete" => named(ProfileAction::Delete),
        other => Err(EngineError(format!(
            "syncprofile: nieznane „{other}” (list, save, run, delete)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("źródło 100%");
        let right = tmp.path().join("kopia");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        let mut s = Session::new(&left, &right);
        s.use_config_dir(&tmp.path().join("config"));
        (tmp, s)
    }

    fn idle(s: &mut Session) {
        s.wait_idle();
    }

    #[test]
    fn a_profile_is_saved_as_a_macro_and_run_restores_folders_and_settings() {
        let (tmp, mut s) = session();
        let (left, right) = (tmp.path().join("źródło 100%"), tmp.path().join("kopia"));
        fs::write(left.join("zażółć.txt"), "gęślą").unwrap();
        s.run_line("sync -mirror -size -delete=permanent -i*.txt -x*.tmp")
            .unwrap();
        idle(&mut s);
        s.run_line("syncprofile save Kopia dzienna").unwrap();
        assert_eq!(s.sync_profiles(), ["Kopia dzienna"]);
        assert!(s.saved_macros().contains(&"sync-Kopia dzienna".to_string()));
        let text =
            fs::read_to_string(tmp.path().join("config/macros/sync-Kopia dzienna.bp")).unwrap();
        assert!(
            text.contains("sync -mirror -size -delete=permanent -i*.txt -x*.tmp"),
            "{text}"
        );
        assert!(text.contains("%%"), "a percent sign is escaped: {text}");

        // Somewhere else, with the tool closed: running puts everything back.
        s.run_line("syncclose").unwrap();
        s.run_line("cd /").unwrap();
        s.run_line("syncprofile run Kopia dzienna").unwrap();
        idle(&mut s);
        let view = s.sync.as_ref().expect("the sync tool is open");
        assert_eq!((&view.left, &view.right), (&left, &right));
        assert_eq!(view.settings.variant, Variant::Mirror);
        assert_eq!(view.settings.compare, CompareBy::Size);
        assert_eq!(view.settings.deletion, Deletion::Permanent);
        assert_eq!(
            (view.include.as_str(), view.exclude.as_str()),
            ("*.txt", "*.tmp")
        );
        // By hand nothing was copied: the plan waits.
        assert!(!right.join("zażółć.txt").exists());
        assert_eq!(s.active, 0);

        // The file itself ends with `syncrun`, which the scheduler's run carries out.
        assert!(text.trim_end().ends_with("syncrun"));
        s.run_line("syncprofile delete Kopia dzienna").unwrap();
        assert!(s.sync_profiles().is_empty());
        assert!(s.run_line("syncprofile run Kopia dzienna").is_err());
    }

    #[test]
    fn saving_needs_the_tool_and_refuses_what_a_line_cannot_hold() {
        let (_tmp, mut s) = session();
        assert!(s.run_line("syncprofile save X").is_err(), "no sync open");
        s.run_line("sync -i*.txt").unwrap();
        idle(&mut s);
        s.run_line("syncprofile save X").unwrap();
        assert!(s.run_line("syncprofile save ../zle").is_err(), "bad name");
        // Patterns with spaces cannot survive a command line.
        s.run_line("syncclose").unwrap();
        s.run_line("sync").unwrap();
        idle(&mut s);
        s.sync.as_mut().unwrap().include = "moje pliki*".into();
        assert!(s.run_line("syncprofile save Y").is_err());
        assert!(parse_syncprofile("save").is_err());
        assert_eq!(parse_syncprofile("").unwrap(), ProfileAction::List);
    }
}
