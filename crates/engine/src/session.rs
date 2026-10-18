use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use bluepane_archive::{Change, Format, Level};
use bluepane_ops::{self as ops, Conflict, Resolution};
use bluepane_vfs::Volume;

use crate::job::{
    Answer, ArchiveJob, Event, JobKind, JobSpec, PackOptions, PasswordProblem, Report, RunningJob,
    SyncJob,
};
use crate::{
    is_parent_entry, name_parts, parse_size, Command, EngineError, Keymap, Mask, NameFormat,
    PackArgs, Panel, SyncArgs, SyncFilter, SyncPreview, SyncSettings, TabAction, UnpackArgs,
    Viewer, ViewerAction,
};

/// What a name dialog is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// Name of a new directory in the active panel.
    Mkdir,
    /// New name for this entry.
    Rename(PathBuf),
    /// Text to search for in the open viewer.
    ViewerFind,
    /// The password of an archive; the UI hides what is typed.
    ArchivePassword,
    /// The password (or key passphrase) of this saved SFTP site; hidden like the above.
    SitePassword(String),
    /// The password of this protected PDF; hidden like the above.
    PdfPassword(PathBuf),
}

/// The settings dialog of `pack` (Alt+F5). The UI edits the fields; `confirm` validates them.
#[derive(Debug, Clone)]
pub struct PackForm {
    /// Archive name; the extension of the chosen format is added when missing.
    pub name: String,
    pub format: Format,
    pub level: Level,
    pub password: String,
    pub password_again: String,
    /// Volume size such as `100m`; empty for a single file.
    pub split: String,
    /// 7z only: hide the file names too (needs a password).
    pub encrypt_names: bool,
    /// What is wrong with the settings, shown in the dialog.
    pub error: Option<String>,
    sources: Vec<PathBuf>,
    dest: PathBuf,
}

impl PackForm {
    /// Where the archive will be created.
    pub fn destination(&self) -> &Path {
        &self.dest
    }

    /// How many items will be packed.
    pub fn item_count(&self) -> usize {
        self.sources.len()
    }

    /// The archive file name that would be created.
    pub fn output_name(&self) -> String {
        output_name(&self.name, self.format)
    }
}

/// The name without a known archive extension (`photos.zip` and `a.tar.gz` lose it).
fn strip_archive_extension(name: &str) -> &str {
    let lower = name.to_lowercase();
    for ext in [".tar.gz", ".tgz", ".tar", ".zip", ".7z"] {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// `photos` + ZIP becomes `photos.zip`; a name that already ends in a known archive
/// extension has it replaced (`photos.tar` + ZIP becomes `photos.zip`).
fn output_name(name: &str, format: Format) -> String {
    let name = name.trim();
    if Format::from_name(name) == Some(format) {
        return name.to_string();
    }
    format!("{}.{}", strip_archive_extension(name), format.extension())
}

/// A file conflict the running job is waiting on.
#[derive(Debug, Clone)]
pub struct ConflictPrompt {
    pub conflict: Conflict,
    reply: Sender<Answer>,
}

impl ConflictPrompt {
    fn answer(&self, resolution: Option<Resolution>, for_all: bool) {
        // The worker may already be gone (cancelled elsewhere); nothing to do then.
        let _ = self.reply.send(Answer {
            resolution,
            for_all,
        });
    }
}

/// A question waiting for the user's answer. `confirm` accepts it and `cancel` rejects it,
/// except for conflicts, which are answered with `overwrite`, `newer`, `skip`, `keepboth`
/// (each also as `...-all`) or `cancel` (stops the job).
#[derive(Debug, Clone)]
pub enum Pending {
    /// Both panels show the same directory: make copies of these paths next to the originals.
    Duplicate(Vec<PathBuf>),
    /// Move these paths to the trash.
    ConfirmDelete(Vec<PathBuf>),
    /// Remove these entries from the archive shown in panel `side` (not recoverable).
    ConfirmRemove { side: usize, entries: Vec<String> },
    /// A name is being typed; the UI edits `text` and `confirm` uses it.
    Input { kind: InputKind, text: String },
    /// An existing file stands in the way of the running copy or move.
    Conflict(ConflictPrompt),
    /// The settings of an archive about to be created.
    Pack(PackForm),
    /// An SFTP server with an unknown key: trust it and remember the key?
    TrustHost(crate::job::HostPrompt),
    /// Delete these server paths for good (panel `side` shows the server).
    ConfirmRemoteDelete { side: usize, paths: Vec<String> },
}

/// What the progress bar shows about the running job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStatus {
    pub label: &'static str,
    pub current: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub items_done: u64,
    pub items_total: u64,
    /// Jobs waiting behind this one.
    pub queued: usize,
    /// The job is blocked on a conflict question.
    pub waiting_for_answer: bool,
}

impl JobStatus {
    /// Completed share between 0 and 1, or `None` while the total is still being counted.
    pub fn fraction(&self) -> Option<f32> {
        let (done, total) = if self.bytes_total > 0 {
            (self.bytes_done, self.bytes_total)
        } else if self.items_total > 0 {
            (self.items_done, self.items_total)
        } else {
            return None;
        };
        Some((done as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }
}

/// The tabs of one side other than the active one. Together with the active panel they
/// form the tab row: `others` in order with the active panel inserted at `position`.
#[derive(Debug, Default)]
struct ParkedTabs {
    others: Vec<Panel>,
    position: usize,
}

/// What the UI needs to draw one side's tab row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStrip {
    pub titles: Vec<String>,
    /// Full directory of each tab, for tooltips.
    pub paths: Vec<PathBuf>,
    /// Index of the active tab in `titles`.
    pub active: usize,
}

/// Resolves `.` and `..` components lexically, like `cd` in a shell (symlinks are not
/// followed), so `a/b/..` becomes `a` and the panel never shows a path containing `..`.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never climb above the root (or a Windows drive prefix).
                if !matches!(
                    out.components().next_back(),
                    Some(Component::RootDir | Component::Prefix(_)) | None
                ) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Short label for a tab: the directory name, or the whole path for a root.
fn tab_title(cwd: &Path) -> String {
    cwd.file_name().map_or_else(
        || cwd.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The whole application state except the UI: two panels, which one is active,
/// the key map and the last status message.
#[derive(Debug)]
pub struct Session {
    /// The active tab of each side.
    pub panels: [Panel; 2],
    pub active: usize,
    pub keymap: Keymap,
    pub status: String,
    /// True when `status` reports a problem (the UI shows it in red).
    pub status_is_error: bool,
    /// Mounted volumes, re-read on `refresh`.
    pub volumes: Vec<Volume>,
    /// Set while the user is asked a question; the question text is in `status`.
    pub pending: Option<Pending>,
    /// Answer conflicts automatically instead of asking (scripts and the CLI).
    pub auto_resolve: Option<Resolution>,
    /// The open file viewer (F3); while it is set the UI shows it instead of the panels.
    pub viewer: Option<Viewer>,
    /// The synchronization window (Alt+S); while it is set the UI shows it.
    pub sync: Option<SyncView>,
    /// The multi-rename window (Ctrl+F2); while it is set the UI shows it.
    pub rename: Option<crate::RenameView>,
    /// The search window (Alt+F7). Its search runs on its own thread, apart from the job
    /// queue, so copying and the like keep working meanwhile.
    pub find: Option<crate::FindView>,
    /// A document shown as pages (F3 on a PDF; Word, Excel, OpenDocument through LibreOffice).
    pub pages: Option<crate::PageViewer>,
    /// The network scanner window (Ctrl+N).
    pub scan: Option<crate::ScanView>,
    /// How the scanner starts programs and asks for shares (tests replace it).
    pub scan_services: crate::ScanServices,
    /// Where the settings files live (set when they are loaded).
    pub(crate) config_dir: Option<PathBuf>,
    /// The colour theme (a name from `THEMES`).
    pub(crate) theme: String,
    /// What `settings.conf` holds (theme and changed keys); written back on every change.
    pub(crate) settings: crate::settings::Settings,
    /// Folder sizes counted on request.
    pub dir_sizes: crate::DirSizes,
    /// Cached footer text and bar scale of each panel (see `panel_stats`).
    pub(crate) stats_cache:
        std::sync::Mutex<[Option<(crate::dirsize::StatsKey, crate::PanelStats)>; 2]>,
    /// Free space of the drives shown above the panels (asked in the background).
    pub disk_usage: crate::DiskUsage,
    /// Quick View (Ctrl+Q): the preview in the other panel, when it is on.
    pub quick: Option<crate::QuickView>,
    /// A preview waiting for the cursor to rest on a file.
    pub(crate) quick_want: Option<(crate::QuickKey, std::time::Instant)>,
    pub(crate) quick_delay: std::time::Duration,
    /// Saved SFTP sites (the site manager) and the server keys the user has trusted.
    pub sites: bluepane_remote::Sites,
    pub known_hosts: bluepane_remote::KnownHosts,
    /// Where passwords of saved sites are read from and saved to besides the sites file
    /// (the system password store, Bitwarden). Tests put fakes here.
    pub secrets: std::sync::Arc<dyn bluepane_remote::SecretServices>,
    /// How a file is opened for editing (tests replace it).
    pub editor: Editor,
    pub(crate) remote_edit: Option<RemoteEdit>,
    /// The queue is paused: the running job finishes, the next ones wait.
    pub paused: bool,
    /// Bytes per second the jobs may move (0 = no limit); shared with running jobs.
    pub(crate) speed_limit: Arc<std::sync::atomic::AtomicU64>,
    /// Text the front end should put on the system clipboard (taken by `take_clipboard`).
    clipboard: Option<String>,
    /// The queue window is open (Ctrl+J).
    pub queue_open: bool,
    /// The command palette, while it is open (F1).
    pub palette: Option<crate::Palette>,
    /// A folder to open once the connection to the named site is up (`go sftp://host/dir`).
    pub(crate) after_connect: Option<(String, String)>,
    /// Thumbnails of pictures for the grid view.
    pub thumbnails: crate::Thumbnails,
    /// The key cheat sheet is shown, with the prefix of the keys it lists (`""` = all).
    pub key_sheet_open: Option<String>,
    /// The `menu` command asked for the context menu (the front end takes the request).
    pub menu_requested: bool,
    /// The Places bar is shown (Ctrl+B).
    pub places_open: bool,
    /// The favorite folders (`fav`).
    pub favorites: crate::Favorites,
    /// The "jump to" list, while it is open (Ctrl+P).
    pub jump: Option<crate::Palette>,
    /// The folders visited so far (for the "jump to" ranking).
    pub visits: crate::Visits,
    pub(crate) last_visit: [Option<PathBuf>; 2],
    /// Commands chosen in the palette, newest first.
    pub recent_commands: Vec<String>,
    /// Text the front end should put in the command line (taken with `take`).
    pub command_prefill: Option<String>,
    /// Jobs that ended and were not shown to the user yet (`take_notices`).
    pub(crate) notices: Vec<crate::Notice>,
    /// The last finished jobs, newest first (for "retry" in the queue window).
    pub finished: VecDeque<crate::FinishedJob>,
    /// The site manager window is open.
    pub sites_open: bool,
    pub(crate) config_loaded: bool,
    parked: [ParkedTabs; 2],
    current: Option<RunningJob>,
    queue: VecDeque<JobSpec>,
    /// The archive job that needs a password; run again once the user types it.
    retry: Option<JobSpec>,
    /// Largest archive entry that F3 extracts for a preview.
    pub view_from_archive_limit: u64,
    /// Temporary directory holding the file being previewed from an archive.
    pub(crate) preview_dir: Option<PathBuf>,
}

/// A file inside an archive larger than this is not previewed (it would have to be extracted
/// whole into a temporary directory first): the user extracts it with F5 instead.
pub const VIEW_FROM_ARCHIVE_LIMIT: u64 = 256 * 1024 * 1024;

/// The synchronization window: the two folders, the settings and what a run would do.
#[derive(Debug, Clone)]
pub struct SyncView {
    pub left: PathBuf,
    pub right: PathBuf,
    pub settings: SyncSettings,
    /// The filter patterns as typed (the compiled filter lives in `settings`).
    pub include: String,
    pub exclude: String,
    /// `None` while the comparison is still running.
    pub preview: Option<Arc<SyncPreview>>,
    /// What the last run did, for the window to show.
    pub summary: Option<String>,
}

/// Counter that keeps the temporary preview directories of one process apart.
static PREVIEW_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A file of a server that is being edited as a local copy.
#[derive(Clone)]
pub(crate) struct RemoteEdit {
    pub(crate) conn: std::sync::Arc<dyn bluepane_remote::RemoteFs>,
    pub(crate) remote_path: String,
    pub(crate) local: PathBuf,
    pub(crate) dir: PathBuf,
    /// Size and time of the copy as downloaded, to tell whether it was changed.
    pub(crate) size: u64,
    pub(crate) modified: Option<std::time::SystemTime>,
}

impl std::fmt::Debug for RemoteEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RemoteEdit({})", self.remote_path)
    }
}

/// How a file is opened for editing.
#[derive(Clone)]
pub struct Editor(pub std::sync::Arc<EditorFn>);

/// What opens a file in an editor: gets the path, says why not on failure.
pub type EditorFn = dyn Fn(&Path) -> Result<(), String> + Send + Sync;

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Editor")
    }
}

/// Opens a file in the user's editor: `BLUEPANE_EDITOR`, else the system's default program.
fn open_in_editor(path: &Path) -> Result<(), String> {
    let editor = std::env::var("BLUEPANE_EDITOR").ok();
    bluepane_vfs::open_external(path, editor.as_deref()).map_err(|e| e.to_string())
}

/// Work that must not wait behind a paused queue: it only serves the interface (a listing, a
/// login, saving a password).
fn is_urgent(spec: &JobSpec) -> bool {
    spec.archive
        .as_ref()
        .and_then(|a| a.remote.as_ref())
        .is_some_and(|r| {
            matches!(
                r.op,
                crate::job::RemoteOp::List { .. }
                    | crate::job::RemoteOp::Connect { .. }
                    | crate::job::RemoteOp::SaveSecret { .. }
            )
        })
}

/// A re-read of a server folder: it says nothing on the status line.
fn is_quiet_listing(spec: &JobSpec) -> bool {
    spec.archive
        .as_ref()
        .and_then(|a| a.remote.as_ref())
        .is_some_and(|r| matches!(r.op, crate::job::RemoteOp::List { .. }))
}

