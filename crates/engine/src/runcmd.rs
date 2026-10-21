//! `terminal` and `run`: the system terminal in the folder of the active panel, and a program
//! started in that folder whose output lands in the viewer. Instead of a terminal of our own:
//! the system's is always better, so we only open it where the user is, and for one-off
//! commands (`!git status`, `!zip -r %n.zip %s`) show what they printed.
//!
//! `run` uses the words and `%f %n %d %o %s` of own buttons (see `buttons`) and no shell, so
//! `;` or `&` in a name are harmless. The program runs in the background and can be stopped
//! with `runstop`; its output is shown when it ends.

use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::session::Session;
use crate::EngineError;

/// How much of each stream is shown; the rest is cut off with a note.
const MAX_SHOWN: usize = 2 * 1024 * 1024;

type Finished = std::io::Result<Option<Output>>;

/// A program that is running (or just ended and waits to be shown).
#[derive(Debug)]
pub(crate) struct RunJob {
    line: String,
    dir: std::path::PathBuf,
    cancel: Arc<AtomicBool>,
    done: Arc<Mutex<Option<Finished>>>,
}

impl Session {
    /// Opens the system terminal in the folder of the active panel.
    pub(crate) fn open_terminal_here(&mut self) -> Result<(), EngineError> {
        let dir = self.active_panel().location();
        if !dir.is_dir() {
            return Err(EngineError(
                "terminal otwiera się w katalogu na dysku, nie w archiwum ani na serwerze".into(),
            ));
        }
        bluepane_vfs::open_terminal(&dir)
            .map_err(|e| EngineError(format!("nie udało się otworzyć terminala: {e}")))?;
        self.status = format!("Terminal: {}", dir.display());
        Ok(())
    }

    /// Starts `line` in the background; the output is shown by [`Session::poll_run`].
    pub(crate) fn run_command(&mut self, line: &str) -> Result<(), EngineError> {
        if self.run_job.is_some() {
            return Err(EngineError(
                "inne polecenie jeszcze działa (runstop je przerywa)".into(),
            ));
        }
        let line = line.trim();
        if line.is_empty() {
            return Err(EngineError(
                "run <program argumenty> (albo !program)".into(),
            ));
        }
        let launch = self.expand_line(line)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Arc::new(Mutex::new(None));
        let (flag, slot) = (cancel.clone(), done.clone());
        std::thread::spawn(move || {
            let result = bluepane_vfs::run_cancellable(&launch.program, &launch.args, &|| {
                flag.load(Ordering::Relaxed)
            })
            .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {e}", launch.program)));
            *slot.lock().unwrap() = Some(result);
        });
        self.status = format!("Działa: {line} (runstop przerywa)");
        self.run_job = Some(RunJob {
            line: line.to_string(),
            dir: self.active_panel().location(),
            cancel,
            done,
        });
        Ok(())
    }

    /// Stops the program started with `run`.
    pub(crate) fn stop_run(&mut self) -> Result<(), EngineError> {
        let job = self
            .run_job
            .as_ref()
            .ok_or_else(|| EngineError("żadne polecenie nie działa".into()))?;
        job.cancel.store(true, Ordering::Relaxed);
        self.status = "Przerywam polecenie…".into();
        Ok(())
    }

    /// Whether a program started with `run` is still running.
    pub fn run_active(&self) -> bool {
        self.run_job.is_some()
    }

    /// Called every frame: shows the output of a program that ended.
    pub(crate) fn poll_run(&mut self) {
        let Some(job) = &self.run_job else { return };
        let Some(result) = job.done.lock().unwrap().take() else {
            return;
        };
        let job = self.run_job.take().expect("checked above");
        match result {
            Err(e) => self.status = format!("Nie udało się uruchomić: {e}"),
            Ok(None) => self.status = format!("Przerwano: {}", job.line),
            Ok(Some(output)) => {
                let text = render_output(&job.line, &job.dir, &output);
                match self.show_git_text("wynik.txt", &text) {
                    Ok(()) => {
                        self.status = match output.status.code() {
                            Some(0) => format!("Gotowe: {} (Esc zamyka)", job.line),
                            Some(code) => format!("Kod wyjścia {code}: {} (Esc zamyka)", job.line),
                            None => format!("Przerwane sygnałem: {} (Esc zamyka)", job.line),
                        }
                    }
                    Err(e) => self.status = e.0,
                }
            }
        }
    }
}

/// Decodes what a program printed: UTF-8, or the Windows code page 1250 when it is not.
fn decode(bytes: &[u8]) -> String {
    let cut = &bytes[..bytes.len().min(MAX_SHOWN)];
    let mut text = match std::str::from_utf8(cut) {
        Ok(text) => text.to_string(),
        Err(_) => bluepane_search::decode_cp1250(cut),
    };
    if bytes.len() > MAX_SHOWN {
        text.push_str("\n… (dalsza część wyjścia pominięta)\n");
    }
    text
}

