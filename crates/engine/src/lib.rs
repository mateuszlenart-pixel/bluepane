//! The engine: two panels, a command set and a key map.
//!
//! Every user action is a [`Command`]. The UI, the command line and the CLI all go
//! through [`Session::execute`] / [`Session::run_line`], so everything is testable
//! without a window.

mod command;
mod format;
mod job;
mod keymap;
mod mask;
mod panel;
mod session;
mod viewer;

pub use bluepane_ops::Resolution;
pub use command::{Command, ViewerAction, COMMAND_NAMES};
pub use format::{format_modified, human_size, name_parts};
pub use job::{Answer, Event, JobKind, JobSpec, Report, RunningJob};
pub use keymap::Keymap;
pub use mask::Mask;
pub use panel::{is_parent_entry, Panel, Sort, SortKey, PARENT_NAME};
pub use session::{ConflictPrompt, InputKind, JobStatus, Pending, Session};
pub use viewer::{Row, ViewMode, Viewer};

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
