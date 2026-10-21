//! The engine: two panels, a command set and a key map.
//!
//! Every user action is a [`Command`]. The UI, the command line and the CLI all go
//! through [`Session::execute`] / [`Session::run_line`], so everything is testable
//! without a window.

mod buttons;
mod cloud;
mod command;
mod command_help;
mod compare;
mod context_menu;
mod dirsize;
mod diskmap;
mod documents;
mod dragdrop;
mod duplicates;
mod fileclass;
mod filter;
mod find;
mod format;
mod gitstatus;
mod hashes;
mod job;
mod jump;
mod keymap;
mod keysheet;
mod layouts;
mod macros;
mod mask;
mod multirename;
mod navbar;
mod omnibox;
mod pageview;
mod palette;
mod panel;
mod places;
mod queue;
mod quickbox;
mod quickview;
mod remote;
mod remote_job;
mod scan;
mod scanhistory;
mod schedule;
mod session;
mod settings;
mod syncprofiles;
mod thumbs;
mod tools;
mod viewer;
mod workspace;

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
pub use buttons::{Button, ButtonAction, ButtonLaunch};
pub use cloud::CloudAction;
pub use command::{
    parse_size, Command, NameFormat, PackArgs, PaletteAction, QueueMove, QuickAction, SpeedLimit,
    SyncArgs, TabAction, UnpackArgs, ViewerAction, COMMAND_NAMES,
};
pub use command_help::{command_help, commands_markdown, CommandHelp, COMMAND_HELP};
pub use compare::{
    Algo, ChecksumAction, CompareAction, CompareView, DiffRow, Outcome as CompareOutcome, RowKind,
    SumLine, SumStatus,
};
pub use context_menu::MenuItem;
pub use dirsize::{DirSizes, PanelStats};
pub use diskmap::{squarify, DiskMap, MapAction, MapItem, MapTile};
pub use dragdrop::DropTarget;
pub use duplicates::{DupAction, DupFile, DupGroup, DupSummary, Duplicates, KeepRule};
pub use fileclass::{class_of, class_of_name, FileClass};
pub use filter::{FilterSpec, Only};
pub use find::{FindForm, FindView, SearchData};
pub use format::{format_mode, format_modified, human_size, name_parts};
pub use gitstatus::GitStatus;
pub use hashes::{HashAction, Hashes};
pub use job::{
    Answer, ArchiveJob, Event, HostPrompt, JobKind, JobSpec, PackOptions, PasswordProblem,
    RemoteJob, RemoteOp, Report, RunningJob, SyncJob,
};
pub use jump::{JumpEntry, Visits};
pub use keymap::Keymap;
pub use keysheet::KeyHelp;
pub use layouts::LayoutAction;
pub use macros::MacroAction;
pub use mask::Mask;
pub use multirename::{RenameEntry, RenameView};
pub use navbar::{crumbs, Crumb, DiskUsage};
pub use omnibox::{classify as classify_address, Hint, Target};
pub use pageview::{PageMove, PageState, PageViewer, PageZoom, RenderedPage, ZoomChange};
pub use palette::{rank as rank_commands, Palette, PaletteEntry};
pub use panel::{
    is_parent_entry, ArchiveView, ClickMode, Panel, PanelView, RemoteView, Sort, SortKey,
    VirtualList, PARENT_NAME,
};
pub use places::{FavoriteAction, Favorites, Place, PlaceSection};
pub use queue::{FinishedJob, Notice, QueueLine};
pub use quickbox::{BoxEntry, BoxTarget};
pub use quickview::{QuickContent, QuickKey, QuickView, QUICK_DELAY};
pub use scan::{Favorite, Launch, ScanData, ScanForm, ScanServices, ScanView, ShareList};
pub use scanhistory::{KnownDevice, ScanHistory, ScanLogEntry};
pub use schedule::ScheduleAction;
pub use session::{
    ConflictPrompt, Editor, InputKind, JobStatus, PackForm, Pending, Session, SyncView, TabStrip,
};
pub use settings::{
    normalize_key, Column, Density, COLUMN_WIDTH, DEFAULT_COLUMNS, DEFAULT_THEME, THEMES,
};
pub use syncprofiles::ProfileAction;
pub use thumbs::{Thumbnails, THUMB_SIZE};
pub use tools::Tool;
pub use viewer::{Row, ViewMode, Viewer};
pub use workspace::WorkspaceAction;

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
