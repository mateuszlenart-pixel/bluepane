//! Scheduled macros: `schedule add Porządki; daily 07:30` makes the system run the macro with
//! `bluepane-cli run-script` at that time (an entry in the user's crontab on Linux, a task in
//! the `bluepane` folder of the Task Scheduler on Windows). bluepane lists and removes only
//! the entries it made. The run uses the two folders the panels show when the entry is made, and
//! the macro file itself: edit it later and the next run does the new thing.
//!
//! Times: `hourly` (every full hour), `daily HH:MM`, `weekly <day> HH:MM` with the day as
//! `pn wt sr cz pt so nd` or `mon tue wed thu fri sat sun`.

use std::path::{Path, PathBuf};

use bluepane_vfs::ScheduleWhen;

use crate::session::Session;
use crate::EngineError;

/// What `schedule` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleAction {
    List,
    Add(String, ScheduleWhen),
    Delete(String),
}

fn day_number(word: &str) -> Option<u8> {
    Some(match word.to_lowercase().as_str() {
        "nd" | "sun" | "niedziela" | "sunday" => 0,
        "pn" | "mon" | "poniedzialek" | "poniedziałek" | "monday" => 1,
        "wt" | "tue" | "wtorek" | "tuesday" => 2,
        "sr" | "śr" | "wed" | "sroda" | "środa" | "wednesday" => 3,
        "cz" | "thu" | "czwartek" | "thursday" => 4,
        "pt" | "fri" | "piatek" | "piątek" | "friday" => 5,
        "so" | "sat" | "sobota" | "saturday" => 6,
        _ => return None,
    })
}

fn clock(word: &str) -> Result<(u8, u8), EngineError> {
    let bad = || EngineError(format!("godzina „{word}” ma mieć postać GG:MM (np. 07:30)"));
    let (h, m) = word.split_once(':').ok_or_else(bad)?;
    let (h, m): (u8, u8) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if h < 24 && m < 60 {
        Ok((h, m))
    } else {
        Err(bad())
    }
}

pub(crate) fn parse_when(text: &str) -> Result<ScheduleWhen, EngineError> {
    let words: Vec<&str> = text.split_whitespace().collect();
    match words.as_slice() {
        ["hourly"] => Ok(ScheduleWhen::Hourly),
        ["daily", time] => {
            let (hour, minute) = clock(time)?;
            Ok(ScheduleWhen::Daily { hour, minute })
        }
        ["weekly", day, time] => {
            let day = day_number(day).ok_or_else(|| {
                EngineError(format!("nieznany dzień „{day}” (pn wt sr cz pt so nd)"))
            })?;
            let (hour, minute) = clock(time)?;
            Ok(ScheduleWhen::Weekly { day, hour, minute })
        }
        _ => Err(EngineError(
            "pora: hourly, daily GG:MM albo weekly <dzień> GG:MM".into(),
        )),
    }
}

/// `schedule [list|add <makro>; <pora>|delete <makro>]`
pub(crate) fn parse_schedule(rest: &str) -> Result<ScheduleAction, EngineError> {
    let rest = rest.trim();
    let (verb, tail) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(v, t)| (v, t.trim()));
    Ok(match verb.to_ascii_lowercase().as_str() {
        "" | "list" => ScheduleAction::List,
        "add" => {
            let (name, when) = tail.split_once(';').ok_or_else(|| {
                EngineError(
                    "schedule add <makro>; hourly | daily GG:MM | weekly <dzień> GG:MM".into(),
                )
            })?;
            ScheduleAction::Add(name.trim().to_string(), parse_when(when)?)
        }
        "delete" => ScheduleAction::Delete(tail.to_string()),
        other => {
            return Err(EngineError(format!(
                "schedule: nieznane „{other}” (list, add, delete)"
            )))
        }
    })
}

/// `bluepane-cli` in the folder of the running program.
pub(crate) fn cli_next_to(exe: &Path) -> Option<PathBuf> {
    let cli = exe
        .parent()?
        .join(format!("bluepane-cli{}", std::env::consts::EXE_SUFFIX));
    cli.is_file().then_some(cli)
}

impl Session {
    /// The program and arguments the system runs for a scheduled macro.
    pub(crate) fn schedule_command(
        &self,
        name: &str,
        exe: &Path,
    ) -> Result<(PathBuf, Vec<String>), EngineError> {
        let script = self.macro_path(name)?;
        if !script.is_file() {
            return Err(EngineError(format!("nie ma makra „{name}”")));
        }
        let cli = cli_next_to(exe).ok_or_else(|| {
            EngineError(
                "nie znaleziono bluepane-cli obok programu (jest potrzebny do harmonogramu)".into(),
            )
        })?;
        let (left, right) = (self.panels[0].location(), self.panels[1].location());
        if !left.is_dir() || !right.is_dir() {
            return Err(EngineError(
                "harmonogram potrzebuje w obu panelach katalogów na dysku".into(),
            ));
        }
        Ok((
            cli,
            vec![
                "run-script".into(),
                left.display().to_string(),
                right.display().to_string(),
                script.display().to_string(),
            ],
        ))
    }

