//! Running an outside program that may take long, with a way to stop it.

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

/// How often a running program is checked on (and `cancelled` is asked).
const POLL: Duration = Duration::from_millis(50);

/// Runs `program` without a shell and without a console window, collecting what it prints.
/// `cancelled` is asked about every 50 ms while the program runs; when it says yes the program
/// is killed and the result is `Ok(None)`. The same closure is a good place to look at how far
/// the program has got (a growing output file, say).
pub fn run_cancellable(
    program: &str,
    args: &[String],
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Option<Output>> {
    run_cancellable_env(program, args, &[], cancelled)
}

/// Like [`run_cancellable`], with extra environment variables for the program (a way to hand
/// it a password that must not show up in the list of processes).
pub fn run_cancellable_env(
    program: &str,
    args: &[String],
    env: &[(String, String)],
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Option<Output>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    let mut child = command.spawn()?;
    // The pipes are drained on threads, so a program that prints a lot never blocks on us.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(Output {
                status,
                stdout: out.join().unwrap_or_default(),
                stderr: err.join().unwrap_or_default(),
            }));
        }
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            return Ok(None);
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// A program that stays around for a few seconds and one that ends at once, per platform.
    fn slow() -> (&'static str, Vec<String>) {
        if cfg!(windows) {
            ("ping", vec!["-n".into(), "8".into(), "127.0.0.1".into()])
        } else {
            ("sleep", vec!["8".into()])
        }
    }

    #[test]
    fn output_is_collected_and_a_missing_program_is_an_error() {
        let me = std::env::current_exe().unwrap();
        let output = run_cancellable(
            me.to_str().unwrap(),
            &["--list".into(), "--format".into(), "terse".into()],
            &|| false,
        )
        .unwrap()
        .expect("not cancelled");
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        let err = run_cancellable("no-such-program-bluepane", &[], &|| false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_program_is_killed_when_cancelled_and_the_check_is_called_while_it_runs() {
        let (program, args) = slow();
        let started = Instant::now();
        let calls = std::cell::Cell::new(0);
        let result = run_cancellable(program, &args, &|| {
            calls.set(calls.get() + 1);
            calls.get() >= 3
        })
        .unwrap();
        assert!(result.is_none(), "cancelled");
        assert!(calls.get() >= 3);
        assert!(started.elapsed() < Duration::from_secs(6), "it was killed");
    }
}
