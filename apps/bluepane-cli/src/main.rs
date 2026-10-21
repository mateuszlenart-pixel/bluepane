//! Headless front end. The same commands as the GUI, no window.
//!
//! ```text
//! bluepane-cli --version
//! bluepane-cli commands
//! bluepane-cli ls <dir>
//! bluepane-cli run <left_dir> <right_dir> "select a.txt" "copy" ...
//! bluepane-cli run <left_dir> <right_dir> "select a.txt" "delete" "confirm"
//! bluepane-cli run-script <left_dir> <right_dir> <script.bp> [argument]...
//! ```

use std::path::Path;
use std::process::ExitCode;

use bluepane_engine::{command_help, Resolution, Session, COMMAND_NAMES};

const USAGE: &str = "użycie:\n  bluepane-cli --version\n  bluepane-cli commands\n  bluepane-cli ls <katalog>\n  bluepane-cli run <lewy> <prawy> <komenda>...\n  bluepane-cli run-script <lewy> <prawy> <skrypt.bp> [argument]...";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V" | "version") => {
            println!(
                "{}",
                bluepane_engine::version_line().replacen("bluepane", "bluepane-cli", 1)
            );
            ExitCode::SUCCESS
        }
        Some("commands") => {
            for name in COMMAND_NAMES {
                match command_help(name) {
                    Some(help) => println!("{name}\t{}", help.description),
                    None => println!("{name}"),
                }
            }
            ExitCode::SUCCESS
        }
        Some("ls") if args.len() == 2 => ls(Path::new(&args[1])),
        Some("run") if args.len() >= 4 => run(&args[1], &args[2], &args[3..]),
        Some("run-script") if args.len() >= 4 => {
            run_script(&args[1], &args[2], &args[3], &args[4..])
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn ls(dir: &Path) -> ExitCode {
    match bluepane_vfs::list_dir(dir) {
        Ok(entries) => {
            for e in entries {
                let suffix = if e.is_dir() { "/" } else { "" };
                println!("{}{suffix}\t{}", e.name, e.size);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{}: {err}", dir.display());
            ExitCode::FAILURE
        }
    }
}

/// Plays a `.bp` script (see `macro` in the app): one command per line, `#` starts a comment,
/// `%1`..`%9` are the arguments, `%d` / `%o` / `%f` the folders and the name under the cursor.
fn run_script(left: &str, right: &str, file: &str, args: &[String]) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{file}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut session = Session::new(left, right);
    session.auto_resolve = Some(Resolution::Skip);
    for line in Session::script_commands(&text) {
        let line = session.fill_parameters(&line, args);
        if let Err(err) = session.run_line(&line) {
            eprintln!("{line}: {err}");
            return ExitCode::FAILURE;
        }
        session.wait_idle();
        if session.status_is_error {
            eprintln!("{line}: {}", session.status);
            return ExitCode::FAILURE;
        }
        if !session.status.is_empty() {
            println!("{line}: {}", session.status);
        }
    }
    if session.pending.is_some() {
        eprintln!("polecenie czeka na potwierdzenie: dodaj linię `confirm` (albo `cancel`)");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Runs the commands one after another, waiting for background jobs to finish.
///
/// Scripts never get a conflict dialog: an existing target is skipped (never overwritten),
/// and `delete` only asks, so it needs a following `confirm` command to take effect.
fn run(left: &str, right: &str, commands: &[String]) -> ExitCode {
    let mut session = Session::new(left, right);
    session.auto_resolve = Some(Resolution::Skip);
    for line in commands {
        if let Err(err) = session.run_line(line) {
            eprintln!("{line}: {err}");
            return ExitCode::FAILURE;
        }
        session.wait_idle();
        if session.status_is_error {
            eprintln!("{line}: {}", session.status);
            return ExitCode::FAILURE;
        }
        if !session.status.is_empty() {
            println!("{line}: {}", session.status);
        }
    }
    if session.pending.is_some() {
        eprintln!("polecenie czeka na potwierdzenie: dodaj komendę `confirm` (albo `cancel`)");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_runs_with_arguments_and_stops_at_a_bad_line() {
        let tmp = tempfile::tempdir().unwrap();
        let (l, r) = (tmp.path().join("l"), tmp.path().join("r"));
        std::fs::create_dir_all(&l).unwrap();
        std::fs::create_dir_all(&r).unwrap();
        let script = tmp.path().join("x.bp");
        std::fs::write(&script, "# nowy folder\nmkdir %1\n").unwrap();
        let code = run_script(
            l.to_str().unwrap(),
            r.to_str().unwrap(),
            script.to_str().unwrap(),
            &["Łódź".into()],
        );
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(l.join("Łódź").is_dir());

        std::fs::write(&script, "zzzbad\nmkdir nigdy\n").unwrap();
        let code = run_script(
            l.to_str().unwrap(),
            r.to_str().unwrap(),
            script.to_str().unwrap(),
            &[],
        );
        assert_eq!(code, ExitCode::FAILURE);
        assert!(!l.join("nigdy").exists());
    }
}
