//! Platform-specific code. Exactly one of the two modules is compiled in.

mod verbatim;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{copy_symlink, extended_path, is_hidden, make_writable, volumes};

#[cfg(not(windows))]
mod linux;
#[cfg(not(windows))]
pub use linux::{copy_symlink, extended_path, is_hidden, make_writable, volumes};

use std::path::PathBuf;

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