/// A number that keeps the temporary preview folders of one process apart.
pub(crate) fn next_preview_number() -> u64 {
    PREVIEW_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Index of the volume that contains `cwd` (the one with the longest matching path).
fn volume_index(volumes: &[Volume], cwd: &Path) -> Option<usize> {
    volumes
        .iter()
        .enumerate()
        .filter(|(_, v)| cwd.starts_with(&v.path))
        .max_by_key(|(_, v)| v.path.components().count())
        .map(|(i, _)| i)
}

impl Session {
    pub fn new(left: impl Into<PathBuf>, right: impl Into<PathBuf>) -> Self {
        Session {
            panels: [Panel::new(left), Panel::new(right)],
            active: 0,
            keymap: Keymap::default(),
            status: String::new(),
            status_is_error: false,
            volumes: bluepane_vfs::volumes(),
            pending: None,
            auto_resolve: None,
            viewer: None,
            sync: None,
            rename: None,
            find: None,
            pages: None,
            scan: None,
            scan_services: crate::ScanServices::default(),
            config_dir: None,
            theme: crate::settings::DEFAULT_THEME.to_string(),
            settings: crate::settings::Settings::default(),
            dir_sizes: crate::DirSizes::default(),
            stats_cache: std::sync::Mutex::default(),
            disk_usage: crate::DiskUsage::default(),
            quick: None,
            quick_want: None,
            quick_delay: crate::QUICK_DELAY,
            sites: bluepane_remote::Sites::in_memory(),
            known_hosts: bluepane_remote::KnownHosts::in_memory(),
            secrets: std::sync::Arc::new(bluepane_remote::SystemServices::default()),
            editor: Editor(std::sync::Arc::new(open_in_editor)),
            remote_edit: None,
            paused: false,
            speed_limit: Arc::default(),
            clipboard: None,
            queue_open: false,
            palette: None,
            jump: None,
            after_connect: None,
            menu_requested: false,
            key_sheet_open: None,
            thumbnails: crate::Thumbnails::default(),
            places_open: false,
            favorites: crate::Favorites::default(),
            visits: crate::Visits::default(),
            last_visit: [None, None],
            recent_commands: Vec::new(),
            command_prefill: None,
            notices: Vec::new(),
            finished: VecDeque::new(),
            sites_open: false,
            config_loaded: false,
            parked: [ParkedTabs::default(), ParkedTabs::default()],
            current: None,
            queue: VecDeque::new(),
            retry: None,
            view_from_archive_limit: VIEW_FROM_ARCHIVE_LIMIT,
            preview_dir: None,
        }
    }

    /// Index into `volumes` of the volume the given panel is on.
    pub fn volume_of(&self, panel: usize) -> Option<usize> {
        volume_index(&self.volumes, &self.panels[panel].cwd)
    }

    pub fn active_panel(&self) -> &Panel {
        &self.panels[self.active]
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        &mut self.panels[self.active]
    }

    /// The text a `copynames` command asked to put on the clipboard, once. The window copies it
    /// to the system clipboard (the engine knows nothing about the toolkit).
    pub(crate) fn put_on_clipboard(&mut self, text: String) {
        self.clipboard = Some(text);
    }

    pub fn config_dir(&self) -> Option<PathBuf> {
        self.config_dir.clone()
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    /// `copynames`: the selection (or the entry under the cursor) as text, one per line.
    fn copy_names(&mut self, format: NameFormat) -> Result<(), EngineError> {
        let panel = self.active_panel();
        let lines: Vec<String> = panel
            .target_entries()
            .into_iter()
            .map(|entry| match format {
                NameFormat::Stem => name_parts(&entry.name, entry.is_dir()).0.to_string(),
                NameFormat::File => entry.name.clone(),
                NameFormat::Path => entry.path.to_string_lossy().into_owned(),
            })
            .collect();
        if lines.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        let what = match format {
            NameFormat::Stem => "nazw bez rozszerzeń",
            NameFormat::File => "nazw z rozszerzeniami",
            NameFormat::Path => "pełnych ścieżek",
        };
        self.status = format!("Skopiowano do schowka: {} {what}", lines.len());
        self.clipboard = Some(lines.join("\n"));
        Ok(())
    }

    /// Parses and executes a command line. On error the message is also put in `status`.
    pub fn run_line(&mut self, line: &str) -> Result<(), EngineError> {
        match Command::parse(line) {
            Ok(command) => self.execute(command),
            Err(err) => {
                self.status = err.0.clone();
                self.status_is_error = true;
                Err(err)
            }
        }
    }

    /// Executes a command. Long operations are queued as background jobs: call [`poll`]
    /// regularly (every frame) to collect their progress and results.
    ///
    /// [`poll`]: Session::poll
    pub fn execute(&mut self, command: Command) -> Result<(), EngineError> {
        let result = self.execute_inner(command);
        self.note_visits();
        match &result {
            Ok(()) => self.status_is_error = false,
            Err(err) => {
                self.status = err.0.clone();
                self.status_is_error = true;
            }
        }
        result
    }

    fn execute_inner(&mut self, command: Command) -> Result<(), EngineError> {
        // Any other command drops an unanswered question, except a conflict: the worker
        // thread is blocked on it and only an answer (or `stop`) may release it.
        let answers = matches!(
            command,
            Command::Confirm | Command::Cancel | Command::Resolve(..) | Command::StopJob
        );
        if !answers && !matches!(self.pending, Some(Pending::Conflict(_))) {
            self.pending = None;
            self.retry = None;
        }
        match command {
            Command::Confirm => self.confirm()?,
            Command::Cancel => self.cancel_question(),
            Command::Resolve(resolution, for_all) => self.resolve(resolution, for_all)?,
            Command::StopJob => self.stop_jobs(),
            Command::Sync(args) => self.start_sync(args)?,
            Command::SyncRun => self.run_sync()?,
            Command::SyncSet(n, direction) => self.set_sync_direction(n, direction)?,
            Command::SyncClose => self.sync = None,
            Command::Sites => self.open_sites(),
            Command::SiteSet(name, field, value) => self.site_set(&name, &field, &value)?,
            Command::SiteDelete(name) => self.site_delete(&name)?,
            Command::EditSend => self.remote_edit_send()?,
            Command::Page(target) => self.page_move(target)?,
            Command::PageZoom(change) => self.page_zoom(change)?,
            Command::PageClose => self.page_close(),
            Command::CopyNames(format) => self.copy_names(format)?,
            Command::Scan => self.open_scan(),
            Command::ScanSet(field, value) => self.scan_set(&field, &value)?,
            Command::ScanRun => self.scan_run()?,
            Command::ScanStop => self.scan_stop(),
            Command::ScanClose => self.scan_close(),
            Command::ScanSelect(ip) => self.scan_select(ip)?,
            Command::ScanFavorite(ip) => self.scan_favorite(ip)?,
            Command::ScanFavorites(show) => {
                if let Some(view) = self.scan.as_mut() {
                    view.show_favorites = show;
                }
            }
            Command::ScanCopy(ip) => self.scan_copy(ip),
            Command::ScanShares(ip) => self.scan_shares(ip)?,
            Command::ScanShare(ip, name) => self.scan_share(ip, &name)?,
            Command::ScanOpen(ip, port) => self.scan_open(ip, port)?,
            Command::ScanSite(ip, protocol) => self.scan_site(ip, protocol)?,
            Command::Palette(action) => match action {
                crate::PaletteAction::Query(text) => self.palette_set_query(text),
                crate::PaletteAction::Close => self.palette = None,
                crate::PaletteAction::Move(by) => self.palette_move(by),
                crate::PaletteAction::Run => self.palette_run()?,
            },
            Command::Jump(action) => match action {
                crate::PaletteAction::Query(text) => self.jump_set_query(text),
                crate::PaletteAction::Close => self.jump = None,
                crate::PaletteAction::Move(by) => self.jump_move(by),
                crate::PaletteAction::Run => self.jump_run()?,
            },
            Command::Go(text) => self.omnibox(&text)?,
            Command::DragTo(remove, target) => self.drag_to(remove, target)?,
            Command::Drop(target, paths) => self.drop_files(target, paths)?,
            Command::PanelView(mode) => {
                let panel = self.active_panel_mut();
                panel.view = mode.unwrap_or(match panel.view {
                    crate::PanelView::List => crate::PanelView::Grid,
                    crate::PanelView::Grid => crate::PanelView::List,
                });
                self.status = match self.active_panel().view {
                    crate::PanelView::List => "Widok: lista".into(),
                    crate::PanelView::Grid => "Widok: miniatury".into(),
                };
            }
            Command::Keys(prefix) => {
                let prefix = prefix.unwrap_or_default();
                self.key_sheet_open = match &self.key_sheet_open {
                    Some(open) if *open == prefix => None,
                    _ => Some(prefix),
                };
            }
            Command::Menu => self.menu_requested = true,
            Command::Open => self.open_default()?,
            Command::Info => self.show_info()?,
            Command::Places => self.places_open = !self.places_open,
            Command::Favorite(action) => self.favorite_action(action)?,
            Command::Queue => self.queue_open = true,
            Command::QueueClose => self.queue_open = false,
            Command::QueuePause => self.pause_queue(true),
            Command::QueueResume => self.pause_queue(false),
            Command::QueueDelete(n) => self.queue_delete(n)?,
            Command::QueueMove(n, to) => self.queue_move(n, to)?,
            Command::Limit(limit) => self.set_speed_limit(limit),
            Command::Theme(name) => self.set_theme(name)?,
            Command::DirSize => self.count_dir_sizes()?,
            Command::Quick(action) => self.quick_action(action)?,
            Command::Bind(args) => self.bind_key(args)?,
            Command::Unbind(key) => self.unbind_key(&key)?,
            Command::QueueRetry(n) => self.queue_retry(n)?,
            Command::QueueClear => self.finished.clear(),
            Command::SitePassword(name, password) => self.site_password(&name, password)?,
            Command::SiteClose => self.sites_open = false,
            Command::Connect(name, password) => self.connect_site(&name, password)?,
            Command::Disconnect => self.disconnect()?,
            Command::Find => self.open_find(),
            Command::FindSet(field, value) => self.find_set(&field, &value)?,
            Command::FindRun => self.find_run()?,
            Command::FindStop => self.find_stop(),
            Command::FindGo(n) => self.find_go(n)?,
            Command::FindView(n) => self.find_view(n)?,
            Command::FindClose => self.find_close(),
            Command::FindPanel => self.find_panel()?,
            Command::MultiRename => self.start_multirename()?,
            Command::MultiRenameSet(field, value) => self.multirename_set(&field, &value)?,
            Command::MultiRenameName(n, name) => self.multirename_name(n, name)?,
            Command::MultiRenameRun => self.multirename_run()?,
            Command::MultiRenameUndo => self.multirename_undo()?,
            Command::MultiRenameClose => self.rename = None,
            Command::Click(row, mode) => {
                if row > self.active_panel().entries.len() {
                    return Err(EngineError(format!("click: nie ma wiersza {row}")));
                }
                self.active_panel_mut().click(row - 1, mode);
            }
            Command::Tab(action) => self.tab_action(action)?,
            Command::Pack(args) => self.pack(args)?,
            Command::Unpack(args) => self.unpack(&args)?,
            Command::TestArchive(password) => self.test_archive(password)?,
            Command::View => self.open_viewer()?,
            Command::Edit => self.edit_file()?,
            Command::Viewer(action) => self.viewer_action(action)?,
            Command::SwitchPanel => self.active = 1 - self.active,
            Command::Up => {
                let panel = self.active_panel_mut();
                if !panel.filter_backspace() {
                    if panel.remote.is_some() {
                        self.remote_up();
                    } else {
                        self.active_panel_mut().up();
                    }
                }
            }
            Command::Enter => self.enter_current(),
            Command::Cursor(delta) => self.active_panel_mut().move_cursor(delta),
            Command::ToggleSelect => self.active_panel_mut().toggle_select(),
            Command::Refresh => {
                self.dir_sizes.clear();
                self.volumes = bluepane_vfs::volumes();
                self.panels.iter_mut().for_each(Panel::reload);
                self.remote_refresh_all();
            }
            Command::Back if self.active_panel().remote.is_some() => self.remote_history(-1)?,
            Command::Forward if self.active_panel().remote.is_some() => self.remote_history(1)?,
            Command::Back => self.active_panel_mut().back()?,
            Command::Forward => self.active_panel_mut().forward()?,
            Command::SelectMask(text) => self.select_by_mask(&text, true)?,
            Command::UnselectMask(text) => self.select_by_mask(&text, false)?,
            Command::SelectAll => {
                let count = self.active_panel_mut().select_all();
                self.status = format!("Zaznaczono: {count}");
            }
            Command::UnselectAll => {
                self.active_panel_mut().unselect_all();
                self.status = "Odznaczono wszystko".into();
            }
            Command::InvertSelection => {
                self.active_panel_mut().invert_selection();
                self.status = format!("Zaznaczono: {}", self.active_panel().selected.len());
            }
            Command::Sort(key, descending) => self.active_panel_mut().set_sort(key, descending),
            Command::Filter(text) => self.active_panel_mut().set_filter(&text),
            Command::Volume(which) => self.go_to_volume(&which)?,
            Command::Select(name) => {
                let panel = self.active_panel_mut();
                let Some(index) = panel
                    .entries
                    .iter()
                    .position(|e| e.name == name && !is_parent_entry(e))
                else {
                    return Err(EngineError(format!("brak elementu: {name}")));
                };
                panel.cursor = index;
                panel.selected.insert(panel.entries[index].path.clone());
            }
            Command::Cd(path) if self.active_panel().remote.is_some() => {
                self.remote_cd(&path.to_string_lossy())?;
            }
            Command::Cd(path) => {
                let panel = self.active_panel_mut();
                let target = normalize_path(&if path.is_absolute() {
                    path
                } else {
                    panel.cwd.join(path)
                });
                if !target.is_dir() {
                    return Err(EngineError(format!("to nie katalog: {}", target.display())));
                }
                panel.change_dir(target);
            }
            Command::Copy => self.transfer(false)?,
            Command::Move => self.transfer(true)?,
            Command::Mkdir(Some(name)) => self.make_dir(&name)?,
            Command::Mkdir(None) => {
                self.status = "Nazwa nowego katalogu (Enter = utwórz, Esc = anuluj)".into();
                self.pending = Some(Pending::Input {
                    kind: InputKind::Mkdir,
                    text: String::new(),
                });
            }
            Command::Rename(Some(name)) => {
                let path = self.entry_to_rename()?;
                self.rename_entry(&path, &name)?;
            }
            Command::Rename(None) => {
                let path = self.entry_to_rename()?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.status = "Nowa nazwa (Enter = zmień, Esc = anuluj)".into();
                self.pending = Some(Pending::Input {
                    kind: InputKind::Rename(path),
                    text: name,
                });
            }
            Command::Chmod(mode) => self.remote_chmod(mode)?,
            Command::Delete => self.propose_delete()?,
        }
        Ok(())
    }

    fn select_by_mask(&mut self, text: &str, select: bool) -> Result<(), EngineError> {
        let mask = Mask::parse(text).ok_or_else(|| EngineError("pusta maska".into()))?;
        let count = self.active_panel_mut().select_matching(&mask, select);
        let verb = if select { "Zaznaczono" } else { "Odznaczono" };
        self.status = format!("{verb}: {count} ({text})");
        Ok(())
    }

    /// Jumps the active panel to a volume given as a 1-based number or a name.
    fn go_to_volume(&mut self, which: &str) -> Result<(), EngineError> {
        let found = match which.parse::<usize>() {
            Ok(n) => n.checked_sub(1).and_then(|i| self.volumes.get(i)),
            Err(_) => self
                .volumes
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(which) || v.path == Path::new(which)),
        };
        let Some(volume) = found else {
            let known: Vec<String> = self
                .volumes
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{} {}", i + 1, v.name))
                .collect();
            return Err(EngineError(format!(
                "nie ma wolumenu: {which} (dostępne: {})",
                known.join(", ")
            )));
        };
        let path = volume.path.clone();
        if !path.is_dir() {
            return Err(EngineError(format!(
                "wolumen niedostępny: {}",
                path.display()
            )));
        }
        self.active_panel_mut().change_dir(path);
        Ok(())
    }

    /// Copy or move the active panel's targets into the other panel's directory.
    pub(crate) fn transfer(&mut self, remove_source: bool) -> Result<(), EngineError> {
        if self.panels.iter().any(|p| p.remote.is_some()) {
            return self.remote_transfer(remove_source);
        }
        if self.panels[1 - self.active].archive.is_some() {
            if self.active_panel().archive.is_some() {
                return Err(EngineError(
                    "nie można kopiować z archiwum do archiwum (najpierw rozpakuj)".into(),
                ));
            }
            if remove_source {
                return Err(EngineError(
                    "przenoszenie do archiwum niedostępne (F5 dodaje pliki do archiwum)".into(),
                ));
            }
            return self.add_to_archive();
        }
        if self.active_panel().archive.is_some() {
            if remove_source {
                return Err(EngineError(
                    "archiwum jest tylko do odczytu: przenoszenie niedostępne (F5 rozpakowuje)"
                        .into(),
                ));
            }
            return self.extract_selection();
        }
        let source = self.active_panel();
        if self.panels[1 - self.active].virtual_list.is_some() {
            return Err(EngineError(
                "drugi panel pokazuje listę wyników: wybierz w nim zwykły katalog".into(),
            ));
        }
        let dest = self.panels[1 - self.active].cwd.clone();
        let targets = source.targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        if dest == source.cwd && source.virtual_list.is_none() {
            if remove_source {
                return Err(EngineError("oba panele wskazują ten sam katalog".into()));
            }
            return self.propose_duplicates(targets);
        }
        let kind = if remove_source {
            JobKind::Move
        } else {
            JobKind::Copy
        };
        self.enqueue(JobSpec {
            kind,
            sources: targets,
            dest: Some(dest),
            archive: None,
        });
        Ok(())
    }

    /// F5 with the same directory in both panels: ask before making `(kopia)` duplicates.
    fn propose_duplicates(&mut self, targets: Vec<PathBuf>) -> Result<(), EngineError> {
        let first = ops::duplicate_path(&targets[0]).map_err(|e| EngineError(e.to_string()))?;
        let first_name = first
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = if targets.len() == 1 {
            format!("Oba panele pokazują ten sam katalog. Utworzyć kopię „{first_name}”? (Enter = tak, Esc = nie)")
        } else {
            format!(
                "Oba panele pokazują ten sam katalog. Utworzyć kopie ({}) z dopiskiem „(kopia)”, np. „{first_name}”? (Enter = tak, Esc = nie)",
                targets.len()
            )
        };
        self.pending = Some(Pending::Duplicate(targets));
        Ok(())
    }

    /// F8: ask first, then move to the trash.
    fn propose_delete(&mut self) -> Result<(), EngineError> {
        if self.active_panel().remote.is_some() {
            return self.remote_propose_delete();
        }
        if self.active_panel().archive.is_some() {
            return self.propose_archive_removal();
        }
        let targets = self.active_panel().targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        self.status = if let [only] = targets.as_slice() {
            let name = only
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("Przenieść do kosza „{name}”? (Enter = tak, Esc = nie)")
        } else {
            format!(
                "Przenieść do kosza zaznaczone elementy ({})? (Enter = tak, Esc = nie)",
                targets.len()
            )
        };
        self.pending = Some(Pending::ConfirmDelete(targets));
        Ok(())
    }

    /// Accepts the open question (`confirm`, Enter).
    fn confirm(&mut self) -> Result<(), EngineError> {
        match self.pending.take() {
            Some(Pending::Duplicate(sources)) => self.enqueue(JobSpec {
                kind: JobKind::Duplicate,
                sources,
                dest: None,
                archive: None,
            }),
            Some(Pending::ConfirmRemove { side, entries }) => {
                self.modify_archive(side, Change::Remove { entries })?
            }
            Some(Pending::ConfirmDelete(sources)) => self.enqueue(JobSpec {
                kind: JobKind::Delete,
                sources,
                dest: None,
                archive: None,
            }),
            Some(Pending::Input {
                kind: InputKind::ArchivePassword,
                text,
            }) => {
                if text.is_empty() {
                    self.pending = Some(Pending::Input {
                        kind: InputKind::ArchivePassword,
                        text,
                    });
                    return Err(EngineError("podaj hasło".into()));
                }
                self.retry_with_password(text)?;
            }
            Some(Pending::Input {
                kind: InputKind::PdfPassword(path),
                text,
            }) => {
                if text.is_empty() {
                    self.pending = Some(Pending::Input {
                        kind: InputKind::PdfPassword(path),
                        text,
                    });
                    return Err(EngineError("podaj hasło".into()));
                }
                self.open_local_pdf(&path, Some(&text))?;
            }
            Some(Pending::TrustHost(prompt)) => self.trust_host(prompt)?,
            Some(Pending::ConfirmRemoteDelete { side, paths }) => {
                self.remote_delete(side, paths)?
            }
            Some(Pending::Input {
                kind: InputKind::SitePassword(site),
                text,
            }) => self.connect_site(&site, Some(text))?,
            Some(Pending::Pack(mut form)) => {
                if let Err(err) = self.start_pack(&form) {
                    form.error = Some(err.0.clone());
                    self.pending = Some(Pending::Pack(form));
                    return Err(err);
                }
            }
            Some(Pending::Input { kind, text }) => {
                let result = match &kind {
                    InputKind::Mkdir => self.make_dir(&text),
                    InputKind::Rename(path) => self.rename_entry(path, &text),
                    InputKind::ViewerFind => {
                        self.viewer_action(ViewerAction::Find(Some(text.clone())))
                    }
                    InputKind::ArchivePassword
                    | InputKind::SitePassword(_)
                    | InputKind::PdfPassword(_) => {
                        unreachable!("handled in the arms above")
                    }
                };
                if let Err(err) = result {
                    // Keep the dialog open so the name can be corrected.
                    self.pending = Some(Pending::Input { kind, text });
                    return Err(err);
                }
            }
            Some(conflict @ Pending::Conflict(_)) => {
                self.pending = Some(conflict);
                return Err(EngineError(
                    "plik już istnieje: wybierz nadpisz, nadpisz starsze, pomiń, zachowaj oba lub anuluj"
                        .into(),
                ));
            }
            None => return Err(EngineError("nie ma o co pytać".into())),
        }
        Ok(())
    }

    /// Rejects the open question (`cancel`, Escape). For a conflict this stops the job.
    fn cancel_question(&mut self) {
        match self.pending.take() {
            Some(Pending::Conflict(prompt)) => {
                prompt.answer(None, false);
                self.queue.clear();
                self.status = "Anulowanie zadania…".into();
            }
            Some(other) => {
                if matches!(
                    other,
                    Pending::Input {
                        kind: InputKind::ArchivePassword,
                        ..
                    }
                ) {
                    self.retry = None;
                    self.start_next();
                }
                self.status = "Anulowano".into();
            }
            None => {}
        }
    }

    fn resolve(&mut self, resolution: Resolution, for_all: bool) -> Result<(), EngineError> {
        match self.pending.take() {
            Some(Pending::Conflict(prompt)) => {
                prompt.answer(Some(resolution), for_all);
                self.status = format!("{}…", self.current_label());
                Ok(())
            }
            other => {
                self.pending = other;
                Err(EngineError("nie ma konfliktu do rozstrzygnięcia".into()))
            }
        }
    }

    fn current_label(&self) -> &'static str {
        self.current
            .as_ref()
            .map_or("Gotowe", |job| job.spec.kind.label())
    }

    fn stop_jobs(&mut self) {
        self.queue.clear();
        let Some(job) = &self.current else {
            self.status = "Nic się nie wykonuje".into();
            return;
        };
        job.progress.cancel();
        if let Some(Pending::Conflict(prompt)) = self.pending.take() {
            prompt.answer(None, false);
        }
        self.status = "Zatrzymywanie…".into();
    }

    /// Runs the job now, or queues it behind the running one.
    pub(crate) fn enqueue(&mut self, spec: JobSpec) {
        self.active_panel_mut().selected.clear();
        self.enqueue_quiet(spec);
    }

    /// Queues a job without touching the selection (listings and connects).
    pub(crate) fn enqueue_quiet(&mut self, spec: JobSpec) {
        if self.current.is_none() && (!self.paused || is_urgent(&spec)) {
            self.start(spec);
        } else {
            self.queue.push_back(spec);
            self.status = if self.paused {
                format!("W kolejce (wstrzymana): {}", self.queue.len())
            } else {
                format!("W kolejce: {}", self.queue.len())
            };
        }
    }

    pub(crate) fn current_spec(&self) -> Option<&JobSpec> {
        self.current.as_ref().map(|job| &job.spec)
    }

    pub(crate) fn queue_specs(&self) -> impl Iterator<Item = &JobSpec> {
        self.queue.iter()
    }

    /// Takes the job at `from` out of the queue and puts it where `to` was.
    pub(crate) fn move_queued(&mut self, from: usize, to: usize) {
        if let Some(spec) = self.queue.remove(from) {
            self.queue.insert(to, spec);
        }
    }

    pub(crate) fn remove_queued(&mut self, index: usize) -> JobSpec {
        self.queue
            .remove(index)
            .expect("index checked by the caller")
    }

    /// Starts the next queued job that may run (after the queue was resumed).
    pub(crate) fn start_waiting(&mut self) {
        if let Some(next) = self.next_startable() {
            self.start(next);
        }
    }

    /// The next queued job that may start now: the first one, or while the queue is paused
    /// only work for the interface.
    fn next_startable(&mut self) -> Option<JobSpec> {
        let index = if self.paused {
            self.queue.iter().position(is_urgent)?
        } else {
            0
        };
        self.queue.remove(index)
    }

    fn start(&mut self, spec: JobSpec) {
        // A refresh of a server listing runs quietly: it must not wipe the result of the
        // transfer that caused it from the status line.
        if !is_quiet_listing(&spec) {
            self.status = format!("{}…", spec.kind.label());
            self.status_is_error = false;
        }
        self.current = Some(RunningJob::start_limited(spec, self.speed_limit.clone()));
    }

    /// True while a job runs or waits in the queue.
    pub fn is_busy(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    /// Progress of the running job, for the progress bar.
    pub fn job_status(&self) -> Option<JobStatus> {
        let job = self.current.as_ref()?;
        let progress = &job.progress;
        Some(JobStatus {
            label: job.spec.kind.label(),
            current: progress.current(),
            bytes_done: progress.bytes_done(),
            bytes_total: progress.bytes_total(),
            items_done: progress.items_done(),
            items_total: progress.items_total(),
            queued: self.queue.len(),
            waiting_for_answer: matches!(self.pending, Some(Pending::Conflict(_))),
        })
    }

    /// Collects what the background job reported since the last call: conflicts to ask
    /// about and the final result. Cheap; call it every frame.
    pub fn poll(&mut self) {
        self.poll_pages();
        self.update_quick();
        loop {
            let Some(job) = &self.current else { return };
            match job.try_event() {
                Ok(None) => return,
                Ok(Some(Event::Conflict { conflict, reply })) => {
                    let prompt = ConflictPrompt { conflict, reply };
                    if let Some(resolution) = self.auto_resolve {
                        prompt.answer(Some(resolution), true);
                        continue;
                    }
                    let name = prompt
                        .conflict
                        .dst
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.status = format!("Plik już istnieje: {name}");
                    self.status_is_error = false;
                    self.pending = Some(Pending::Conflict(prompt));
                    return;
                }
                Ok(Some(Event::Finished(report))) => self.finish_job(&report),
                Err(_) => self.finish_job(&Report {
                    failed: vec!["zadanie przerwane nieoczekiwanie".into()],
                    ..Report::default()
                }),
            }
        }
    }

    fn finish_job(&mut self, report: &Report) {
        let Some(mut job) = self.current.take() else {
            return;
        };
        job.join();
        let spec = job.spec;
        self.pending = None;
        self.panels.iter_mut().for_each(Panel::reload);

        if let Some(problem) = report.password {
            self.ask_password(spec, problem);
            return;
        }

        let (text, is_error) = if spec.kind == JobKind::Remote {
            self.remote_result(&spec, report)
        } else if matches!(spec.kind, JobKind::SyncCompare | JobKind::SyncApply) {
            self.sync_result(&spec, report)
        } else if matches!(
            spec.kind,
            JobKind::OpenArchive
                | JobKind::Extract
                | JobKind::Pack
                | JobKind::VerifyArchive
                | JobKind::ModifyArchive
        ) {
            self.archive_result(&spec, report)
        } else {
            let (verb, total) = (spec.kind.verb(), spec.sources.len());
            let mut text = match report.failed.first() {
                Some(first) => format!("{verb}: {} z {total}, błąd: {first}", report.done),
                None => format!("{verb}: {}", report.done),
            };
            if report.skipped > 0 {
                text.push_str(&format!(", pominięto: {}", report.skipped));
            }
            if report.cancelled {
                text = format!("Anulowano. {text}");
            }
            (text, !report.failed.is_empty())
        };
        self.record_finished(&spec, &text, is_error, report.cancelled);
        self.status = text;
        self.status_is_error = is_error;

        if let Some(next) = self.next_startable() {
            if is_quiet_listing(&next) {
                self.start(next);
                return;
            }
            let message = std::mem::take(&mut self.status);
            self.start(next);
            // Keep an error of the finished job visible until the next one says something.
            if is_error {
                self.status = message;
                self.status_is_error = true;
            }
        }
    }

    /// Starts the next queued job when nothing is running.
    fn start_next(&mut self) {
        if self.current.is_none() {
            if let Some(next) = self.queue.pop_front() {
                self.start(next);
            }
        }
    }

    /// An archive job needs a password: ask for it and run the job again with it.
    fn ask_password(&mut self, spec: JobSpec, problem: PasswordProblem) {
        self.retry = Some(spec);
        self.pending = Some(Pending::Input {
            kind: InputKind::ArchivePassword,
            text: String::new(),
        });
        self.status_is_error = problem == PasswordProblem::Wrong;
        self.status = match problem {
            PasswordProblem::Needed => {
                "Archiwum jest zaszyfrowane: podaj hasło (Enter = dalej, Esc = anuluj)".into()
            }
            PasswordProblem::Wrong => "Błędne hasło: spróbuj ponownie (Esc = anuluj)".into(),
        };
    }

    fn retry_with_password(&mut self, password: String) -> Result<(), EngineError> {
        let Some(mut spec) = self.retry.take() else {
            return Err(EngineError("nie ma operacji do ponowienia".into()));
        };
        let archive_path = spec.sources.first().cloned();
        if let Some(job) = spec.archive.as_mut() {
            job.password = Some(password.clone());
        }
        // Remember it for the panels that show this archive, so the next F5 does not ask again.
        for panel in &mut self.panels {
            if let Some(view) = panel.archive.as_mut() {
                if Some(&view.archive) == archive_path.as_ref() {
                    view.password = Some(password.clone());
                }
            }
        }
        self.enqueue(spec);
        Ok(())
    }

    fn sync_spec(view: &SyncView, kind: JobKind, plan: Option<Arc<SyncPreview>>) -> JobSpec {
        JobSpec {
            kind,
            sources: Vec::new(),
            dest: None,
            archive: Some(ArchiveJob {
                sync: Some(SyncJob {
                    left: view.left.clone(),
                    right: view.right.clone(),
                    settings: view.settings.clone(),
                    preview: plan,
                }),
                ..ArchiveJob::default()
            }),
        }
    }

    /// `sync`: compare the two panels' directories (nothing is written).
    fn start_sync(&mut self, args: SyncArgs) -> Result<(), EngineError> {
        self.deny_remote("synchronizacja")?;
        if self.panels.iter().any(|p| p.virtual_list.is_some()) {
            return Err(EngineError(
                "synchronizacja działa na katalogach, nie na liście wyników".into(),
            ));
        }
        if self.panels.iter().any(|p| p.archive.is_some()) {
            return Err(EngineError(
                "synchronizacja działa na katalogach, nie wewnątrz archiwum".into(),
            ));
        }
        let (left, right) = (self.panels[0].cwd.clone(), self.panels[1].cwd.clone());
        if left == right {
            return Err(EngineError("oba panele wskazują ten sam katalog".into()));
        }
        if left.starts_with(&right) || right.starts_with(&left) {
            return Err(EngineError(
                "jeden folder leży w drugim: synchronizacja zapętliłaby się".into(),
            ));
        }
        let mut view = match self.sync.take() {
            Some(v) if v.left == left && v.right == right => v,
            _ => SyncView {
                left,
                right,
                settings: SyncSettings::default(),
                include: String::new(),
                exclude: String::new(),
                preview: None,
                summary: None,
            },
        };
        if let Some(variant) = args.variant {
            view.settings.variant = variant;
        }
        if let Some(compare) = args.compare {
            view.settings.compare = compare;
        }
        if let Some(deletion) = args.deletion {
            view.settings.deletion = deletion;
        }
        if let Some(include) = args.include {
            view.include = include;
        }
        if let Some(exclude) = args.exclude {
            view.exclude = exclude;
        }
        view.settings.filter = SyncFilter::new(&view.include, &view.exclude);
        view.preview = None;
        // A manual comparison starts a fresh report.
        view.summary = None;
        let spec = Self::sync_spec(&view, JobKind::SyncCompare, None);
        self.sync = Some(view);
        self.enqueue(spec);
        Ok(())
    }

    /// `syncrun`: carry out the plan that is on the screen.
    fn run_sync(&mut self) -> Result<(), EngineError> {
        let Some(view) = &self.sync else {
            return Err(EngineError("najpierw porównaj foldery (sync)".into()));
        };
        let Some(preview) = view.preview.clone() else {
            return Err(EngineError("porównanie jeszcze trwa".into()));
        };
        if preview.plan.is_noop() {
            return Err(EngineError("nie ma nic do zsynchronizowania".into()));
        }
        let spec = Self::sync_spec(view, JobKind::SyncApply, Some(preview));
        self.enqueue(spec);
        Ok(())
    }

    /// `syncset n right|left|skip`: change the direction of one row of the plan.
    fn set_sync_direction(
        &mut self,
        n: usize,
        direction: crate::SyncDirection,
    ) -> Result<(), EngineError> {
        let Some(preview) = self.sync.as_mut().and_then(|v| v.preview.as_mut()) else {
            return Err(EngineError("nie ma porównania do zmiany".into()));
        };
        Arc::make_mut(preview)
            .plan
            .redirect(n - 1, direction)
            .map_err(EngineError)
    }

    /// The status line for a finished synchronization job; keeps the window's data current.
    fn sync_result(&mut self, spec: &JobSpec, report: &Report) -> (String, bool) {
        if let Some(first) = report.failed.first() {
            if let Some(view) = &mut self.sync {
                view.summary = Some(format!("Błąd: {first}"));
            }
            return (format!("Błąd: {first}"), true);
        }
        if report.cancelled {
            // A stopped run may have changed some files: show the real state again.
            if spec.kind == JobKind::SyncApply {
                if let Some(view) = &mut self.sync {
                    let next = Self::sync_spec(view, JobKind::SyncCompare, None);
                    view.preview = None;
                    self.queue.push_front(next);
                }
            }
            return ("Anulowano".into(), false);
        }
        match spec.kind {
            JobKind::SyncCompare => {
                let Some(preview) = report.sync_preview.clone() else {
                    return ("Nie udało się porównać folderów".into(), true);
                };
                let plan = &preview.plan;
                let mut text = if plan.actions.is_empty() {
                    "Foldery są zsynchronizowane".to_string()
                } else {
                    format!(
                        "Różnice: {} (kopiowanie: {}, usuwanie: {}, konflikty: {}), do skopiowania {}",
                        plan.actions.len(),
                        plan.copies(),
                        plan.deletions(),
                        plan.conflicts(),
                        crate::human_size(plan.bytes())
                    )
                };
                if !preview.unreadable.is_empty() {
                    text.push_str(&format!(", nieczytelne: {}", preview.unreadable.len()));
                }
                if preview.skipped_links > 0 {
                    text.push_str(&format!(
                        ", pominięto dowiązania: {}",
                        preview.skipped_links
                    ));
                }
                let is_error = !preview.unreadable.is_empty();
                if let Some(view) = &mut self.sync {
                    // Right after a run, keep saying what it did, then what is still left.
                    if let Some(summary) = &view.summary {
                        text = format!("{summary}. Pozostało: {}", text.to_lowercase());
                    }
                    view.preview = Some(preview);
                }
                (text, is_error)
            }
            _ => {
                let Some(done) = &report.sync_done else {
                    return ("Nie wykonano synchronizacji".into(), true);
                };
                let mut text = format!(
                    "Zsynchronizowano: skopiowano {}, usunięto {}",
                    done.copied, done.deleted
                );
                if done.left_alone > 0 {
                    text.push_str(&format!(", pozostawiono: {}", done.left_alone));
                }
                let failed = !done.failed.is_empty();
                if let Some(first) = done.failed.first() {
                    text.push_str(&format!(", błędy: {} ({first})", done.failed.len()));
                }
                // Show what is left to do (conflicts, failures) without another click.
                if let Some(view) = &mut self.sync {
                    view.summary = Some(text.clone());
                    let next = Self::sync_spec(view, JobKind::SyncCompare, None);
                    view.preview = None;
                    self.queue.push_front(next);
                }
                (text, failed)
            }
        }
    }

    /// The status line for a finished archive job (and, for "open", the panel switch).
    fn archive_result(&mut self, spec: &JobSpec, report: &Report) -> (String, bool) {
        let archive_name = spec
            .sources
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(first) = report.failed.first() {
            return (format!("Błąd: {first}"), true);
        }
        if report.cancelled {
            return ("Anulowano".into(), false);
        }
        match spec.kind {
            JobKind::OpenArchive => {
                let Some(listing) = report.listing.clone() else {
                    return ("Nie udało się wczytać archiwum".into(), true);
                };
                let side = spec.archive.as_ref().map_or(0, |a| a.side);
                let path = spec.sources.first().cloned().unwrap_or_default();
                let still_there = self.panels[side].archive.is_none()
                    && path.parent() == Some(self.panels[side].cwd.as_path());
                let mut text = format!(
                    "Archiwum {archive_name}: {} elementów",
                    listing.entries.len()
                );
                if listing.volumes > 1 {
                    text.push_str(&format!(", części: {}", listing.volumes));
                }
                if listing.has_encrypted() {
                    text.push_str(", zaszyfrowane (hasło przy rozpakowaniu)");
                }
                if listing.unsafe_skipped > 0 {
                    text.push_str(&format!(
                        ", pominięto niebezpieczne nazwy: {}",
                        listing.unsafe_skipped
                    ));
                }
                if still_there {
                    // A password given to read hidden names is kept for extracting too.
                    let known = spec.archive.as_ref().and_then(|a| a.password.clone());
                    self.panels[side].open_archive(path, listing, known);
                } else {
                    text.push_str(" (panel zmienił katalog: nie otwieram)");
                }
                (text, false)
            }
            JobKind::Extract if spec.archive.as_ref().is_some_and(|a| a.view) => {
                self.show_extracted_preview(spec)
            }
            JobKind::Extract => {
                let mut text = format!("Rozpakowano: {}", report.files);
                if report.skipped > 0 {
                    text.push_str(&format!(", pominięto: {}", report.skipped));
                }
                if report.left_out > 0 {
                    text.push_str(&format!(", pominięto dowiązania: {}", report.left_out));
                }
                (text, false)
            }
            JobKind::Pack => {
                if report.skipped > 0 && report.created.is_empty() {
                    return ("Archiwum już istnieje: pominięto".into(), false);
                }
                let name = report
                    .created
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut text = format!("Spakowano: {} plików do {name}", report.files);
                if report.created.len() > 1 {
                    text.push_str(&format!(" ({} części)", report.created.len()));
                }
                if report.left_out > 0 {
                    text.push_str(&format!(", pominięto dowiązania: {}", report.left_out));
                }
                (text, false)
            }
            JobKind::ModifyArchive => {
                let Some(done) = report.modified else {
                    return ("Nie zmieniono archiwum".into(), true);
                };
                let path = spec.sources.first().cloned().unwrap_or_default();
                if let Some(listing) = &report.listing {
                    for panel in &mut self.panels {
                        if panel.archive.as_ref().map(|v| &v.archive) == Some(&path) {
                            panel.replace_listing(listing.clone());
                        }
                    }
                }
                let mut parts = Vec::new();
                if done.added > 0 {
                    parts.push(format!("dodano: {}", done.added));
                }
                if done.removed > 0 {
                    parts.push(format!("usunięto: {}", done.removed));
                }
                if done.renamed > 0 {
                    parts.push(format!("zmieniono nazwę: {}", done.renamed));
                }
                if done.skipped > 0 {
                    parts.push(format!("pominięto: {}", done.skipped));
                }
                if done.left_out > 0 {
                    parts.push(format!("pominięto dowiązania: {}", done.left_out));
                }
                if parts.is_empty() {
                    parts.push("bez zmian".into());
                }
                (
                    format!("Archiwum {archive_name}: {}", parts.join(", ")),
                    false,
                )
            }
            JobKind::VerifyArchive => match &report.verify {
                Some(check) if check.damaged.is_empty() => (
                    format!(
                        "Archiwum {archive_name} jest w porządku: {} plików, {}",
                        check.files,
                        crate::human_size(check.bytes)
                    ),
                    false,
                ),
                Some(check) => (
                    format!(
                        "Uszkodzone w {archive_name}: {} ({})",
                        check.damaged.len(),
                        check.damaged[0]
                    ),
                    true,
                ),
                None => ("Nie sprawdzono archiwum".into(), true),
            },
            _ => (String::new(), false),
        }
    }

    /// Blocks until every job has finished. Returns false when it stopped because a
    /// question needs an answer first (a conflict, a password): answer it, then call this
    /// again. For scripts and tests.
    pub fn wait_idle(&mut self) -> bool {
        loop {
            self.poll();
            if !self.is_busy() {
                return true;
            }
            // Jobs are waiting but none is running: they are held back by a question.
            if matches!(self.pending, Some(Pending::Conflict(_))) || self.current.is_none() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The tab row of one side, for drawing.
    pub fn tab_strip(&self, side: usize) -> TabStrip {
        let parked = &self.parked[side];
        let mut paths: Vec<PathBuf> = parked.others.iter().map(Panel::location).collect();
        paths.insert(parked.position, self.panels[side].location());
        TabStrip {
            titles: paths.iter().map(|p| tab_title(p)).collect(),
            paths,
            active: parked.position,
        }
    }

    fn tab_count(&self, side: usize) -> usize {
        self.parked[side].others.len() + 1
    }

    fn tab_action(&mut self, action: TabAction) -> Result<(), EngineError> {
        let side = self.active;
        let count = self.tab_count(side);
        let position = self.parked[side].position;
        match action {
            TabAction::New => {
                if self.panels[side].remote.is_some() {
                    // The same server and folder in a second tab (no new connection).
                    let copy = self.panels[side].clone();
                    self.open_panel_tab(side, copy);
                } else {
                    let cwd = self.panels[side].cwd.clone();
                    self.open_tab(side, cwd);
                }
            }
            TabAction::Open => {
                let entry = self
                    .active_panel()
                    .current()
                    .filter(|e| !is_parent_entry(e) && e.is_dir())
                    .ok_or_else(|| EngineError("kursor nie wskazuje katalogu".into()))?;
                let path = entry.path.clone();
                if self.panels[side].remote.is_some() {
                    let copy = self.panels[side].clone();
                    self.open_panel_tab(side, copy);
                    self.remote_goto(side, path.to_string_lossy().into_owned())?;
                } else {
                    self.open_tab(side, path);
                }
            }
            TabAction::Close(which) => {
                let index = which.map_or(position, |n| n - 1);
                self.close_tab(side, index)?;
            }
            TabAction::Next => self.switch_tab(side, (position + 1) % count),
            TabAction::Prev => self.switch_tab(side, (position + count - 1) % count),
            TabAction::Goto(n) => {
                if n > count {
                    return Err(EngineError(format!("nie ma zakładki {n} (jest {count})")));
                }
                self.switch_tab(side, n - 1);
            }
        }
        let strip = self.tab_strip(side);
        self.status = format!("Zakładka {} z {}", strip.active + 1, strip.titles.len());
        Ok(())
    }

    /// Makes tab `index` (0-based, in tab-row order) the active one of `side`.
    fn switch_tab(&mut self, side: usize, index: usize) {
        let parked = &mut self.parked[side];
        if index == parked.position {
            return;
        }
        let current = std::mem::replace(&mut self.panels[side], Panel::blank());
        parked.others.insert(parked.position, current);
        self.panels[side] = parked.others.remove(index);
        parked.position = index;
        // The directory may have changed while the tab was in the background.
        self.panels[side].reload();
        if let Some(path) = self.panels[side].remote.as_ref().map(|r| r.path.clone()) {
            let _ = self.remote_goto(side, path);
        }
    }

    /// Opens a new tab for `cwd` right after the active one and makes it active.
    fn open_tab(&mut self, side: usize, cwd: PathBuf) {
        self.open_panel_tab(side, Panel::new(cwd));
    }

    fn open_panel_tab(&mut self, side: usize, panel: Panel) {
        let parked = &mut self.parked[side];
        let current = std::mem::replace(&mut self.panels[side], panel);
        parked.others.insert(parked.position, current);
        parked.position += 1;
    }

    fn close_tab(&mut self, side: usize, index: usize) -> Result<(), EngineError> {
        let count = self.tab_count(side);
        if count == 1 {
            return Err(EngineError("to jedyna zakładka".into()));
        }
        if index >= count {
            return Err(EngineError(format!(
                "nie ma zakładki {} (jest {count})",
                index + 1
            )));
        }
        let parked = &mut self.parked[side];
        if index == parked.position {
            // Closing the active tab activates its right neighbour, or the left one at the end.
            let take = if index < parked.others.len() {
                index
            } else {
                index - 1
            };
            self.panels[side] = parked.others.remove(take);
            parked.position = take;
            self.panels[side].reload();
        } else if index < parked.position {
            parked.others.remove(index);
            parked.position -= 1;
        } else {
            parked.others.remove(index - 1);
        }
        Ok(())
    }

    /// The file under the cursor, for view and edit: never `..`, never a directory.
    pub(crate) fn entry_to_open(&self) -> Result<PathBuf, EngineError> {
        let entry = self
            .active_panel()
            .current()
            .filter(|e| !is_parent_entry(e))
            .ok_or_else(|| EngineError("nic nie wskazano".into()))?;
        if entry.is_dir() {
            return Err(EngineError(format!("to katalog, nie plik: {}", entry.name)));
        }
        Ok(entry.path.clone())
    }

    /// F3: opens the built-in viewer on the file under the cursor.
    fn open_viewer(&mut self) -> Result<(), EngineError> {
        if self.active_panel().remote.is_some() {
            return self.remote_view();
        }
        if self.active_panel().archive.is_some() {
            return self.preview_from_archive();
        }
        let path = self.entry_to_open()?;
        if bluepane_docs::is_picture(&path) {
            // A file named .png that is not a picture still opens as text / hex below.
            if let Ok(viewer) = crate::PageViewer::from_picture(&path) {
                self.show_pages(viewer);
                return Ok(());
            }
        }
        match bluepane_docs::kind_of(&path) {
            Some(bluepane_docs::Kind::Pdf) => return self.open_local_pdf(&path, None),
            // Word, Excel, OpenDocument: pages through LibreOffice when it is there, the
            // text of the document when it is not.
            Some(_) if self.open_office_pages(&path) => return Ok(()),
            _ => {}
        }
        let viewer = self.viewer_for_local(&path)?;
        self.status = format!(
            "Podgląd: {}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        self.viewer = Some(viewer);
        Ok(())
    }

    /// F4: opens the file in `BLUEPANE_EDITOR`, or the system's default application.
    fn edit_file(&mut self) -> Result<(), EngineError> {
        if self.active_panel().remote.is_some() {
            return self.remote_edit_start();
        }
        self.deny_remote("edycja (F4)")?;
        self.deny_in_archive("edycja")?;
        let path = self.entry_to_open()?;
        let editor = std::env::var("BLUEPANE_EDITOR").ok();
        bluepane_vfs::open_external(&path, editor.as_deref())
            .map_err(|e| EngineError(format!("nie udało się otworzyć edytora: {e}")))?;
        self.status = format!(
            "Otwarto w edytorze: {}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        Ok(())
    }

    fn viewer_action(&mut self, action: ViewerAction) -> Result<(), EngineError> {
        let Some(viewer) = self.viewer.as_mut() else {
            return Err(EngineError("podgląd nie jest otwarty (F3)".into()));
        };
        match action {
            ViewerAction::Scroll(rows) => viewer.scroll(rows),
            ViewerAction::Page(pages) => viewer.page(pages),
            ViewerAction::HScroll(columns) => viewer.scroll_columns(columns),
            ViewerAction::Top => viewer.top(),
            ViewerAction::Bottom => viewer.bottom(),
            ViewerAction::Mode => viewer.toggle_mode(),
            ViewerAction::Goto(spec) => viewer.goto(&spec)?,
            ViewerAction::Find(Some(text)) => {
                viewer.find(&text, true);
                self.status = viewer.message.clone();
            }
            ViewerAction::Find(None) => {
                let text = viewer.last_search().unwrap_or_default().to_string();
                self.status = "Szukaj w pliku (Enter = szukaj, Esc = anuluj)".into();
                self.pending = Some(Pending::Input {
                    kind: InputKind::ViewerFind,
                    text,
                });
            }
            ViewerAction::Next | ViewerAction::Prev => {
                viewer.find_again(action == ViewerAction::Next);
                self.status = viewer.message.clone();
            }
            ViewerAction::Close => {
                self.viewer = None;
                self.remove_preview_dir();
            }
        }
        Ok(())
    }

    /// F3 inside an archive: extract the file to a temporary directory, then view it there.
    fn preview_from_archive(&mut self) -> Result<(), EngineError> {
        let panel = self.active_panel();
        let Some(view) = panel.archive.clone() else {
            return Err(EngineError("to nie jest archiwum".into()));
        };
        let entry = panel
            .current()
            .filter(|e| !is_parent_entry(e))
            .ok_or_else(|| EngineError("nic nie wskazano".into()))?;
        if entry.is_dir() {
            return Err(EngineError(format!("to katalog, nie plik: {}", entry.name)));
        }
        if entry.size > self.view_from_archive_limit {
            return Err(EngineError(format!(
                "plik w archiwum ma {}: za duży na podgląd (limit {}), rozpakuj go (F5)",
                crate::human_size(entry.size),
                crate::human_size(self.view_from_archive_limit)
            )));
        }
        let inner = view.inner_path_of(entry).unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "bluepane-view-{}-{}",
            std::process::id(),
            PREVIEW_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        self.enqueue(JobSpec {
            kind: JobKind::Extract,
            sources: vec![view.archive.clone()],
            dest: Some(dir),
            archive: Some(ArchiveJob {
                entries: vec![inner],
                strip: view.inner.clone(),
                password: view.password.clone(),
                listing: Some(view.listing.clone()),
                view: true,
                ..ArchiveJob::default()
            }),
        });
        Ok(())
    }

    /// The preview file is out of the archive: open the viewer on it.
    fn show_extracted_preview(&mut self, spec: &JobSpec) -> (String, bool) {
        let dir = spec.dest.clone().unwrap_or_default();
        let file = spec
            .archive
            .as_ref()
            .and_then(|a| a.entries.first())
            .and_then(|entry| {
                let strip = spec.archive.as_ref().map_or("", |a| a.strip.as_str());
                let relative = if strip.is_empty() {
                    entry.as_str()
                } else {
                    entry.strip_prefix(strip)?.trim_start_matches('/')
                };
                Some(dir.join(relative))
            })
            .unwrap_or_else(|| dir.clone());
        self.show_fetched_document(&file, dir, "z archiwum")
    }

    /// Deletes the temporary directory of the archive preview, if any.
    pub(crate) fn remove_preview_dir(&mut self) {
        if let Some(dir) = self.preview_dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Refuses write operations while the active panel shows the inside of an archive.
    pub(crate) fn deny_in_archive(&self, what: &str) -> Result<(), EngineError> {
        if self.active_panel().archive.is_some() {
            return Err(EngineError(format!(
                "{what} nie jest dostępne w archiwum (tylko do odczytu): rozpakuj je (F5 lub unpack)"
            )));
        }
        Ok(())
    }

    /// Enter: opens an archive file in the panel, or enters the directory under the cursor.
    fn enter_current(&mut self) {
        if self.active_panel().remote.is_some() {
            self.remote_enter();
            return;
        }
        let panel = self.active_panel();
        if panel.archive.is_none() {
            let archive = panel
                .current()
                .filter(|e| !e.is_dir() && !is_parent_entry(e))
                .filter(|e| bluepane_archive::detect(&e.path).is_some())
                .map(|e| e.path.clone());
            if let Some(path) = archive {
                let side = self.active;
                self.enqueue(JobSpec {
                    kind: JobKind::OpenArchive,
                    sources: vec![path],
                    dest: None,
                    archive: Some(ArchiveJob {
                        side,
                        ..ArchiveJob::default()
                    }),
                });
                return;
            }
        }
        self.active_panel_mut().enter();
    }

    /// The archive an archive command acts on, with what is known about it: the one being
    /// browsed, or the archive file under the cursor.
    #[allow(clippy::type_complexity)]
    fn archive_target(
        &self,
    ) -> Result<
        (
            PathBuf,
            Option<std::sync::Arc<bluepane_archive::Listing>>,
            Option<String>,
        ),
        EngineError,
    > {
        let panel = self.active_panel();
        if let Some(view) = &panel.archive {
            return Ok((
                view.archive.clone(),
                Some(view.listing.clone()),
                view.password.clone(),
            ));
        }
        panel
            .current()
            .filter(|e| !e.is_dir() && !is_parent_entry(e))
            .filter(|e| bluepane_archive::detect(&e.path).is_some())
            .map(|e| (e.path.clone(), None, None))
            .ok_or_else(|| EngineError("kursor nie wskazuje archiwum (zip, tar, tar.gz)".into()))
    }

    /// The other panel's directory, as the place to extract or pack into.
    pub(crate) fn other_directory(&self) -> Result<PathBuf, EngineError> {
        let other = &self.panels[1 - self.active];
        if other.remote.is_some() {
            return Err(EngineError(
                "drugi panel pokazuje serwer SFTP: wybierz w nim zwykły katalog".into(),
            ));
        }
        if other.virtual_list.is_some() {
            return Err(EngineError(
                "drugi panel pokazuje listę wyników: wybierz w nim zwykły katalog".into(),
            ));
        }
        if other.archive.is_some() {
            return Err(EngineError(
                "drugi panel pokazuje wnętrze archiwum: wybierz w nim zwykły katalog".into(),
            ));
        }
        Ok(other.cwd.clone())
    }

    /// F5 inside an archive: extract the selection into the other panel's directory.
    fn extract_selection(&mut self) -> Result<(), EngineError> {
        let dest = self.other_directory()?;
        let panel = self.active_panel();
        let Some(view) = panel.archive.clone() else {
            return Err(EngineError("to nie jest archiwum".into()));
        };
        let entries: Vec<String> = panel
            .target_entries()
            .into_iter()
            .filter_map(|e| view.inner_path_of(e))
            .collect();
        if entries.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        self.enqueue(JobSpec {
            kind: JobKind::Extract,
            sources: vec![view.archive.clone()],
            dest: Some(dest),
            archive: Some(ArchiveJob {
                entries,
                strip: view.inner.clone(),
                password: view.password.clone(),
                listing: Some(view.listing.clone()),
                ..ArchiveJob::default()
            }),
        });
        Ok(())
    }

    /// `unpack`: the whole archive into the other panel's directory.
    fn unpack(&mut self, args: &UnpackArgs) -> Result<(), EngineError> {
        self.deny_remote("rozpakowanie")?;
        let (archive, listing, known_password) = self.archive_target()?;
        let base = if args.here {
            archive
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| EngineError("archiwum nie ma katalogu nadrzędnego".into()))?
        } else {
            self.other_directory()?
        };
        let dest = if args.subfolder {
            let name = archive
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let stem = strip_archive_extension(name.strip_suffix(".001").unwrap_or(&name));
            base.join(if stem.is_empty() { "archiwum" } else { stem })
        } else {
            base
        };
        self.enqueue(JobSpec {
            kind: JobKind::Extract,
            sources: vec![archive],
            dest: Some(dest),
            archive: Some(ArchiveJob {
                password: args.password.clone().or(known_password),
                listing,
                ..ArchiveJob::default()
            }),
        });
        Ok(())
    }

    /// `testarchive`: read the whole archive and report damaged files.
    fn test_archive(&mut self, password: Option<String>) -> Result<(), EngineError> {
        self.deny_remote("sprawdzanie archiwum")?;
        let (archive, _listing, known_password) = self.archive_target()?;
        self.enqueue(JobSpec {
            kind: JobKind::VerifyArchive,
            sources: vec![archive],
            dest: None,
            archive: Some(ArchiveJob {
                password: password.or(known_password),
                ..ArchiveJob::default()
            }),
        });
        Ok(())
    }

    /// `pack`: without arguments open the settings dialog, with arguments start right away.
    fn pack(&mut self, args: Option<PackArgs>) -> Result<(), EngineError> {
        self.deny_remote("pakowanie")?;
        self.deny_remote("pakowanie")?;
        self.deny_in_archive("pakowanie")?;
        let dest = self.other_directory()?;
        let sources = self.active_panel().targets();
        if sources.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        let mut form = PackForm {
            name: self.default_archive_name(&sources),
            format: Format::Zip,
            level: Level::Normal,
            password: String::new(),
            password_again: String::new(),
            split: String::new(),
            encrypt_names: false,
            error: None,
            sources,
            dest,
        };
        match args {
            None => {
                self.status = "Pakowanie: ustaw opcje (Enter = pakuj, Esc = anuluj)".into();
                self.pending = Some(Pending::Pack(form));
                Ok(())
            }
            Some(args) => {
                if !args.name.is_empty() {
                    form.name = args.name;
                }
                form.format = args
                    .format
                    .or_else(|| Format::from_name(&form.name))
                    .unwrap_or(Format::Zip);
                if let Some(level) = args.level {
                    form.level = level;
                }
                if let Some(password) = args.password {
                    form.password_again = password.clone();
                    form.password = password;
                }
                if let Some(split) = args.split {
                    form.split = split.to_string();
                }
                form.encrypt_names = args.encrypt_names;
                self.start_pack(&form)
            }
        }
    }

    /// The suggested archive name: the single item's name (without extension for a file),
    /// or the current directory's name for several items.
    fn default_archive_name(&self, sources: &[PathBuf]) -> String {
        let panel = self.active_panel();
        let name_of = |path: &Path| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let base = match sources {
            [only] => {
                let is_dir = panel
                    .entries
                    .iter()
                    .find(|e| &e.path == only)
                    .is_some_and(|e| e.is_dir());
                name_parts(&name_of(only), is_dir).0.to_string()
            }
            _ => name_of(&panel.cwd),
        };
        if base.is_empty() {
            "archiwum".into()
        } else {
            base
        }
    }

    /// Checks the pack settings and starts the job.
    fn start_pack(&mut self, form: &PackForm) -> Result<(), EngineError> {
        let name = form.name.trim();
        if name.is_empty() {
            return Err(EngineError("podaj nazwę archiwum".into()));
        }
        if name.contains(['/', '\\', '\0']) {
            return Err(EngineError(
                "nazwa archiwum nie może zawierać ukośników (wskazuje je drugi panel)".into(),
            ));
        }
        if form.password != form.password_again {
            return Err(EngineError("hasła się różnią".into()));
        }
        if !form.password.is_empty() && !form.format.supports_password() {
            return Err(EngineError(
                "TAR nie obsługuje haseł: wybierz ZIP lub 7z albo usuń hasło".into(),
            ));
        }
        if form.encrypt_names && !form.format.can_encrypt_names() {
            return Err(EngineError(
                "ukrycie nazw plików jest możliwe tylko w formacie 7z".into(),
            ));
        }
        if form.encrypt_names && form.password.is_empty() {
            return Err(EngineError("ukrycie nazw plików wymaga hasła".into()));
        }
        let split = if form.split.trim().is_empty() {
            None
        } else {
            Some(parse_size(&form.split)?)
        };
        self.enqueue(JobSpec {
            kind: JobKind::Pack,
            sources: form.sources.clone(),
            dest: Some(form.dest.clone()),
            archive: Some(ArchiveJob {
                password: (!form.password.is_empty()).then(|| form.password.clone()),
                pack: Some(PackOptions {
                    format: form.format,
                    level: form.level,
                    split,
                    encrypt_names: form.encrypt_names,
                    output_name: output_name(name, form.format),
                }),
                ..ArchiveJob::default()
            }),
        });
        Ok(())
    }

    fn make_dir(&mut self, name: &str) -> Result<(), EngineError> {
        if self.active_panel().remote.is_some() {
            return self.remote_mkdir(name);
        }
        if self.active_panel().archive.is_some() {
            return self.make_archive_dir(name);
        }
        if self.active_panel().virtual_list.is_some() {
            return Err(EngineError(
                "to lista wyników, nie katalog: najpierw wyjdź z niej (Backspace)".into(),
            ));
        }
        let cwd = self.active_panel().cwd.clone();
        let created = ops::mkdir(&cwd, name).map_err(|e| EngineError(e.to_string()))?;
        self.panels.iter_mut().for_each(Panel::reload);
        self.active_panel_mut().put_cursor_on(&created);
        self.status = format!("Utworzono katalog {}", name.trim());
        Ok(())
    }

    /// Queues a change of the archive shown in panel `side`.
    fn modify_archive(&mut self, side: usize, change: Change) -> Result<(), EngineError> {
        let Some(view) = self.panels[side].archive.clone() else {
            return Err(EngineError("to nie jest archiwum".into()));
        };
        self.enqueue(JobSpec {
            kind: JobKind::ModifyArchive,
            sources: vec![view.archive.clone()],
            dest: None,
            archive: Some(ArchiveJob {
                password: view.password.clone(),
                listing: Some(view.listing.clone()),
                change: Some(change),
                side,
                ..ArchiveJob::default()
            }),
        });
        Ok(())
    }

    /// F5 from a directory into an archive panel: add the selection to the archive.
    fn add_to_archive(&mut self) -> Result<(), EngineError> {
        let side = 1 - self.active;
        let sources = self.active_panel().targets();
        if sources.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        let into = self.panels[side]
            .archive
            .as_ref()
            .map(|v| v.inner.clone())
            .unwrap_or_default();
        self.modify_archive(side, Change::Add { sources, into })
    }

    /// F8 inside an archive: ask before taking entries out for good.
    fn propose_archive_removal(&mut self) -> Result<(), EngineError> {
        let panel = self.active_panel();
        let Some(view) = &panel.archive else {
            return Err(EngineError("to nie jest archiwum".into()));
        };
        let entries: Vec<String> = panel
            .target_entries()
            .into_iter()
            .filter_map(|e| view.inner_path_of(e))
            .collect();
        if entries.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        self.status = if let [only] = entries.as_slice() {
            format!("Usunąć „{only}” z archiwum? Nie trafi do kosza. (Enter = tak, Esc = nie)")
        } else {
            format!(
                "Usunąć z archiwum zaznaczone elementy ({})? Nie trafią do kosza. (Enter = tak, Esc = nie)",
                entries.len()
            )
        };
        self.pending = Some(Pending::ConfirmRemove {
            side: self.active,
            entries,
        });
        Ok(())
    }

    /// The archive path of a new name typed in the directory being browsed.
    fn archive_child_path(&self, name: &str) -> Result<String, EngineError> {
        let name = name.trim();
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(EngineError(format!("niedozwolona nazwa: „{name}”")));
        }
        let inner = self
            .active_panel()
            .archive
            .as_ref()
            .map(|v| v.inner.as_str())
            .unwrap_or_default();
        Ok(if inner.is_empty() {
            name.to_string()
        } else {
            format!("{inner}/{name}")
        })
    }

    fn make_archive_dir(&mut self, name: &str) -> Result<(), EngineError> {
        let path = self.archive_child_path(name)?;
        self.modify_archive(self.active, Change::NewDir { path })
    }

    fn rename_in_archive(&mut self, entry: &Path, new_name: &str) -> Result<(), EngineError> {
        let to = self.archive_child_path(new_name)?;
        let from = self
            .active_panel()
            .archive
            .as_ref()
            .and_then(|view| {
                let name = entry.file_name()?.to_string_lossy().into_owned();
                Some(if view.inner.is_empty() {
                    name
                } else {
                    format!("{}/{name}", view.inner)
                })
            })
            .ok_or_else(|| EngineError("to nie jest archiwum".into()))?;
        if from == to {
            return Ok(());
        }
        self.modify_archive(self.active, Change::Rename { from, to })
    }

    /// The entry a rename acts on: the one under the cursor, never `..`.
    fn entry_to_rename(&self) -> Result<PathBuf, EngineError> {
        self.active_panel()
            .current()
            .filter(|e| !is_parent_entry(e))
            .map(|e| e.path.clone())
            .ok_or_else(|| EngineError("nic nie wskazano".into()))
    }

    fn rename_entry(&mut self, path: &Path, new_name: &str) -> Result<(), EngineError> {
        if self.active_panel().remote.is_some() {
            return self.remote_rename(path, new_name);
        }
        if self.active_panel().archive.is_some() {
            return self.rename_in_archive(path, new_name);
        }
        let renamed = ops::rename(path, new_name).map_err(|e| EngineError(e.to_string()))?;
        self.panels.iter_mut().for_each(Panel::reload);
        self.active_panel_mut().put_cursor_on(&renamed);
        self.status = format!("Zmieniono nazwę na {}", new_name.trim());
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.remove_preview_dir();
        self.remove_edit_dir();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_dirs() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir(&left).unwrap();
        std::fs::create_dir(&right).unwrap();
        std::fs::write(left.join("a.txt"), "a").unwrap();
        std::fs::write(left.join("b.txt"), "b").unwrap();
        let session = Session::new(left, right);
        (tmp, session)
    }

    #[test]
    fn copy_goes_to_the_other_panel() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("left/a.txt").exists());
        assert_eq!(s.panels[1].entries.len(), 2); // ".." and a.txt
        assert!(s.panels[0].selected.is_empty());
    }

    #[test]
    fn move_uses_cursor_when_nothing_is_selected() {
        let (tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("move").unwrap();
        assert!(s.wait_idle());
        assert!(!tmp.path().join("left/a.txt").exists());
        assert!(tmp.path().join("right/a.txt").exists());
    }

    /// Both panels on `left`, cursor on a.txt.
    fn same_dir() -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = two_dirs();
        s.run_line("switch").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        s.run_line("cursor 1").unwrap();
        (tmp, s)
    }

    #[test]
    fn move_in_the_same_directory_is_rejected() {
        let (_tmp, mut s) = same_dir();
        assert!(s.run_line("move").is_err());
        assert!(s.status.contains("ten sam katalog"));
        assert!(s.pending.is_none());
    }

    #[test]
    fn copy_in_the_same_directory_asks_before_duplicating() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_some());
        assert!(s.status.contains("a (kopia).txt"), "{}", s.status);
        // Nothing is created until the user answers.
        assert!(!tmp.path().join("left/a (kopia).txt").exists());

        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a (kopia).txt")).unwrap(),
            "a"
        );
        assert!(tmp.path().join("left/a.txt").exists());
        // Both panels were reloaded.
        assert!(s.panels[0]
            .entries
            .iter()
            .any(|e| e.name == "a (kopia).txt"));
        assert!(s.panels[1]
            .entries
            .iter()
            .any(|e| e.name == "a (kopia).txt"));
    }

    #[test]
    fn declining_creates_nothing() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert_eq!(s.status, "Anulowano");
        assert!(!tmp.path().join("left/a (kopia).txt").exists());
        assert!(s.run_line("confirm").is_err());
    }

    #[test]
    fn any_other_command_drops_the_question() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        s.run_line("cursor 1").unwrap();
        assert!(s.pending.is_none());
        assert!(s.run_line("confirm").is_err());
        assert!(!tmp.path().join("left/a (kopia).txt").exists());
    }

    #[test]
    fn duplicates_every_selected_entry_and_never_overwrites() {
        let (tmp, mut s) = same_dir();
        std::fs::write(tmp.path().join("left/a (kopia).txt"), "old").unwrap();
        s.run_line("cursor -1").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a (kopia).txt")).unwrap(),
            "old"
        );
        assert!(tmp.path().join("left/a (kopia 2).txt").exists());
        assert!(tmp.path().join("left/b (kopia).txt").exists());
    }

    #[test]
    fn copy_with_nothing_to_copy_does_not_ask() {
        let (_tmp, mut s) = same_dir();
        s.run_line("cursor -1").unwrap(); // on ".."
        assert!(s.run_line("copy").is_err());
        assert!(s.pending.is_none());
    }

    #[test]
    fn mkdir_and_cd_work() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir new").unwrap();
        assert!(tmp.path().join("left/new").is_dir());
        s.run_line("cd new").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/new"));
        assert!(s.run_line("cd nope").is_err());
    }

    #[test]
    fn errors_land_in_status() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("frobnicate").is_err());
        assert!(s.status.contains("nieznana"));
    }

    #[test]
    fn parent_row_is_not_an_operation_target() {
        let (tmp, mut s) = two_dirs();
        // The cursor starts on "..": nothing to copy, and nothing must happen to the parent.
        assert!(s.run_line("copy").is_err());
        assert!(s.run_line("delete").is_err());
        assert!(s.run_line("select ..").is_err());
        assert!(tmp.path().join("left/a.txt").exists());
    }

    #[test]
    fn back_and_forward_follow_the_history() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("cd sub").unwrap();
        s.run_line("back").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("forward").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/sub"));
        assert!(s.run_line("forward").is_err());
        assert!(s.status.contains("historii"));
        // The other panel has its own history.
        s.run_line("switch").unwrap();
        assert!(s.run_line("back").is_err());
    }

    #[test]
    fn backspace_edits_the_filter_before_going_up() {
        let (tmp, mut s) = two_dirs();
        s.run_line("filter b").unwrap();
        assert_eq!(s.active_panel().entries.len(), 2); // ".." and b.txt
        s.run_line("up").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        assert!(s.active_panel().filter.is_empty());
        s.run_line("up").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path());
    }

    #[test]
    fn operations_ignore_entries_hidden_by_the_filter() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("filter b").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle()); // acts on b.txt under the cursor, not on hidden a.txt
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!tmp.path().join("right/a.txt").exists());
    }

    #[test]
    fn volume_jump_by_number_and_name() {
        let (tmp, mut s) = two_dirs();
        s.volumes = vec![
            Volume {
                name: "one".into(),
                path: tmp.path().join("left"),
            },
            Volume {
                name: "Two".into(),
                path: tmp.path().join("right"),
            },
        ];
        s.run_line("volume 2").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
        s.run_line("volume one").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("volume TWO").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
        s.run_line("back").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));

        assert!(s.run_line("volume 0").is_err());
        assert!(s.run_line("volume 3").is_err());
        let err = s.run_line("volume nope").unwrap_err();
        assert!(err.0.contains("dostępne: 1 one, 2 Two"), "{err}");
    }

    #[test]
    fn volume_of_picks_the_longest_prefix() {
        let volumes = vec![
            Volume {
                name: "root".into(),
                path: PathBuf::from("/a"),
            },
            Volume {
                name: "nested".into(),
                path: PathBuf::from("/a/b"),
            },
        ];
        assert_eq!(volume_index(&volumes, Path::new("/a/b/c")), Some(1));
        assert_eq!(volume_index(&volumes, Path::new("/a/x")), Some(0));
        assert_eq!(volume_index(&volumes, Path::new("/z")), None);
    }

    #[test]
    fn selection_commands_report_in_status() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("left/c.md"), "c").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("selmask *.txt").unwrap();
        assert!(s.status.contains("Zaznaczono: 2"), "{}", s.status);
        s.run_line("unselmask b*").unwrap();
        assert_eq!(s.active_panel().selected.len(), 1);
        s.run_line("invert").unwrap();
        assert_eq!(s.active_panel().selected.len(), 2);
        s.run_line("selectall").unwrap();
        assert!(s.status.contains("Zaznaczono: 3"));
        s.run_line("unselectall").unwrap();
        assert!(s.active_panel().selected.is_empty());
        assert!(s.run_line("selmask ;").is_err());
    }

    #[test]
    fn copy_acts_on_the_mask_selection() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("left/c.md"), "c").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("selmask *.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!tmp.path().join("right/c.md").exists());
    }

    #[test]
    fn sort_command_reorders_the_active_panel_only() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("left/big.bin"), vec![0u8; 100]).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("sort size desc").unwrap();
        let names: Vec<_> = s.panels[0]
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["..", "big.bin", "a.txt", "b.txt"]);
        assert_eq!(s.panels[1].sort.key, crate::SortKey::Name);
        assert!(s.run_line("sort bogus").is_err());
    }

    fn read(path: std::path::PathBuf) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    /// Both dirs hold `a.txt`: "a" on the left, "old" on the right; a.txt is selected.
    fn clash() -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("right/a.txt"), "old").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select a.txt").unwrap();
        (tmp, s)
    }

    fn set_mtime(path: std::path::PathBuf, secs: u64) {
        let time = std::time::UNIX_EPOCH + Duration::from_secs(secs);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn an_existing_target_asks_and_overwrite_replaces_it() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        assert!(!s.wait_idle(), "must stop at the conflict");
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        assert!(s.status.contains("a.txt"));
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        let job = s.job_status().unwrap();
        assert!(job.waiting_for_answer);

        s.run_line("overwrite").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "a");
        assert!(s.pending.is_none());
        assert!(!s.status_is_error);
    }

    #[test]
    fn skip_keeps_the_target_and_says_so() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("skip").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        assert!(s.status.contains("pominięto: 1"), "{}", s.status);
    }

    #[test]
    fn keep_both_writes_a_second_copy() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("keepboth").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        assert_eq!(read(tmp.path().join("right/a (kopia).txt")), "a");
    }

    #[test]
    fn newer_overwrites_only_older_targets_and_all_answers_once() {
        let (tmp, mut s) = two_dirs();
        for name in ["a.txt", "b.txt"] {
            std::fs::write(tmp.path().join("right").join(name), "old").unwrap();
        }
        set_mtime(tmp.path().join("left/a.txt"), 2_000); // newer than the target
        set_mtime(tmp.path().join("right/a.txt"), 1_000);
        set_mtime(tmp.path().join("left/b.txt"), 1_000); // older than the target
        set_mtime(tmp.path().join("right/b.txt"), 2_000);
        s.run_line("refresh").unwrap();
        s.run_line("selmask *.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(!s.wait_idle());
        s.run_line("newer-all").unwrap();
        assert!(s.wait_idle(), "one answer covers both conflicts");
        assert_eq!(read(tmp.path().join("right/a.txt")), "a");
        assert_eq!(read(tmp.path().join("right/b.txt")), "old");
        assert!(s.status.contains("pominięto: 1"), "{}", s.status);
    }

    #[test]
    fn other_commands_do_not_release_a_conflict_but_stop_does() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("cursor 1").unwrap();
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        assert!(
            s.run_line("confirm").is_err(),
            "Enter must not answer a conflict"
        );

        s.run_line("stop").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert!(s.status.starts_with("Anulowano"), "{}", s.status);
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
    }

    #[test]
    fn cancel_on_a_conflict_cancels_the_job_and_the_queue() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap(); // queued behind the blocked job
        assert!(s.status.contains("W kolejce"));
        s.run_line("cancel").unwrap();
        assert!(s.wait_idle());
        assert!(
            !tmp.path().join("right/b.txt").exists(),
            "queue was dropped"
        );
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
    }

    #[test]
    fn answers_without_a_question_are_errors() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("overwrite").is_err());
        assert!(s.run_line("confirm").is_err());
        assert!(s.status_is_error);
        s.run_line("stop").unwrap();
        assert_eq!(s.status, "Nic się nie wykonuje");
    }

    #[test]
    fn auto_resolve_answers_conflicts_without_asking() {
        let (tmp, mut s) = clash();
        s.auto_resolve = Some(Resolution::Skip);
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        assert!(s.pending.is_none());
    }

    #[test]
    fn jobs_run_one_after_another() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!s.is_busy());
        assert!(s.job_status().is_none());
    }

    #[test]
    fn delete_asks_first_and_cancel_keeps_the_file() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("delete").unwrap();
        assert!(matches!(s.pending, Some(Pending::ConfirmDelete(_))));
        assert!(s.status.contains("kosza"));
        assert!(s.wait_idle(), "nothing was started");
        assert!(tmp.path().join("left/a.txt").exists());
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert!(tmp.path().join("left/a.txt").exists());
        // Cancelling keeps the selection; with nothing selected and the cursor on ".."
        // there is nothing to delete.
        s.run_line("unselectall").unwrap();
        s.run_line("cursor -5").unwrap();
        assert!(s.run_line("delete").is_err());
    }

    fn type_into_dialog(s: &mut Session, text: &str) {
        match &mut s.pending {
            Some(Pending::Input { text: field, .. }) => *field = text.to_string(),
            other => panic!("no name dialog open: {other:?}"),
        }
    }

    #[test]
    fn mkdir_without_a_name_opens_a_dialog() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir").unwrap();
        type_into_dialog(&mut s, "Reports");
        s.run_line("confirm").unwrap();
        assert!(tmp.path().join("left/Reports").is_dir());
        assert!(s.pending.is_none());
        assert_eq!(s.active_panel().current().unwrap().name, "Reports");
    }

    #[test]
    fn a_bad_name_keeps_the_dialog_open_for_correction() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir").unwrap();
        type_into_dialog(&mut s, "a/b");
        assert!(s.run_line("confirm").is_err());
        assert!(s.status_is_error);
        assert!(matches!(s.pending, Some(Pending::Input { .. })));
        type_into_dialog(&mut s, "fixed");
        s.run_line("confirm").unwrap();
        assert!(tmp.path().join("left/fixed").is_dir());
        s.run_line("mkdir").unwrap();
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
    }

    #[test]
    fn rename_dialog_is_prefilled_and_renames_the_entry_under_the_cursor() {
        let (tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("rename").unwrap();
        match &s.pending {
            Some(Pending::Input { text, kind }) => {
                assert_eq!(text, "a.txt");
                assert_eq!(*kind, InputKind::Rename(tmp.path().join("left/a.txt")));
            }
            other => panic!("{other:?}"),
        }
        type_into_dialog(&mut s, "renamed.txt");
        s.run_line("confirm").unwrap();
        assert!(!tmp.path().join("left/a.txt").exists());
        assert_eq!(read(tmp.path().join("left/renamed.txt")), "a");
        assert_eq!(s.active_panel().current().unwrap().name, "renamed.txt");
    }

    #[test]
    fn rename_refuses_existing_names_and_the_parent_row() {
        let (tmp, mut s) = two_dirs();
        assert!(s.run_line("rename x").is_err(), "cursor is on ..");
        s.run_line("cursor 1").unwrap();
        assert!(s.run_line("rename b.txt").is_err());
        assert_eq!(read(tmp.path().join("left/b.txt")), "b");
        s.run_line("rename c.txt").unwrap();
        assert!(tmp.path().join("left/c.txt").exists());
    }

    #[test]
    fn progress_fraction_prefers_bytes_then_items() {
        let mut status = JobStatus {
            label: "Kopiowanie",
            current: String::new(),
            bytes_done: 0,
            bytes_total: 0,
            items_done: 0,
            items_total: 0,
            queued: 0,
            waiting_for_answer: false,
        };
        assert_eq!(status.fraction(), None);
        status.items_total = 4;
        status.items_done = 1;
        assert_eq!(status.fraction(), Some(0.25));
        status.bytes_total = 200;
        status.bytes_done = 50;
        assert_eq!(status.fraction(), Some(0.25));
        status.bytes_done = 500;
        assert_eq!(status.fraction(), Some(1.0));
    }

    #[test]
    fn view_opens_the_file_under_the_cursor_and_close_leaves() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("view").is_err(), "cursor is on ..");
        s.run_line("cursor 1").unwrap();
        s.run_line("view").unwrap();
        let viewer = s.viewer.as_mut().expect("viewer is open");
        viewer.set_viewport(5, 80);
        assert_eq!(viewer.visible_rows()[0].text, "a");
        assert!(s.status.contains("a.txt"));
        s.run_line("viewer close").unwrap();
        assert!(s.viewer.is_none());
        assert!(
            s.run_line("viewer scroll 1").is_err(),
            "nothing is open any more"
        );
    }

    #[test]
    fn view_refuses_directories() {
        let (_tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("select sub").unwrap();
        let err = s.run_line("view").unwrap_err();
        assert!(err.0.contains("katalog"), "{err}");
        assert!(s.viewer.is_none());
    }

    #[test]
    fn viewer_commands_navigate_and_search() {
        let (tmp, mut s) = two_dirs();
        let lines: String = (0..50).map(|i| format!("row {i}\n")).collect();
        std::fs::write(tmp.path().join("left/long.txt"), lines).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select long.txt").unwrap();
        s.run_line("view").unwrap();
        s.viewer.as_mut().unwrap().set_viewport(5, 80);
        s.run_line("viewer scroll 10").unwrap();
        assert_eq!(s.viewer.as_mut().unwrap().visible_rows()[0].text, "row 10");
        s.run_line("viewer goto 0").unwrap();
        s.run_line("viewer find row 33").unwrap();
        assert!(s.status.contains("znaleziono"), "{}", s.status);
        assert_eq!(s.viewer.as_mut().unwrap().visible_rows()[0].text, "row 33");
        s.run_line("viewer mode").unwrap();
        assert_eq!(s.viewer.as_ref().unwrap().mode(), crate::ViewMode::Hex);
        assert!(s.run_line("viewer goto nonsense").is_err());
    }

    #[test]
    fn the_viewer_search_dialog_is_prefilled_with_the_last_search() {
        let (_tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("view").unwrap();
        s.run_line("viewer find").unwrap();
        type_into_dialog(&mut s, "a");
        s.run_line("confirm").unwrap();
        assert!(s.pending.is_none());
        s.run_line("viewer find").unwrap();
        match &s.pending {
            Some(Pending::Input { text, kind }) => {
                assert_eq!(text, "a");
                assert_eq!(*kind, InputKind::ViewerFind);
            }
            other => panic!("{other:?}"),
        }
        s.run_line("cancel").unwrap();
        assert!(
            s.viewer.is_some(),
            "cancelling the dialog keeps the viewer open"
        );
    }

    fn titles(s: &Session, side: usize) -> Vec<String> {
        s.tab_strip(side).titles
    }

    #[test]
    fn a_new_tab_copies_the_directory_and_becomes_active() {
        let (tmp, mut s) = two_dirs();
        let strip = s.tab_strip(0);
        assert_eq!(strip.titles, ["left"]);
        assert_eq!(strip.paths, [tmp.path().join("left")]);
        assert_eq!(strip.active, 0);
        s.run_line("tab new").unwrap();
        assert_eq!(titles(&s, 0), ["left", "left"]);
        assert_eq!(s.tab_strip(0).active, 1);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left"));
        assert!(s.status.contains("2 z 2"), "{}", s.status);
        // The other side is untouched.
        assert_eq!(titles(&s, 1), ["right"]);
    }

    #[test]
    fn tabs_keep_their_own_directory_history_and_sort() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd sub").unwrap();
        s.run_line("sort size desc").unwrap();
        assert_eq!(titles(&s, 0), ["left", "sub"]);
        s.run_line("tab 1").unwrap();
        assert_eq!(s.panels[0].cwd, tmp.path().join("left"));
        assert!(
            !s.panels[0].sort.descending,
            "the first tab was not re-sorted"
        );
        assert!(!s.panels[0].can_go_back());
        s.run_line("tab 2").unwrap();
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/sub"));
        assert!(s.panels[0].sort.descending);
        assert!(s.panels[0].can_go_back(), "history belongs to the tab");
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let (_tmp, mut s) = two_dirs();
        s.run_line("tab new").unwrap();
        s.run_line("tab new").unwrap();
        assert_eq!(s.tab_strip(0).active, 2);
        s.run_line("tab next").unwrap();
        assert_eq!(s.tab_strip(0).active, 0);
        s.run_line("tab prev").unwrap();
        assert_eq!(s.tab_strip(0).active, 2);
        s.run_line("tab prev").unwrap();
        assert_eq!(s.tab_strip(0).active, 1);
    }

    #[test]
    fn open_puts_the_directory_under_the_cursor_in_a_new_tab() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("cursor -5").unwrap(); // up to ".."
        assert!(s.run_line("tab open").is_err());
        s.run_line("select sub").unwrap();
        s.run_line("tab open").unwrap();
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/sub"));
        assert_eq!(titles(&s, 0), ["left", "sub"]);
        s.run_line("tab 1").unwrap();
        s.run_line("cursor 100").unwrap();
        assert!(s.run_line("tab open").is_err(), "a file is not a directory");
    }

    #[test]
    fn closing_tabs_activates_a_neighbour_and_keeps_the_last_one() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir a").unwrap();
        s.run_line("mkdir b").unwrap();
        s.run_line("tab new").unwrap(); // tabs: left, left*
        s.run_line("cd a").unwrap(); // left, a*
        s.run_line("tab new").unwrap(); // left, a, a*
        s.run_line("cd ..").unwrap();
        s.run_line("cd b").unwrap(); // left, a, b*
        assert_eq!(titles(&s, 0), ["left", "a", "b"]);

        // Closing the last tab activates the one on its left.
        s.run_line("tab close").unwrap();
        assert_eq!(titles(&s, 0), ["left", "a"]);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/a"));
        // Closing a background tab keeps the active one and fixes the index.
        s.run_line("tab close 1").unwrap();
        assert_eq!(titles(&s, 0), ["a"]);
        assert_eq!(s.tab_strip(0).active, 0);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/a"));
        assert!(s.run_line("tab close").is_err(), "the last tab stays");
    }

    #[test]
    fn closing_the_active_tab_in_the_middle_activates_the_one_to_its_right() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir a").unwrap();
        s.run_line("mkdir b").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd a").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd ..").unwrap();
        s.run_line("cd b").unwrap(); // left, a, b*
        s.run_line("tab 2").unwrap();
        s.run_line("tab close").unwrap();
        assert_eq!(titles(&s, 0), ["left", "b"]);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/b"));
        assert_eq!(s.tab_strip(0).active, 1);
    }

    #[test]
    fn bad_tab_numbers_are_errors() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("tab 2").is_err());
        assert!(s.run_line("tab close 3").is_err());
        assert!(s.run_line("tab 0").is_err());
        assert!(s.status_is_error);
    }

    #[test]
    fn a_background_tab_is_refreshed_when_it_is_shown_again() {
        let (tmp, mut s) = two_dirs();
        s.run_line("tab new").unwrap();
        s.run_line("tab 1").unwrap();
        std::fs::write(tmp.path().join("left/fresh.txt"), "x").unwrap();
        s.run_line("tab 2").unwrap();
        assert!(s.panels[0].entries.iter().any(|e| e.name == "fresh.txt"));
    }

    #[test]
    fn operations_use_the_active_tab_of_each_side() {
        let (tmp, mut s) = two_dirs();
        std::fs::create_dir(tmp.path().join("other")).unwrap();
        s.run_line("switch").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("other").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(
            tmp.path().join("other/a.txt").exists(),
            "copied into the active tab of the other side"
        );
        assert!(!tmp.path().join("right/a.txt").exists());
    }

    #[test]
    fn cd_normalises_dot_and_dotdot() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir a").unwrap();
        s.run_line("cd a").unwrap();
        s.run_line("cd ..").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("cd ./a/../a/.").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/a"));
        s.run_line(&format!("cd {}/left/a/../../right", tmp.path().display()))
            .unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
    }

    #[test]
    fn normalising_never_climbs_above_the_root() {
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        let beyond = Path::new(root).join("..").join("..");
        assert_eq!(normalize_path(&beyond), Path::new(root));
        assert_eq!(normalize_path(Path::new("a/b/../../..")), Path::new(""));
    }

    // ---- archives -------------------------------------------------------------------

    fn make_archive(
        sources: &[PathBuf],
        out: &Path,
        format: bluepane_archive::Format,
        password: Option<&str>,
    ) {
        let options = bluepane_archive::CreateOptions {
            format,
            level: bluepane_archive::Level::Normal,
            password: password.map(str::to_string),
            split: None,
            encrypt_names: false,
        };
        bluepane_archive::create(
            sources,
            out,
            &options,
            &bluepane_ops::Progress::new(),
            &mut bluepane_ops::FailOnConflict,
        )
        .unwrap();
    }

    /// `left/box.zip` (docs/a.txt, docs/sub/b.txt, top.txt) and `left/locked.zip` (the same,
    /// password "pw"); `right` is empty. The active panel is the left one.
    fn with_archives() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("docs/sub")).unwrap();
        std::fs::write(src.join("docs/a.txt"), "alpha").unwrap();
        std::fs::write(src.join("docs/sub/b.txt"), "bravo").unwrap();
        std::fs::write(src.join("top.txt"), "top").unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let sources = [src.join("docs"), src.join("top.txt")];
        make_archive(
            &sources,
            &left.join("box.zip"),
            bluepane_archive::Format::Zip,
            None,
        );
        make_archive(
            &sources,
            &left.join("locked.zip"),
            bluepane_archive::Format::Zip,
            Some("pw"),
        );
        let s = Session::new(left, right);
        (tmp, s)
    }

    fn enter_archive(s: &mut Session, name: &str) {
        s.run_line(&format!("select {name}")).unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert!(s.panels[s.active].archive.is_some(), "{}", s.status);
    }

    fn rows(s: &Session) -> Vec<String> {
        s.active_panel()
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    #[test]
    fn enter_on_an_archive_opens_it_in_the_panel() {
        let (tmp, mut s) = with_archives();
        s.run_line("select box.zip").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("Archiwum box.zip"), "{}", s.status);
        assert!(!s.status_is_error);
        assert_eq!(rows(&s), ["..", "docs", "top.txt"]);
        assert_eq!(s.active_panel().location(), tmp.path().join("left/box.zip"));
        assert_eq!(s.tab_strip(0).titles, ["box.zip"]);

        s.run_line("select docs").unwrap();
        s.run_line("enter").unwrap();
        assert_eq!(rows(&s), ["..", "sub", "a.txt"]);
        s.run_line("up").unwrap();
        s.run_line("up").unwrap();
        assert!(
            s.active_panel().archive.is_none(),
            "up from the top level leaves"
        );
        assert_eq!(s.active_panel().current().unwrap().name, "box.zip");
    }

    #[test]
    fn enter_on_a_plain_file_does_nothing_and_on_a_directory_enters_it() {
        let (tmp, mut s) = with_archives();
        std::fs::write(tmp.path().join("left/plain.txt"), "x").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select plain.txt").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert!(s.active_panel().archive.is_none());
        std::fs::create_dir(tmp.path().join("left/d")).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select d").unwrap();
        s.run_line("enter").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/d"));
    }

    #[test]
    fn a_broken_archive_reports_an_error_and_the_panel_stays_put() {
        let (tmp, mut s) = with_archives();
        std::fs::write(tmp.path().join("left/bad.zip"), "not a zip").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select bad.zip").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert!(s.status_is_error, "{}", s.status);
        assert!(s.status.contains("bad.zip"));
        assert!(s.active_panel().archive.is_none());
    }

    #[test]
    fn f5_in_an_archive_extracts_the_selection_into_the_other_panel() {
        let (tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select docs").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("Rozpakowano: 2"), "{}", s.status);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/docs/a.txt")).unwrap(),
            "alpha"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/docs/sub/b.txt")).unwrap(),
            "bravo"
        );
        assert!(!tmp.path().join("right/top.txt").exists());
        assert!(
            s.panels[1].entries.iter().any(|e| e.name == "docs"),
            "the other panel refreshed"
        );
        assert!(
            s.active_panel().archive.is_some(),
            "still browsing the archive"
        );
    }

    #[test]
    fn extracting_from_a_subdirectory_drops_the_directory_prefix() {
        let (tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select docs").unwrap();
        s.run_line("enter").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/a.txt")).unwrap(),
            "alpha"
        );
        assert!(!tmp.path().join("right/docs").exists());
    }

    #[test]
    fn extraction_asks_before_replacing_files() {
        let (tmp, mut s) = with_archives();
        std::fs::write(tmp.path().join("right/top.txt"), "mine").unwrap();
        s.run_line("refresh").unwrap();
        enter_archive(&mut s, "box.zip");
        s.run_line("select top.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(!s.wait_idle(), "must stop at the conflict");
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        s.run_line("keepboth").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/top.txt")).unwrap(),
            "mine"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/top (kopia).txt")).unwrap(),
            "top"
        );
    }

    #[test]
    fn viewing_and_packing_stay_unavailable_inside_an_archive() {
        let (_tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select top.txt").unwrap();
        for command in ["move", "edit", "pack"] {
            let err = s.run_line(command).unwrap_err();
            assert!(!err.0.is_empty(), "{command}");
            assert!(s.pending.is_none(), "{command} must not open a dialog");
        }
    }

    /// Both panels in `left`; the right one (inactive) browses `name`, the left one is active.
    fn archive_on_the_right(name: &str) -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = with_archives();
        std::fs::write(tmp.path().join("left/plain.txt"), "plain").unwrap();
        s.run_line("switch").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        enter_archive(&mut s, name);
        s.run_line("switch").unwrap();
        s.run_line("refresh").unwrap();
        (tmp, s)
    }

    fn archive_names(path: &Path, password: Option<&str>) -> Vec<String> {
        bluepane_archive::list(path, password, &bluepane_ops::Progress::new())
            .unwrap()
            .entries
            .iter()
            .map(|e| e.path.clone())
            .collect()
    }

    #[test]
    fn f5_into_an_archive_panel_adds_the_selection() {
        let (tmp, mut s) = archive_on_the_right("box.zip");
        s.run_line("select plain.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        assert!(s.status.contains("dodano: 1"), "{}", s.status);
        let names = archive_names(&tmp.path().join("left/box.zip"), None);
        assert!(names.contains(&"plain.txt".to_string()), "{names:?}");
        // The panel that shows the archive sees the new file without re-entering it.
        assert!(s.panels[1].entries.iter().any(|e| e.name == "plain.txt"));
        assert!(s.run_line("move").is_err());
    }

    #[test]
    fn adding_goes_into_the_directory_being_browsed() {
        let (tmp, mut s) = archive_on_the_right("box.zip");
        s.run_line("switch").unwrap();
        s.run_line("select docs").unwrap();
        s.run_line("enter").unwrap();
        s.run_line("switch").unwrap();
        s.run_line("select plain.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        let names = archive_names(&tmp.path().join("left/box.zip"), None);
        assert!(names.contains(&"docs/plain.txt".to_string()), "{names:?}");
    }

    #[test]
    fn copying_from_one_archive_to_another_is_refused() {
        let (_tmp, mut s) = archive_on_the_right("box.zip");
        s.run_line("select locked.zip").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        s.run_line("select top.txt").unwrap();
        let err = s.run_line("copy").unwrap_err();
        assert!(err.0.contains("archiwum do archiwum"), "{err}");
    }

    #[test]
    fn delete_in_an_archive_asks_and_removes_for_good() {
        let (tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select top.txt").unwrap();
        s.run_line("delete").unwrap();
        assert!(matches!(s.pending, Some(Pending::ConfirmRemove { .. })));
        assert!(s.status.contains("Nie trafią") || s.status.contains("Nie trafi"));
        // Refusing changes nothing.
        s.run_line("cancel").unwrap();
        assert!(
            archive_names(&tmp.path().join("left/box.zip"), None).contains(&"top.txt".to_string())
        );

        s.run_line("delete").unwrap();
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        assert!(s.status.contains("usunięto: 1"), "{}", s.status);
        let names = archive_names(&tmp.path().join("left/box.zip"), None);
        assert!(!names.contains(&"top.txt".to_string()), "{names:?}");
        assert!(!rows(&s).contains(&"top.txt".to_string()));
        assert!(
            !tmp.path().join("left/.box.zip.bluepane-tmp").exists(),
            "no temporary file is left behind"
        );
    }

    #[test]
    fn mkdir_and_rename_work_inside_an_archive() {
        let (tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("mkdir fresh").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        assert!(rows(&s).contains(&"fresh".to_string()), "{:?}", rows(&s));

        s.run_line("select top.txt").unwrap();
        s.run_line("cursor 0").unwrap();
        let index = s
            .active_panel()
            .entries
            .iter()
            .position(|e| e.name == "top.txt")
            .unwrap();
        s.run_line(&format!("cursor {index}")).unwrap();
        s.run_line("rename renamed.txt").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        let names = archive_names(&tmp.path().join("left/box.zip"), None);
        assert!(names.contains(&"renamed.txt".to_string()), "{names:?}");
        assert!(names.contains(&"fresh".to_string()), "{names:?}");
        assert!(!names.contains(&"top.txt".to_string()), "{names:?}");

        // A name with a slash would silently move the entry elsewhere: refuse it.
        assert!(s.run_line("mkdir a/b").is_err());
        assert!(s.run_line("rename x/y").is_err());
    }

    #[test]
    fn changing_an_encrypted_archive_asks_for_the_password_once() {
        let (tmp, mut s) = archive_on_the_right("locked.zip");
        s.run_line("select plain.txt").unwrap();
        s.run_line("copy").unwrap();
        s.wait_idle();
        assert!(matches!(
            s.pending,
            Some(Pending::Input {
                kind: InputKind::ArchivePassword,
                ..
            })
        ));
        if let Some(Pending::Input { text, .. }) = &mut s.pending {
            *text = "pw".to_string();
        }
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        let names = archive_names(&tmp.path().join("left/locked.zip"), Some("pw"));
        assert!(names.contains(&"plain.txt".to_string()), "{names:?}");
    }

    #[test]
    fn unpack_extracts_the_archive_under_the_cursor_or_the_open_one() {
        let (tmp, mut s) = with_archives();
        s.run_line("select box.zip").unwrap();
        s.run_line("unpack").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/docs/sub/b.txt").exists());
        assert!(tmp.path().join("right/top.txt").exists());

        s.run_line("unpack -s").unwrap();
        assert!(s.wait_idle());
        assert!(
            tmp.path().join("right/box/docs/a.txt").exists(),
            "extracted into a subfolder"
        );

        // From inside the archive too.
        enter_archive(&mut s, "box.zip");
        std::fs::remove_dir_all(tmp.path().join("right")).unwrap();
        std::fs::create_dir(tmp.path().join("right")).unwrap();
        s.run_line("unpack").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/top.txt").exists());
    }

    #[test]
    fn unpack_needs_an_archive() {
        let (tmp, mut s) = with_archives();
        std::fs::write(tmp.path().join("left/plain.txt"), "x").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select plain.txt").unwrap();
        let err = s.run_line("unpack").unwrap_err();
        assert!(err.0.contains("archiwum"), "{err}");
        assert!(s.run_line("testarchive").is_err());
    }

    #[test]
    fn a_missing_password_is_asked_for_and_the_job_continues() {
        let (tmp, mut s) = with_archives();
        enter_archive(&mut s, "locked.zip");
        assert!(s.status.contains("zaszyfrowane"), "{}", s.status);
        s.run_line("select top.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(
            matches!(
                s.pending,
                Some(Pending::Input {
                    kind: InputKind::ArchivePassword,
                    ..
                })
            ),
            "{:?}",
            s.pending
        );
        assert!(s.status.contains("hasło"));
        assert!(!tmp.path().join("right/top.txt").exists());

        // An empty answer keeps the dialog open.
        assert!(s.run_line("confirm").is_err());
        type_into_dialog(&mut s, "wrong");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("Błędne hasło"), "{}", s.status);
        assert!(s.status_is_error);
        assert!(
            matches!(s.pending, Some(Pending::Input { .. })),
            "asks again"
        );

        type_into_dialog(&mut s, "pw");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/top.txt")).unwrap(),
            "top"
        );
        assert!(!s.status_is_error, "{}", s.status);

        // The password is remembered for this archive: no second prompt.
        s.run_line("select docs").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert!(tmp.path().join("right/docs/a.txt").exists());
    }

    #[test]
    fn cancelling_the_password_dialog_extracts_nothing() {
        let (tmp, mut s) = with_archives();
        s.run_line("select locked.zip").unwrap();
        s.run_line("unpack").unwrap();
        assert!(s.wait_idle());
        assert!(matches!(s.pending, Some(Pending::Input { .. })));
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_dir(tmp.path().join("right")).unwrap().count(),
            0
        );
        assert!(s.run_line("confirm").is_err(), "nothing left to retry");
    }

    #[test]
    fn a_password_on_the_command_line_skips_the_dialog() {
        let (tmp, mut s) = with_archives();
        s.run_line("select locked.zip").unwrap();
        s.run_line("unpack -ppw").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert!(tmp.path().join("right/docs/sub/b.txt").exists());
    }

    #[test]
    fn a_queued_job_waits_while_the_password_is_asked() {
        let (tmp, mut s) = with_archives();
        s.run_line("select locked.zip").unwrap();
        s.run_line("unpack").unwrap();
        s.run_line("select box.zip").unwrap();
        s.run_line("unpack -s").unwrap(); // queued behind the first job
        assert!(!s.wait_idle(), "held back by the password question");
        assert!(matches!(s.pending, Some(Pending::Input { .. })));
        assert!(
            !tmp.path().join("right/box").exists(),
            "held back until the question is answered"
        );
        type_into_dialog(&mut s, "pw");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/top.txt").exists());
        assert!(tmp.path().join("right/box/top.txt").exists());
    }

    #[test]
    fn cancelling_the_password_dialog_releases_the_queue() {
        let (tmp, mut s) = with_archives();
        s.run_line("select locked.zip").unwrap();
        s.run_line("unpack").unwrap();
        s.run_line("select box.zip").unwrap();
        s.run_line("unpack -s").unwrap();
        assert!(!s.wait_idle(), "held back by the password question");
        s.run_line("cancel").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/box/top.txt").exists());
        assert!(!tmp.path().join("right/top.txt").exists());
    }

    fn pack_form(s: &mut Session) -> &mut PackForm {
        match &mut s.pending {
            Some(Pending::Pack(form)) => form,
            other => panic!("no pack dialog: {other:?}"),
        }
    }

    fn list_names(path: &Path) -> Vec<String> {
        let listing = bluepane_archive::list(path, None, &bluepane_ops::Progress::new()).unwrap();
        listing.entries.iter().map(|e| e.path.clone()).collect()
    }

    #[test]
    fn pack_with_a_name_creates_the_archive_in_the_other_panel() {
        let (tmp, mut s) = two_dirs();
        s.run_line("selmask *.txt").unwrap();
        s.run_line("pack both.zip").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("Spakowano: 2"), "{}", s.status);
        assert_eq!(
            list_names(&tmp.path().join("right/both.zip")),
            ["a.txt", "b.txt"]
        );
        assert!(tmp.path().join("left/a.txt").exists(), "the originals stay");
        assert!(s.panels[1].entries.iter().any(|e| e.name == "both.zip"));
    }

    #[test]
    fn pack_defaults_the_name_and_adds_the_extension_of_the_format() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("pack -ttgz").unwrap();
        assert!(s.wait_idle());
        assert!(
            tmp.path().join("right/a.tar.gz").exists(),
            "named after the single file"
        );
        s.run_line("select b.txt").unwrap();
        s.run_line("pack bundle -ttar").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/bundle.tar").exists());
        // The format follows the extension of the name when no -t is given.
        s.run_line("select a.txt").unwrap();
        s.run_line("pack named.tgz").unwrap();
        assert!(s.wait_idle());
        assert_eq!(list_names(&tmp.path().join("right/named.tgz")), ["a.txt"]);
    }

    #[test]
    fn output_names_get_the_right_extension() {
        assert_eq!(output_name("photos", Format::Zip), "photos.zip");
        assert_eq!(output_name("photos.zip", Format::Zip), "photos.zip");
        assert_eq!(output_name("Photos.ZIP", Format::Zip), "Photos.ZIP");
        assert_eq!(output_name("photos.tar", Format::Zip), "photos.zip");
        assert_eq!(output_name("photos.zip", Format::TarGz), "photos.tar.gz");
        assert_eq!(output_name("a.b.c", Format::Tar), "a.b.c.tar");
        assert_eq!(output_name(" x ", Format::Tar), "x.tar");
        assert_eq!(output_name("data.tgz", Format::TarGz), "data.tgz");
    }

    #[test]
    fn pack_dialog_is_prefilled_and_validated_before_starting() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("pack").unwrap();
        let form = pack_form(&mut s);
        assert_eq!(form.name, "a");
        assert_eq!(form.format, Format::Zip);
        assert_eq!(form.output_name(), "a.zip");
        assert_eq!(form.destination(), tmp.path().join("right"));
        assert_eq!(form.item_count(), 1);

        form.password = "one".into();
        form.password_again = "two".into();
        assert!(s.run_line("confirm").unwrap_err().0.contains("różnią"));
        assert!(pack_form(&mut s)
            .error
            .as_deref()
            .unwrap()
            .contains("różnią"));
        let form = pack_form(&mut s);
        form.password_again = "one".into();
        form.format = Format::Tar;
        assert!(s.run_line("confirm").unwrap_err().0.contains("TAR"));
        let form = pack_form(&mut s);
        form.format = Format::Zip;
        form.split = "lots".into();
        assert!(s.run_line("confirm").is_err());
        let form = pack_form(&mut s);
        form.split = String::new();
        form.name = String::new();
        assert!(s.run_line("confirm").unwrap_err().0.contains("nazwę"));
        let form = pack_form(&mut s);
        form.name = "bad/name".into();
        assert!(s.run_line("confirm").is_err());
        assert!(
            !tmp.path().join("right/a.zip").exists(),
            "nothing started yet"
        );

        let form = pack_form(&mut s);
        form.name = "secret".into();
        s.run_line("confirm").unwrap();
        assert!(s.pending.is_none());
        assert!(s.wait_idle());
        let archive = tmp.path().join("right/secret.zip");
        assert!(
            bluepane_archive::list(&archive, None, &bluepane_ops::Progress::new())
                .unwrap()
                .has_encrypted()
        );
    }

    #[test]
    fn cancelling_the_pack_dialog_creates_nothing() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("pack").unwrap();
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_dir(tmp.path().join("right")).unwrap().count(),
            0
        );
    }

    #[test]
    fn a_packed_password_zip_round_trips_through_unpack() {
        let (tmp, mut s) = two_dirs();
        s.run_line("selmask *").unwrap();
        s.run_line("pack sealed.zip -psekret -mx=9").unwrap();
        assert!(s.wait_idle());
        s.run_line("switch").unwrap();
        s.run_line("select sealed.zip").unwrap();
        s.run_line("unpack -s -psekret").unwrap();
        // The panels swapped roles: extract into the (now other) left panel's directory.
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/sealed/a.txt")).unwrap(),
            "a"
        );
    }

    #[test]
    fn pack_splits_into_volumes_that_open_from_the_first() {
        let (tmp, mut s) = two_dirs();
        let big: Vec<u8> = (0..60_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(tmp.path().join("left/big.bin"), &big).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select big.bin").unwrap();
        s.run_line("pack parts.zip -v20k -mx0").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("części"), "{}", s.status);
        assert!(tmp.path().join("right/parts.zip.001").exists());
        assert!(tmp.path().join("right/parts.zip.003").exists());
        assert!(!tmp.path().join("right/parts.zip").exists());

        s.run_line("switch").unwrap();
        s.run_line("select parts.zip.001").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert_eq!(s.active_panel().entries[1].name, "big.bin");
        assert_eq!(s.active_panel().entries[1].size, 60_000);
    }

    #[test]
    fn an_existing_archive_name_triggers_the_conflict_dialog() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("right/x.zip"), "precious").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("pack x.zip").unwrap();
        assert!(!s.wait_idle());
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        s.run_line("keepboth").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/x.zip")).unwrap(),
            "precious"
        );
        assert!(tmp.path().join("right/x (kopia).zip").exists());

        s.run_line("pack x.zip").unwrap();
        assert!(!s.wait_idle());
        s.run_line("skip").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("już istnieje"), "{}", s.status);
    }

    #[test]
    fn pack_needs_a_selection_and_a_plain_directory_on_the_other_side() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("pack x.zip").is_err(), "cursor is on ..");
        s.run_line("select a.txt").unwrap();
        // Make the other panel show an archive.
        std::fs::write(_tmp.path().join("right/in.zip"), "x").unwrap();
        s.run_line("switch").unwrap();
        s.run_line("refresh").unwrap();
        let make = |s: &mut Session| {
            let src = _tmp.path().join("left/b.txt");
            make_archive(
                &[src],
                &_tmp.path().join("right/real.zip"),
                bluepane_archive::Format::Zip,
                None,
            );
            s.run_line("refresh").unwrap();
        };
        make(&mut s);
        s.run_line("select real.zip").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        s.run_line("switch").unwrap();
        s.run_line("select a.txt").unwrap();
        let err = s.run_line("pack x.zip").unwrap_err();
        assert!(err.0.contains("archiwum"), "{err}");
    }

    #[test]
    fn testarchive_reports_intact_damaged_and_password_protected_archives() {
        let (tmp, mut s) = with_archives();
        s.run_line("select box.zip").unwrap();
        s.run_line("testarchive").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("w porządku"), "{}", s.status);
        assert!(!s.status_is_error);

        s.run_line("select locked.zip").unwrap();
        s.run_line("testarchive").unwrap();
        assert!(s.wait_idle());
        assert!(
            matches!(s.pending, Some(Pending::Input { .. })),
            "asks for the password"
        );
        type_into_dialog(&mut s, "pw");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("w porządku"), "{}", s.status);

        // Damage a stored archive (data that does not also appear in a file name).
        let src = tmp.path().join("src/zeta.txt");
        std::fs::write(&src, "UNIQUEDATA").unwrap();
        let options = bluepane_archive::CreateOptions {
            format: bluepane_archive::Format::Zip,
            level: bluepane_archive::Level::Store,
            password: None,
            split: None,
            encrypt_names: false,
        };
        bluepane_archive::create(
            &[src],
            &tmp.path().join("left/dam.zip"),
            &options,
            &bluepane_ops::Progress::new(),
            &mut bluepane_ops::FailOnConflict,
        )
        .unwrap();
        let path = tmp.path().join("left/dam.zip");
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes.windows(10).position(|w| w == b"UNIQUEDATA").unwrap();
        bytes[at] ^= 0xff;
        std::fs::write(&path, bytes).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select dam.zip").unwrap();
        s.run_line("testarchive").unwrap();
        assert!(s.wait_idle());
        assert!(s.status_is_error, "{}", s.status);
        assert!(s.status.contains("Uszkodzone"), "{}", s.status);
    }

    #[test]
    fn tar_archives_browse_and_extract_too() {
        let (tmp, mut s) = with_archives();
        let src = tmp.path().join("src");
        make_archive(
            &[src.join("docs")],
            &tmp.path().join("left/t.tar.gz"),
            bluepane_archive::Format::TarGz,
            None,
        );
        s.run_line("refresh").unwrap();
        enter_archive(&mut s, "t.tar.gz");
        assert_eq!(rows(&s), ["..", "docs"]);
        s.run_line("select docs").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("right/docs/sub/b.txt")).unwrap(),
            "bravo"
        );
    }

    #[test]
    fn changing_directory_leaves_the_archive_and_keeps_other_tabs() {
        let (tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("tab new").unwrap();
        assert!(
            s.active_panel().archive.is_none(),
            "a new tab shows the real directory"
        );
        assert_eq!(s.tab_strip(0).titles, ["box.zip", "left"]);
        s.run_line("tab 1").unwrap();
        assert!(
            s.active_panel().archive.is_some(),
            "the first tab is still inside the archive"
        );
        s.run_line(&format!("cd {}", tmp.path().display())).unwrap();
        assert!(s.active_panel().archive.is_none());
    }

    fn preview_dirs() -> Vec<PathBuf> {
        std::fs::read_dir(std::env::temp_dir())
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name().is_some_and(|n| {
                            n.to_string_lossy()
                                .starts_with(&format!("bluepane-view-{}-", std::process::id()))
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn f3_on_a_file_inside_an_archive_extracts_it_and_shows_it() {
        let (_tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select docs").unwrap();
        s.run_line("enter").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("view").unwrap();
        assert!(s.wait_idle());
        assert!(
            s.status.contains("Podgląd z archiwum: a.txt"),
            "{}",
            s.status
        );
        let viewer = s.viewer.as_mut().expect("the viewer opened");
        viewer.set_viewport(5, 80);
        assert_eq!(viewer.visible_rows()[0].text, "alpha");
        let shown = viewer.path().to_path_buf();
        assert!(shown.exists());
        assert!(shown.starts_with(std::env::temp_dir()), "{shown:?}");
        assert!(
            s.active_panel().archive.is_some(),
            "the panel stays in the archive"
        );

        s.run_line("viewer close").unwrap();
        assert!(s.viewer.is_none());
        assert!(!shown.exists(), "the temporary file is removed on close");
        assert!(!shown.parent().unwrap().exists(), "and so is its directory");
    }

    #[test]
    fn previewing_inside_an_archive_asks_for_a_missing_password() {
        let (_tmp, mut s) = with_archives();
        enter_archive(&mut s, "locked.zip");
        s.run_line("select top.txt").unwrap();
        s.run_line("view").unwrap();
        assert!(s.wait_idle());
        assert!(s.viewer.is_none());
        assert!(matches!(
            s.pending,
            Some(Pending::Input {
                kind: InputKind::ArchivePassword,
                ..
            })
        ));
        type_into_dialog(&mut s, "pw");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        let viewer = s.viewer.as_mut().expect("opens once the password is right");
        viewer.set_viewport(5, 80);
        assert_eq!(viewer.visible_rows()[0].text, "top");
        s.run_line("viewer close").unwrap();
    }

    #[test]
    fn cancelling_the_preview_password_leaves_no_temporary_files() {
        let (_tmp, mut s) = with_archives();
        let before = preview_dirs().len();
        enter_archive(&mut s, "locked.zip");
        s.run_line("select top.txt").unwrap();
        s.run_line("view").unwrap();
        assert!(s.wait_idle());
        s.run_line("cancel").unwrap();
        assert!(s.wait_idle());
        assert!(s.viewer.is_none());
        assert_eq!(
            preview_dirs().len(),
            before,
            "nothing was left in the temp directory"
        );
    }

    #[test]
    fn pack_to_7z_with_hidden_names_then_open_it_with_the_password() {
        let (tmp, mut s) = two_dirs();
        s.run_line("selmask *.txt").unwrap();
        s.run_line("pack vault -t7z -psekret -mhe").unwrap();
        assert!(s.wait_idle());
        assert!(s.status.contains("Spakowano: 2"), "{}", s.status);
        assert!(tmp.path().join("right/vault.7z").exists());

        // Open it from the other side: the names are hidden, so even opening asks.
        s.run_line("switch").unwrap();
        s.run_line("select vault.7z").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert!(
            matches!(
                s.pending,
                Some(Pending::Input {
                    kind: InputKind::ArchivePassword,
                    ..
                })
            ),
            "{:?}",
            s.pending
        );
        assert!(s.active_panel().archive.is_none());
        type_into_dialog(&mut s, "wrong");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(s.status_is_error, "{}", s.status);
        assert!(
            matches!(s.pending, Some(Pending::Input { .. })),
            "asks again"
        );

        type_into_dialog(&mut s, "sekret");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(s.active_panel().archive.is_some(), "{}", s.status);
        assert_eq!(rows(&s), ["..", "a.txt", "b.txt"]);

        // The password given to read the names also covers extracting: no second prompt.
        // (The originals are gone, so nothing clashes in the destination.)
        for name in ["a.txt", "b.txt"] {
            std::fs::remove_file(tmp.path().join("left").join(name)).unwrap();
        }
        s.run_line("refresh").unwrap();
        s.run_line("selectall").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a.txt")).unwrap(),
            "a"
        );
    }

    #[test]
    fn pack_to_7z_with_a_visible_password_asks_only_when_extracting() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("pack lock.7z -ppw -mx=9").unwrap();
        assert!(s.wait_idle());
        s.run_line("switch").unwrap();
        s.run_line("select lock.7z").unwrap();
        s.run_line("enter").unwrap();
        assert!(s.wait_idle());
        assert!(
            s.pending.is_none(),
            "the names are readable without the password"
        );
        assert!(s.status.contains("zaszyfrowane"), "{}", s.status);
        // The original is removed so that nothing clashes in the destination.
        std::fs::remove_file(tmp.path().join("left/a.txt")).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(matches!(s.pending, Some(Pending::Input { .. })));
        type_into_dialog(&mut s, "pw");
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a.txt")).unwrap(),
            "a"
        );
    }

    #[test]
    fn previewing_a_directory_or_a_huge_entry_is_refused() {
        let (_tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select docs").unwrap();
        assert!(s.run_line("view").unwrap_err().0.contains("katalog"));
        s.run_line("cursor -10").unwrap(); // onto ".."
        assert!(s.run_line("view").is_err());
        s.run_line("select top.txt").unwrap();
        s.view_from_archive_limit = 1;
        let err = s.run_line("view").unwrap_err();
        assert!(err.0.contains("F5"), "{err}");
        assert!(s.wait_idle());
        assert!(s.viewer.is_none());
    }

    #[test]
    fn a_new_preview_replaces_the_old_one_and_dropping_the_session_cleans_up() {
        let (_tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        s.run_line("select top.txt").unwrap();
        s.run_line("view").unwrap();
        assert!(s.wait_idle());
        let first = s.viewer.as_ref().unwrap().path().to_path_buf();
        s.run_line("viewer close").unwrap();
        assert!(!first.exists());

        s.run_line("view").unwrap();
        assert!(s.wait_idle());
        let second = s.viewer.as_ref().unwrap().path().to_path_buf();
        assert!(second.exists());
        drop(s);
        assert!(!second.exists(), "dropping the session removes the preview");
    }

    #[test]
    fn previewing_works_for_tar_too() {
        let (tmp, mut s) = with_archives();
        let src = tmp.path().join("src");
        make_archive(
            &[src.join("top.txt")],
            &tmp.path().join("left/p.tar.gz"),
            bluepane_archive::Format::TarGz,
            None,
        );
        s.run_line("refresh").unwrap();
        enter_archive(&mut s, "p.tar.gz");
        s.run_line("select top.txt").unwrap();
        s.run_line("view").unwrap();
        assert!(s.wait_idle());
        let status = s.status.clone();
        let viewer = s.viewer.as_mut().unwrap_or_else(|| panic!("{status}"));
        viewer.set_viewport(3, 40);
        assert_eq!(viewer.visible_rows()[0].text, "top");
        s.run_line("viewer close").unwrap();
    }

    #[test]
    fn the_pack_dialog_validates_7z_options() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("pack").unwrap();
        let form = pack_form(&mut s);
        form.format = Format::SevenZ;
        assert_eq!(form.output_name(), "a.7z");
        form.encrypt_names = true;
        assert!(
            s.run_line("confirm").unwrap_err().0.contains("hasła"),
            "needs a password"
        );
        let form = pack_form(&mut s);
        form.password = "pw".into();
        form.password_again = "pw".into();
        form.format = Format::Zip;
        assert!(
            s.run_line("confirm").unwrap_err().0.contains("7z"),
            "ZIP cannot hide names"
        );
        let form = pack_form(&mut s);
        form.format = Format::Tar;
        form.encrypt_names = false;
        assert!(s.run_line("confirm").unwrap_err().0.contains("TAR"));
        let form = pack_form(&mut s);
        form.format = Format::SevenZ;
        form.encrypt_names = true;
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.7z").exists());
    }

    #[test]
    fn output_names_know_the_7z_extension() {
        assert_eq!(output_name("data", Format::SevenZ), "data.7z");
        assert_eq!(output_name("data.7z", Format::SevenZ), "data.7z");
        assert_eq!(output_name("data.zip", Format::SevenZ), "data.7z");
        assert_eq!(output_name("data.7z", Format::Zip), "data.zip");
    }

    // ---- synchronization ------------------------------------------------------------

    fn sync_dirs() -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("right/only_right.txt"), "r").unwrap();
        s.run_line("refresh").unwrap();
        (tmp, s)
    }

    fn versions(tmp: &tempfile::TempDir) -> String {
        format!("-delete=versions:{}", tmp.path().join("versions").display())
    }

    fn plan_ops(s: &Session) -> Vec<(String, crate::SyncOp)> {
        s.sync
            .as_ref()
            .and_then(|v| v.preview.as_ref())
            .map(|p| {
                p.plan
                    .actions
                    .iter()
                    .map(|a| (a.path.clone(), a.op.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn sync_compares_without_writing_anything() {
        let (tmp, mut s) = sync_dirs();
        s.run_line("sync -mirror").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        assert!(s.status.starts_with("Różnice: 3"), "{}", s.status);
        let ops = plan_ops(&s);
        assert!(ops.contains(&("a.txt".to_string(), crate::SyncOp::CopyToRight)));
        assert!(ops.contains(&("only_right.txt".to_string(), crate::SyncOp::DeleteRight)));
        assert!(!tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("right/only_right.txt").exists());
    }

    #[test]
    fn syncrun_mirrors_and_then_shows_that_nothing_is_left() {
        let (tmp, mut s) = sync_dirs();
        s.run_line(&format!("sync -mirror {}", versions(&tmp)))
            .unwrap();
        assert!(s.wait_idle());
        s.run_line("syncrun").unwrap();
        assert!(s.wait_idle());
        assert!(!s.status_is_error, "{}", s.status);
        let right = tmp.path().join("right");
        assert_eq!(std::fs::read_to_string(right.join("a.txt")).unwrap(), "a");
        assert!(!right.join("only_right.txt").exists());
        assert!(tmp.path().join("versions/only_right.txt").exists());
        let view = s.sync.as_ref().unwrap();
        assert!(view
            .summary
            .as_deref()
            .unwrap()
            .contains("skopiowano 2, usunięto 1"));
        // The window was refreshed after the run: the folders are the same now.
        assert!(view.preview.as_ref().unwrap().plan.actions.is_empty());
        // Both panels show the new content.
        assert!(s.panels[1].entries.iter().any(|e| e.name == "b.txt"));
        assert!(s.run_line("syncrun").is_err(), "nothing left to do");
    }

    #[test]
    fn a_row_can_be_redirected_before_running() {
        let (tmp, mut s) = sync_dirs();
        s.run_line("sync -update").unwrap();
        assert!(s.wait_idle());
        // Update skips right-only files; send this one to the left instead.
        let row = plan_ops(&s)
            .iter()
            .position(|(p, _)| p == "only_right.txt")
            .unwrap();
        s.run_line(&format!("syncset {} left", row + 1)).unwrap();
        s.run_line("syncrun").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("left/only_right.txt").exists());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(s.run_line("syncset 99 left").is_err());
    }

    #[test]
    fn filters_and_comparison_options_reach_the_engine() {
        let (tmp, mut s) = sync_dirs();
        std::fs::write(tmp.path().join("left/skip.tmp"), "t").unwrap();
        s.run_line("sync -update -x*.tmp -size").unwrap();
        assert!(s.wait_idle());
        assert!(!plan_ops(&s).iter().any(|(p, _)| p == "skip.tmp"));
        // Later calls keep the earlier settings unless told otherwise.
        s.run_line("sync -twoway").unwrap();
        assert!(s.wait_idle());
        let view = s.sync.as_ref().unwrap();
        assert_eq!(view.exclude, "*.tmp");
        assert_eq!(view.settings.compare, crate::CompareBy::Size);
        assert!(!plan_ops(&s).iter().any(|(p, _)| p == "skip.tmp"));
    }

    #[test]
    fn sync_refuses_unsafe_setups() {
        let (tmp, mut s) = sync_dirs();
        // Same directory in both panels.
        s.run_line("switch").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        let err = s.run_line("sync").unwrap_err();
        assert!(err.0.contains("ten sam katalog"), "{err}");
        // One inside the other.
        std::fs::create_dir(tmp.path().join("left/inner")).unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left/inner").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        let err = s.run_line("sync").unwrap_err();
        assert!(err.0.contains("w drugim"), "{err}");
        assert!(s.sync.is_none());
        assert!(s.run_line("syncrun").is_err());
        assert!(s.run_line("syncset 1 left").is_err());
    }

    #[test]
    fn sync_does_not_work_inside_an_archive() {
        let (_tmp, mut s) = with_archives();
        enter_archive(&mut s, "box.zip");
        let err = s.run_line("sync").unwrap_err();
        assert!(err.0.contains("archiwum"), "{err}");
    }

    #[test]
    fn closing_the_sync_window_and_the_default_key() {
        let (_tmp, mut s) = sync_dirs();
        s.run_line("sync").unwrap();
        assert!(s.wait_idle());
        assert!(s.sync.is_some());
        s.run_line("syncclose").unwrap();
        assert!(s.sync.is_none());
        assert_eq!(s.keymap.lookup("Alt+S"), Some("sync"));
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(left.join("folder.v2")).unwrap();
        std::fs::write(left.join("report.final.txt"), "x").unwrap();
        std::fs::write(left.join("Makefile"), "x").unwrap();
        std::fs::write(left.join("data.csv"), "x").unwrap();
        let session = Session::new(left, tmp.path().join("right"));
        (tmp, session)
    }

    #[test]
    fn names_are_copied_in_three_shapes_one_per_line() {
        let (tmp, mut s) = session();
        s.run_line("select data.csv").unwrap();
        s.run_line("select report.final.txt").unwrap();
        s.run_line("select folder.v2").unwrap();
        s.run_line("select Makefile").unwrap();
        s.run_line("copynames stem").unwrap();
        assert_eq!(
            s.take_clipboard().as_deref(),
            Some("folder.v2\ndata\nMakefile\nreport.final"),
            "folders and files without an extension keep their whole name"
        );
        assert!(s.take_clipboard().is_none(), "taken once");
        s.run_line("copynames file").unwrap();
        assert_eq!(
            s.take_clipboard().as_deref(),
            Some("folder.v2\ndata.csv\nMakefile\nreport.final.txt")
        );
        s.run_line("copynames").unwrap();
        assert!(
            s.take_clipboard().is_some(),
            "the default is the name with its extension"
        );
        s.run_line("copynames path").unwrap();
        let paths = s.take_clipboard().unwrap();
        let first = tmp.path().join("left").join("folder.v2");
        assert_eq!(paths.lines().next().unwrap(), first.to_string_lossy());
        assert_eq!(paths.lines().count(), 4);
        assert!(s.status.contains("pełnych ścieżek"), "{}", s.status);
    }

    #[test]
    fn without_a_selection_the_entry_under_the_cursor_is_used() {
        let (_tmp, mut s) = session();
        s.run_line("select data.csv").unwrap();
        s.run_line("unselectall").unwrap();
        s.run_line("copynames file").unwrap();
        assert_eq!(s.take_clipboard().as_deref(), Some("data.csv"));
    }

    #[test]
    fn nothing_to_copy_and_bad_words_are_errors() {
        let (_tmp, mut s) = session();
        assert!(s.run_line("copynames").is_err(), "the cursor is on ..");
        assert!(s.take_clipboard().is_none());
        assert!(s.run_line("copynames bogus").is_err());
        assert_eq!(s.keymap.lookup("Ctrl+Shift+Num1"), Some("copynames stem"));
        assert_eq!(s.keymap.lookup("Ctrl+Shift+Num2"), Some("copynames file"));
        assert_eq!(s.keymap.lookup("Ctrl+Shift+Num3"), Some("copynames path"));
    }
}
