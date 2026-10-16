//! A small log file (`bluepane.log` in the config directory) for bug reports: the start of
//! every run and every panic with its place. On Windows the release build has no console
//! window, so this is where a crash message ends up.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The file is cut to a fresh one when it grows past this (the old one is kept as `.old`).
const LIMIT: u64 = 512 * 1024;

/// Appends one line to the log, with the time (seconds since 1970). Never fails loudly:
/// a log that cannot be written must not stop the program.
pub fn append(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LIMIT) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[{seconds}] {text}");
    }
}

/// Writes the start of the run and makes every panic go to the log as well as to the
/// terminal (where there is one).
pub fn install(path: PathBuf) {
    append(
        &path,
        &format!(
            "start bluepane {} ({} {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let place = info
            .location()
            .map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        append(&path, &format!("PANIC{place}: {message}"));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_appended_with_a_time() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("conf/bluepane.log");
        append(&path, "one");
        append(&path, "two");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].starts_with('[') && lines[0].ends_with("] one"),
            "{text}"
        );
        assert!(lines[1].ends_with("] two"));
    }

    #[test]
    fn a_big_log_is_set_aside_and_a_new_one_started() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bluepane.log");
        std::fs::write(&path, vec![b'x'; (LIMIT + 10) as usize]).unwrap();
        append(&path, "fresh");
        assert!(tmp.path().join("bluepane.log.old").exists());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("fresh") && text.len() < 100, "{}", text.len());
    }

    #[test]
    fn a_panic_ends_up_in_the_log() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bluepane.log");
        // The hook is process-wide: run the panic on a thread and restore the default after.
        let original = std::panic::take_hook();
        install(path.clone());
        let result = std::thread::spawn(|| panic!("boom for the log")).join();
        assert!(result.is_err());
        let _ = std::panic::take_hook();
        std::panic::set_hook(original);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("start bluepane"), "{text}");
        assert!(
            text.contains("PANIC") && text.contains("boom for the log"),
            "{text}"
        );
    }
}