fn render_output(line: &str, dir: &std::path::Path, output: &Output) -> String {
    let mut text = format!("$ {line}\n# katalog: {}\n\n", dir.display());
    text.push_str(&decode(&output.stdout));
    if !output.stderr.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("\n--- błędy (stderr) ---\n");
        text.push_str(&decode(&output.stderr));
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    match output.status.code() {
        Some(code) => text.push_str(&format!("\n# kod wyjścia: {code}\n")),
        None => text.push_str("\n# przerwane sygnałem\n"),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn session(tmp: &std::path::Path) -> Session {
        std::fs::create_dir_all(tmp.join("l")).unwrap();
        std::fs::create_dir_all(tmp.join("r")).unwrap();
        std::fs::write(tmp.join("l/Zażółć gęślą.txt"), "a").unwrap();
        Session::new(tmp.join("l"), tmp.join("r"))
    }

    fn wait_for_end(s: &mut Session) {
        let start = Instant::now();
        while s.run_active() {
            assert!(start.elapsed() < Duration::from_secs(60), "polecenie wisi");
            std::thread::sleep(Duration::from_millis(20));
            s.poll();
        }
    }

    /// A command line that runs this test binary, which prints the list of tests and exits.
    fn self_line() -> String {
        format!("\"{}\" --list", std::env::current_exe().unwrap().display())
    }

    fn shown_text(s: &Session) -> String {
        let viewer = s.viewer.as_ref().expect("wynik w podglądzie");
        std::fs::read_to_string(viewer.path()).unwrap()
    }

    #[test]
    fn the_output_of_a_command_lands_in_the_viewer() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line(&format!("run {}", self_line())).unwrap();
        assert!(s.run_active());
        wait_for_end(&mut s);
        let text = shown_text(&s);
        assert!(text.starts_with("$ "), "{text}");
        assert!(text.contains("the_output_of_a_command"), "{text}");
        assert!(text.contains("kod wyjścia: 0"), "{text}");
        assert!(s.status.starts_with("Gotowe"), "{}", s.status);
    }

    #[test]
    fn a_bang_is_the_same_as_run() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line(&format!("!{}", self_line())).unwrap();
        wait_for_end(&mut s);
        assert!(shown_text(&s).contains("kod wyjścia"));
    }

    #[test]
    fn a_failing_command_shows_its_exit_code() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        let me = std::env::current_exe().unwrap();
        s.run_line(&format!("run \"{}\" --nie-ma-takiej-opcji", me.display()))
            .unwrap();
        wait_for_end(&mut s);
        assert!(s.status.starts_with("Kod wyjścia"), "{}", s.status);
        assert!(shown_text(&s).contains("stderr"));
    }

    #[test]
    fn a_missing_program_is_a_readable_message() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("run program-ktorego-nie-ma %f").unwrap();
        wait_for_end(&mut s);
        assert!(
            s.status.starts_with("Nie udało się uruchomić"),
            "{}",
            s.status
        );
        assert!(s.viewer.is_none());
    }

    #[test]
    fn placeholders_and_polish_names_reach_the_program_as_one_argument() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cursor 1").unwrap();
        let launch = s.expand_line("prog -f %f -n %n").unwrap();
        assert_eq!(
            launch.args[1],
            tmp.path().join("l/Zażółć gęślą.txt").display().to_string()
        );
        assert_eq!(launch.args[3], "Zażółć gęślą.txt");
        assert_eq!(launch.dir, tmp.path().join("l"));
    }

    #[test]
    fn a_second_command_waits_for_the_first_and_runstop_ends_it() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        let slow = if cfg!(windows) {
            "run ping -n 60 127.0.0.1"
        } else {
            "run sleep 60"
        };
        s.run_line(slow).unwrap();
        assert!(s.run_line("run cokolwiek").is_err());
        s.run_line("runstop").unwrap();
        wait_for_end(&mut s);
        assert!(s.status.starts_with("Przerwano"), "{}", s.status);
        assert!(s.run_line("runstop").is_err());
    }

    #[test]
    fn the_commands_parse() {
        use crate::Command;
        assert_eq!(Command::parse("terminal").unwrap(), Command::Terminal);
        assert_eq!(Command::parse("runstop").unwrap(), Command::RunStop);
        assert_eq!(
            Command::parse("run git status").unwrap(),
            Command::Run("git status".into())
        );
        assert_eq!(
            Command::parse("!  git log").unwrap(),
            Command::Run("git log".into())
        );
    }

    #[test]
    fn run_needs_a_program_and_a_folder_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        assert!(s.run_line("run").is_err());
        assert!(s.run_line("!").is_err());
    }

    #[test]
    fn output_in_the_windows_code_page_is_decoded() {
        // "Zażółć" in cp1250: 'ż' 0xBF, 'ó' 0xF3, 'ł' 0xB3, 'ć' 0xE6.
        let text = decode(&[0x5A, 0x61, 0xBF, 0xF3, 0xB3, 0xE6]);
        assert_eq!(text, "Zażółć");
        assert_eq!(decode("Zażółć".as_bytes()), "Zażółć");
    }
}
