//! Scheduled runs of a program through the system's own scheduler: `crontab` on Linux, the Task
//! Scheduler (`schtasks`) on Windows. bluepane only adds, lists and removes the entries it made
//! itself (marked `bluepane:` in cron, kept in the `bluepane\` folder of the Task Scheduler);
//! nothing else in the user's scheduler is read as ours or changed.
//!
//! The text builders here are plain functions without any system call, so both platforms'
//! output is tested on every platform; only the calls themselves live in `platform`.

use std::path::Path;

/// When a task runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleWhen {
    /// Every hour, at the full hour.
    Hourly,
    Daily {
        hour: u8,
        minute: u8,
    },
    /// `day`: 0 = Sunday .. 6 = Saturday (as in cron).
    Weekly {
        day: u8,
        hour: u8,
        minute: u8,
    },
}

/// One entry made by bluepane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledTask {
    pub id: String,
    /// What the system reports about its timing (cron fields or the next run).
    pub detail: String,
}

/// The marker closing a cron line made by bluepane.
const MARK: &str = "# bluepane:";

/// A word safe to put in a `sh` command line: single quotes, and `%` escaped for crontab.
// Used by the Linux build and by the tests of every platform.
#[allow(dead_code)]
pub(crate) fn sh_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''")).replace('%', "\\%")
}

/// The crontab line for a task.
#[allow(dead_code)]
pub(crate) fn cron_line(id: &str, when: ScheduleWhen, program: &Path, args: &[String]) -> String {
    let fields = match when {
        ScheduleWhen::Hourly => "0 * * * *".to_string(),
        ScheduleWhen::Daily { hour, minute } => format!("{minute} {hour} * * *"),
        ScheduleWhen::Weekly { day, hour, minute } => format!("{minute} {hour} * * {day}"),
    };
    let mut command = sh_quote(&program.display().to_string());
    for arg in args {
        command.push(' ');
        command.push_str(&sh_quote(arg));
    }
    format!("{fields} {command} {MARK}{id}")
}

