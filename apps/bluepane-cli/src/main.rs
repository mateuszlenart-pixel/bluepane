//! Headless front end. The same commands as the GUI, no window.
//!
//! ```text
//! bluepane-cli commands
//! bluepane-cli ls <dir>
//! bluepane-cli run <left_dir> <right_dir> "select a.txt" "copy" ...
//! bluepane-cli run <left_dir> <right_dir> "select a.txt" "delete" "confirm"
//! ```

use std::path::Path;
use std::process::ExitCode;

use bluepane_engine::{Resolution, Session, COMMAND_NAMES};

const USAGE: &str = "użycie:\n  bluepane-cli commands\n  bluepane-cli ls <katalog>\n  bluepane-cli run <lewy> <prawy> <komenda>...";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("commands") => {
            for name in COMMAND_NAMES {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Some("ls") if args.len() == 2 => ls(Path::new(&args[1])),
        Some("run") if args.len() >= 4 => run(&args[1], &args[2], &args[3..]),
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
