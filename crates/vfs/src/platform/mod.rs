//! Platform-specific code. Exactly one of the two modules is compiled in.

mod arp;
mod verbatim;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{
    copy_symlink, extended_path, file_mode, is_hidden, make_writable, neighbours, set_mode,
    volumes, DEFAULT_OPENER,
};

#[cfg(not(windows))]
mod linux;
#[cfg(not(windows))]
pub use linux::{
    copy_symlink, extended_path, file_mode, is_hidden, make_writable, neighbours, set_mode,
    volumes, DEFAULT_OPENER,
};

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// Program and arguments that open `path`: the user's own command when given (split on
/// spaces, so `code -w` works), otherwise the platform's default opener.
pub(crate) fn launch_plan(editor: Option<&str>, path: &Path) -> (String, Vec<OsString>) {
    let custom: Vec<&str> = editor
        .map(|e| e.split_whitespace().collect())
        .unwrap_or_default();
    let (program, mut args): (String, Vec<OsString>) = match custom.split_first() {
        Some((program, rest)) => (
            (*program).to_string(),
            rest.iter().map(OsString::from).collect(),
        ),
        None => {
            let (program, args) = DEFAULT_OPENER;
            (
                program.to_string(),
                args.iter().map(OsString::from).collect(),
            )
        }
    };
    args.push(path.as_os_str().to_os_string());
    (program, args)
}

/// Opens a file outside bluepane: with `editor` (a command line such as `code -w`) when
/// given, otherwise with the system's default application. Does not wait for it to close.
pub fn open_external(path: &Path, editor: Option<&str>) -> io::Result<()> {
    let (program, args) = launch_plan(editor, path);
    let mut child = std::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("{program}: {e}")))?;
    // Reap the child when it exits so it does not linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The user's home directory, if it can be determined.
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

/// Directory for bluepane's configuration.
///
/// `BLUEPANE_CONFIG_DIR` overrides the platform default, which is handy for tests
/// and for running with an isolated profile.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("BLUEPANE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("bluepane"))
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(xdg).join("bluepane"))
    } else {
        home_dir().map(|h| h.join(".config").join("bluepane"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_opener_gets_the_path_last() {
        let (program, args) = launch_plan(None, Path::new("/tmp/a b.txt"));
        assert_eq!(program, DEFAULT_OPENER.0);
        assert_eq!(args.last().unwrap(), "/tmp/a b.txt");
    }

    #[test]
    fn a_custom_editor_keeps_its_arguments() {
        let (program, args) = launch_plan(Some("code -w --new-window"), Path::new("f.txt"));
        assert_eq!(program, "code");
        assert_eq!(args, ["-w", "--new-window", "f.txt"].map(OsString::from));
    }

    #[test]
    fn a_blank_editor_falls_back_to_the_default() {
        let (program, _) = launch_plan(Some("   "), Path::new("f"));
        assert_eq!(program, DEFAULT_OPENER.0);
    }

    #[test]
    fn a_missing_program_is_a_readable_error() {
        let err =
            open_external(Path::new("f"), Some("definitely-not-a-real-program-xyz")).unwrap_err();
        assert!(err
            .to_string()
            .contains("definitely-not-a-real-program-xyz"));
    }
}
