//! The engine: two panels, a command set and a key map.
//!
//! Every user action is a [`Command`]. The UI, the command line and the CLI all go
//! through [`Session::execute`] / [`Session::run_line`], so everything is testable
//! without a window.

mod command;
mod dirsize;
mod documents;
mod fileclass;
mod find;
mod format;
mod job;
mod keymap;
mod mask;
mod multirename;
mod navbar;
mod pageview;
mod panel;
mod queue;
mod quickview;
mod remote;
mod remote_job;
mod scan;
mod session;
mod settings;
mod viewer;

pub use bluepane_archive::{Format as ArchiveFormat, Level as CompressionLevel};
pub use bluepane_docs::PageImage;
pub use bluepane_netscan::{local_range, Host as ScanHost, ScanSummary, Service as ScanService};
pub use bluepane_ops::Resolution;
pub use bluepane_remote::{bitwarden_available, Protocol, SecretServices, SecretStore, Site};
pub use bluepane_rename::{
    changed_spans as rename_changed_spans, CaseMode as RenameCase, Item as RenameItem,
    Plan as RenamePlan, Row as RenameRow, Settings as RenameSettings, Span as RenameSpan,
    Status as RenameStatus,
};
pub use bluepane_search::{
    Encoding as FindEncoding, Hit as FindHit, LineHit as FindLine, Location as FindLocation,
    Summary as FindSummary,
};
pub use bluepane_sync::{
    Action as SyncAction, Category as SyncCategory, CompareBy, Deletion as SyncDeletion,
    Direction as SyncDirection, ExecReport as SyncReport, Filter as SyncFilter, Kind as SyncKind,
    Node as SyncNode, Op as SyncOp, Preview as SyncPreview, Settings as SyncSettings,
    Variant as SyncVariant,
};
pub use bluepane_vfs::{Share, ShareKind};
pub use command::{
    parse_size, Command, NameFormat, PackArgs, QueueMove, QuickAction, SpeedLimit, SyncArgs,
    TabAction, UnpackArgs, ViewerAction, COMMAND_NAMES,
};
pub use dirsize::DirSizes;
pub use fileclass::{class_of, class_of_name, FileClass};
pub use find::{FindForm, FindView, SearchData};
pub use format::{format_modified, human_size, name_parts};
pub use job::{
    Answer, ArchiveJob, Event, HostPrompt, JobKind, JobSpec, PackOptions, PasswordProblem,
    RemoteJob, RemoteOp, Report, RunningJob, SyncJob,
};
pub use keymap::Keymap;
pub use mask::Mask;
pub use multirename::{RenameEntry, RenameView};
pub use navbar::{crumbs, Crumb, DiskUsage};
pub use pageview::{PageMove, PageState, PageViewer, PageZoom, RenderedPage, ZoomChange};
pub use panel::{
    is_parent_entry, ArchiveView, ClickMode, Panel, RemoteView, Sort, SortKey, VirtualList,
    PARENT_NAME,
};
pub use queue::{FinishedJob, QueueLine};
pub use quickview::{QuickContent, QuickKey, QuickView, QUICK_DELAY};
pub use scan::{Favorite, Launch, ScanData, ScanForm, ScanServices, ScanView, ShareList};
pub use session::{
    ConflictPrompt, Editor, InputKind, JobStatus, PackForm, Pending, Session, SyncView, TabStrip,
};
pub use settings::{DEFAULT_THEME, THEMES};
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
