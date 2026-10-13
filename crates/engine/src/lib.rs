//! The engine: two panels, a command set and a key map.
//!
//! Every user action is a [`Command`]. The UI, the command line and the CLI all go
//! through [`Session::execute`] / [`Session::run_line`], so everything is testable
//! without a window.

mod command;
mod keymap;
mod panel;
mod session;

pub use command::{Command, COMMAND_NAMES};
pub use keymap::Keymap;
pub use panel::{is_parent_entry, Panel, PARENT_NAME};
pub use session::{Pending, Session};

use std::fmt;

/// A user-facing error (bad command, failed operation). The message is ready to display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError(pub String);

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EngineError {}