/// The crontab text with the entry of `id` replaced by `line` (or just removed when `None`),
/// and whether there was one. Every other line is kept exactly.
#[allow(dead_code)]
pub(crate) fn cron_replace(existing: &str, id: &str, line: Option<&str>) -> (String, bool) {
    let tail = format!("{MARK}{id}");
    let mut found = false;
    let mut out = String::new();
    for l in existing.lines() {
        if l.ends_with(&tail) {
            found = true;
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    if let Some(line) = line {
        out.push_str(line);
        out.push('\n');
    }
    (out, found)
}

/// The entries of bluepane in a crontab text.
#[allow(dead_code)]
pub(crate) fn cron_tasks(existing: &str) -> Vec<ScheduledTask> {
    existing
        .lines()
        .filter_map(|l| {
            let (before, id) = l.rsplit_once(MARK)?;
            let detail: Vec<&str> = before.split_whitespace().take(5).collect();
            Some(ScheduledTask {
                id: id.to_string(),
                detail: detail.join(" "),
            })
        })
        .collect()
}

const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

/// The name of a task in the Task Scheduler.
#[allow(dead_code)]
pub(crate) fn task_name(id: &str) -> String {
    format!("bluepane\\{id}")
}

/// Arguments of `schtasks /Create` (replaces a task of the same name).
#[allow(dead_code)]
pub(crate) fn schtasks_create(
    id: &str,
    when: ScheduleWhen,
    program: &Path,
    args: &[String],
) -> Vec<String> {
    let mut run = format!("\"{}\"", program.display());
    for arg in args {
        run.push_str(&format!(" \"{arg}\""));
    }
    let mut out: Vec<String> = ["/Create", "/F", "/TN"].map(String::from).to_vec();
    out.push(task_name(id));
    out.push("/TR".into());
    out.push(run);
    match when {
        ScheduleWhen::Hourly => out.extend(["/SC", "HOURLY"].map(String::from)),
        ScheduleWhen::Daily { hour, minute } => out.extend(
            ["/SC", "DAILY", "/ST"]
                .map(String::from)
                .into_iter()
                .chain([format!("{hour:02}:{minute:02}")]),
        ),
        ScheduleWhen::Weekly { day, hour, minute } => out.extend(
            ["/SC", "WEEKLY", "/D"]
                .map(String::from)
                .into_iter()
                .chain([DAYS[usize::from(day % 7)].to_string(), "/ST".into()])
                .chain([format!("{hour:02}:{minute:02}")]),
        ),
    }
    out
}

/// The entries of bluepane in the CSV output of `schtasks /Query /FO CSV /NH`.
#[allow(dead_code)]
pub(crate) fn schtasks_tasks(csv: &str) -> Vec<ScheduledTask> {
    csv.lines()
        .filter_map(|l| {
            let mut fields = l.trim().trim_matches('"').split("\",\"");
            let name = fields.next()?;
            let id = name.strip_prefix("\\bluepane\\")?;
            Some(ScheduledTask {
                id: id.to_string(),
                detail: fields.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_cron_line_quotes_every_word_and_ends_with_the_marker() {
        let line = cron_line(
            "Zażółć 1",
            ScheduleWhen::Weekly {
                day: 1,
                hour: 7,
                minute: 5,
            },
            &PathBuf::from("/opt/blue pane/bluepane-cli"),
            &["run-script".into(), "/home/ala/it's 100%".into()],
        );
        assert_eq!(
            line,
            "5 7 * * 1 '/opt/blue pane/bluepane-cli' 'run-script' '/home/ala/it'\\''s 100\\%' # bluepane:Zażółć 1"
        );
        assert_eq!(
            cron_line("x", ScheduleWhen::Hourly, Path::new("/b"), &[]),
            "0 * * * * '/b' # bluepane:x"
        );
        assert!(cron_line(
            "x",
            ScheduleWhen::Daily {
                hour: 23,
                minute: 0
            },
            Path::new("/b"),
            &[]
        )
        .starts_with("0 23 * * * "));
    }

    #[test]
    fn replacing_touches_only_the_entry_of_the_same_name() {
        let existing = "MAILTO=a\n0 1 * * * backup # bluepane:inne\n* * * * * mine # bluepane:A\n5 5 * * * tez\n";
        let (out, found) = cron_replace(existing, "A", Some("1 1 * * * new # bluepane:A"));
        assert!(found);
        assert_eq!(
            out,
            "MAILTO=a\n0 1 * * * backup # bluepane:inne\n5 5 * * * tez\n1 1 * * * new # bluepane:A\n"
        );
        let (out, found) = cron_replace(existing, "nie ma", None);
        assert!(!found);
        assert_eq!(out, existing);
        let (out, found) = cron_replace(existing, "A", None);
        assert!(found && !out.contains("mine"));
        let tasks = cron_tasks(existing);
        assert_eq!(
            tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["inne", "A"]
        );
        assert_eq!(tasks[0].detail, "0 1 * * *");
    }

    #[test]
    fn schtasks_arguments_name_the_task_the_time_and_the_command() {
        let args = schtasks_create(
            "Zażółć",
            ScheduleWhen::Weekly {
                day: 0,
                hour: 7,
                minute: 5,
            },
            Path::new(r"C:\Program Files\bluepane\bluepane-cli.exe"),
            &["run-script".into(), r"C:\Users\Ala\Mój skrypt.bp".into()],
        );
        assert_eq!(args[..4], ["/Create", "/F", "/TN", r"bluepane\Zażółć"]);
        assert_eq!(
            args[5],
            r#""C:\Program Files\bluepane\bluepane-cli.exe" "run-script" "C:\Users\Ala\Mój skrypt.bp""#
        );
        assert_eq!(args[6..], ["/SC", "WEEKLY", "/D", "SUN", "/ST", "07:05"]);
        let daily = schtasks_create(
            "x",
            ScheduleWhen::Daily {
                hour: 9,
                minute: 30,
            },
            Path::new("b"),
            &[],
        );
        assert_eq!(daily[6..], ["/SC", "DAILY", "/ST", "09:30"]);
        let hourly = schtasks_create("x", ScheduleWhen::Hourly, Path::new("b"), &[]);
        assert_eq!(hourly[6..], ["/SC", "HOURLY"]);
    }

    #[test]
    fn only_tasks_of_the_bluepane_folder_are_listed() {
        let csv = "\"\\bluepane\\Zażółć\",\"2026-10-21 07:05:00\",\"Gotowe\"\r\n\"\\Microsoft\\Inne\",\"N/A\",\"Gotowe\"\r\n\"\\bluepane\\b\",\"N/A\",\"Gotowe\"\r\n";
        let tasks = schtasks_tasks(csv);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "Zażółć");
        assert_eq!(tasks[0].detail, "2026-10-21 07:05:00");
    }
}