    pub(crate) fn schedule_action(&mut self, action: ScheduleAction) -> Result<(), EngineError> {
        let sys = |e: std::io::Error| EngineError(format!("harmonogram systemu: {e}"));
        match action {
            ScheduleAction::List => {
                let tasks = bluepane_vfs::schedule_list().map_err(sys)?;
                self.status = if tasks.is_empty() {
                    "Brak zaplanowanych makr (schedule add <makro>; daily 07:30)".into()
                } else {
                    format!(
                        "Zaplanowane: {}",
                        tasks
                            .iter()
                            .map(|t| format!("{} ({})", t.id, t.detail))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
            }
            ScheduleAction::Add(name, when) => {
                let name = self.macro_name(&name)?;
                let exe = std::env::current_exe().map_err(|e| EngineError(e.to_string()))?;
                let (program, args) = self.schedule_command(&name, &exe)?;
                bluepane_vfs::schedule_set(&name, when, &program, &args).map_err(sys)?;
                self.status = format!("Zaplanowano makro „{name}” w harmonogramie systemu");
            }
            ScheduleAction::Delete(name) => {
                let name = self.macro_name(&name)?;
                bluepane_vfs::schedule_remove(&name)
                    .map_err(|_| EngineError(format!("makro „{name}” nie jest zaplanowane")))?;
                self.status = format!("Usunięto makro „{name}” z harmonogramu systemu");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_parsed_with_polish_and_english_day_names() {
        assert_eq!(parse_when("hourly").unwrap(), ScheduleWhen::Hourly);
        assert_eq!(
            parse_when("daily 07:30").unwrap(),
            ScheduleWhen::Daily {
                hour: 7,
                minute: 30
            }
        );
        assert_eq!(
            parse_when("weekly śr 23:59").unwrap(),
            ScheduleWhen::Weekly {
                day: 3,
                hour: 23,
                minute: 59
            }
        );
        assert_eq!(
            parse_when("weekly Sun 0:05").unwrap(),
            ScheduleWhen::Weekly {
                day: 0,
                hour: 0,
                minute: 5
            }
        );
        for bad in [
            "",
            "daily",
            "daily 24:00",
            "daily 7.30",
            "weekly xx 07:30",
            "yearly 07:30",
        ] {
            assert!(parse_when(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_command_parses_and_names_with_spaces_and_polish_letters_work() {
        assert_eq!(
            parse_schedule("add Zażółć gęślą; daily 07:30").unwrap(),
            ScheduleAction::Add(
                "Zażółć gęślą".into(),
                ScheduleWhen::Daily {
                    hour: 7,
                    minute: 30
                }
            )
        );
        assert_eq!(parse_schedule("").unwrap(), ScheduleAction::List);
        assert_eq!(
            parse_schedule("delete Zażółć").unwrap(),
            ScheduleAction::Delete("Zażółć".into())
        );
        assert!(parse_schedule("add bez pory").is_err());
        assert!(parse_schedule("zzz").is_err());
    }

    #[test]
    fn the_scheduled_command_runs_the_script_with_both_panel_folders() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in ["l", "r", "bin"] {
            std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
        }
        let mut s = Session::new(tmp.path().join("l"), tmp.path().join("r"));
        s.use_config_dir(&tmp.path().join("config"));
        let exe = tmp
            .path()
            .join("bin")
            .join(format!("bluepane{}", std::env::consts::EXE_SUFFIX));
        assert!(
            s.schedule_command("Porządki", &exe).is_err(),
            "no macro yet"
        );

        std::fs::create_dir_all(tmp.path().join("config/macros")).unwrap();
        std::fs::write(tmp.path().join("config/macros/Porządki.bp"), "cd ..\n").unwrap();
        let err = s.schedule_command("Porządki", &exe).unwrap_err();
        assert!(err.0.contains("bluepane-cli"), "{}", err.0);

        let cli = tmp
            .path()
            .join("bin")
            .join(format!("bluepane-cli{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&cli, "").unwrap();
        let (program, args) = s.schedule_command("Porządki", &exe).unwrap();
        assert_eq!(program, cli);
        assert_eq!(args[0], "run-script");
        assert_eq!(args[1], tmp.path().join("l").display().to_string());
        assert_eq!(args[2], tmp.path().join("r").display().to_string());
        assert!(args[3].ends_with("Porządki.bp"));
    }
}
