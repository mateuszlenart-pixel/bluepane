use std::path::PathBuf;

use bluepane_archive::{Format, Level};
use bluepane_ops::Resolution;
use bluepane_sync::{CompareBy, Deletion, Direction, Variant};
use std::time::Duration;

use crate::pageview::{PageMove, ZoomChange};
use crate::{ClickMode, EngineError, SortKey};

/// Names accepted at the command line (aliases excluded).
pub const COMMAND_NAMES: &[&str] = &[
    "switch",
    "up",
    "enter",
    "cursor",
    "toggle",
    "click",
    "select",
    "cd",
    "back",
    "forward",
    "volume",
    "filter",
    "refresh",
    "copy",
    "move",
    "mkdir",
    "chmod",
    "delete",
    "confirm",
    "cancel",
    "selmask",
    "unselmask",
    "selectall",
    "unselectall",
    "invert",
    "sort",
    "sync",
    "sites",
    "siteset",
    "sitedel",
    "editsend",
    "page",
    "pagezoom",
    "pageclose",
    "copynames",
    "theme",
    "dirsize",
    "quick",
    "bind",
    "unbind",
    "scan",
    "scanset",
    "scanrun",
    "scanstop",
    "scanclose",
    "scanselect",
    "scanfav",
    "scanfavorites",
    "scancopy",
    "scanshares",
    "scanshare",
    "scanopen",
    "scansite",
    "rename",
    "stop",
    "syncrun",
    "syncset",
    "syncclose",
    "view",
    "edit",
    "viewer",
    "tab",
    "pack",
    "unpack",
    "testarchive",
    "palette",
    "jump",
    "go",
    "dragto",
    "drop",
    "menu",
    "open",
    "info",
    "places",
    "fav",
    "queue",
    "queueclose",
    "queuepause",
    "queueresume",
    "queuedel",
    "queuemove",
    "limit",
    "queueretry",
    "queueclear",
    "sitepassword",
    "siteclose",
    "connect",
    "disconnect",
    "find",
    "findset",
    "findrun",
    "findstop",
    "findgo",
    "findview",
    "findclose",
    "findpanel",
    "multirename",
    "mrset",
    "mrname",
    "mrrun",
    "mrundo",
    "mrclose",
];

/// What `copynames` puts on the clipboard for each selected entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFormat {
    /// The name without its extension (`report`).
    Stem,
    /// The name with its extension (`report.txt`).
    File,
    /// The full path (`/home/me/docs/report.txt`).
    Path,
}

/// What the command palette should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// Open it (or change the text when it is open).
    Query(String),
    Close,
    /// Move the highlight by this many rows.
    Move(i32),
    /// Run the highlighted command.
    Run,
}

/// What the open file viewer should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerAction {
    /// Scroll by rows (negative = up).
    Scroll(i64),
    /// Scroll by pages (negative = up).
    Page(i64),
    /// Scroll sideways by characters.
    HScroll(i64),
    Top,
    Bottom,
    /// Switch between text and hex.
    Mode,
    /// Jump to a byte offset or a percentage such as `50%`.
    Goto(String),
    /// Search forward; without text a dialog asks for it.
    Find(Option<String>),
    /// Repeat the search forward / backward.
    Next,
    Prev,
    Close,
}

/// What to do with the active side's tabs. Numbers are 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabAction {
    /// New tab showing the same directory (Ctrl+T).
    New,
    /// New tab showing the directory under the cursor (Ctrl+Up).
    Open,
    /// Close a tab; the active one without a number (Ctrl+W).
    Close(Option<usize>),
    Next,
    Prev,
    /// Switch to tab number n (Ctrl+1 ... Ctrl+9).
    Goto(usize),
}

/// Settings given to `pack` on the command line (7-Zip style switches).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackArgs {
    /// Archive name; empty means "choose a default from the selection".
    pub name: String,
    /// `-tzip`, `-ttar`, `-ttgz`; by default taken from the name's extension, else ZIP.
    pub format: Option<Format>,
    /// `-mx=0` (store) to `-mx=9` (maximum).
    pub level: Option<Level>,
    /// `-p<password>`: ZIP only (AES-256).
    pub password: Option<String>,
    /// `-v<size>`: cut into volumes of this many bytes, e.g. `-v100m`.
    pub split: Option<u64>,
    /// `-mhe`: 7z only, hide the file names too (needs `-p`).
    pub encrypt_names: bool,
}

/// Settings given to `unpack`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnpackArgs {
    /// `-p<password>`
    pub password: Option<String>,
    /// `-s`: extract into a subfolder named after the archive.
    pub subfolder: bool,
    /// `-h`: extract next to the archive ("here") instead of into the other panel.
    pub here: bool,
}

/// What `quick` does: switch Quick View on or off, scroll its text, or turn its pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAction {
    Toggle,
    Scroll(i64),
    Page(i32),
}

/// What `limit` was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedLimit {
    Show,
    Off,
    BytesPerSecond(u64),
}

/// Where `queuemove` puts a waiting job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMove {
    Up,
    Down,
    Top,
}

/// Settings given to `sync`; whatever is left out keeps its previous (or default) value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncArgs {
    /// `-twoway`, `-mirror`, `-update`.
    pub variant: Option<Variant>,
    /// `-time`, `-size`, `-content`.
    pub compare: Option<CompareBy>,
    /// `-tol=<seconds>`: how far apart two modification times may be and still count as equal.
    pub tolerance: Option<u64>,
    /// `-i<patterns>`: only these names take part (`;` separates patterns).
    pub include: Option<String>,
    /// `-x<patterns>`: these names never take part.
    pub exclude: Option<String>,
    /// `-delete=trash|permanent|versions:<folder>`.
    pub deletion: Option<Deletion>,
}

/// One user action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Move focus to the other panel (Tab).
    SwitchPanel,
    /// Go to the parent directory (Backspace). With a quick filter active, removes its
    /// last character instead.
    Up,
    /// Enter the directory under the cursor (Enter).
    Enter,
    /// Move the cursor by a number of rows (arrows).
    Cursor(i32),
    /// Toggle selection of the entry under the cursor and move down (Space, Insert).
    ToggleSelect,
    /// A mouse click on a row of the active panel (1-based), with the keys held down.
    Click(usize, ClickMode),
    /// Put the cursor on the entry with this name and select it.
    Select(String),
    /// Change directory in the active panel.
    Cd(PathBuf),
    /// Previous directory in the active panel's history (Alt+Left).
    Back,
    /// Next directory in the active panel's history (Alt+Right).
    Forward,
    /// Jump the active panel to a volume, given as a 1-based number or a name (Alt+1..9).
    Volume(String),
    /// Set the quick filter of the active panel; empty text clears it (Escape).
    Filter(String),
    Refresh,
    /// Copy the selection (or the entry under the cursor) to the other panel (F5).
    /// When both panels show the same directory this proposes duplicates instead.
    Copy,
    /// Select the visible entries matching a mask such as `*.txt;*.md` (Ctrl+M).
    SelectMask(String),
    /// Unselect the visible entries matching a mask (Ctrl+Shift+M).
    UnselectMask(String),
    /// Select every visible entry (Ctrl+A).
    SelectAll,
    /// Clear the selection (Ctrl+Shift+A).
    UnselectAll,
    /// Invert the selection of the visible entries (Ctrl+I).
    InvertSelection,
    /// Order the active panel; `None` direction flips on a repeated key (Ctrl+F3..F6).
    Sort(SortKey, Option<bool>),
    /// Accept the pending proposal (Enter while a question is shown).
    Confirm,
    /// Reject the pending proposal (Escape while a question is shown).
    Cancel,
    /// Move the selection to the other panel (F6).
    Move,
    /// Create a directory in the active panel (F7). Without a name a dialog asks for it.
    Mkdir(Option<String>),
    /// Set the Unix permission bits (octal, e.g. `755`) of the selection on a server.
    Chmod(u32),
    /// Open, close or switch tabs of the active panel.
    Tab(TabAction),
    /// Pack the selection into an archive in the other panel's directory (Alt+F5). Without
    /// arguments a dialog asks for the settings.
    Pack(Option<PackArgs>),
    /// Extract an archive into the other panel's directory (Alt+F9).
    Unpack(UnpackArgs),
    /// Check that an archive is intact; the optional text is its password.
    TestArchive(Option<String>),
    /// Open the file under the cursor in the built-in viewer (F3).
    View,
    /// Open the file under the cursor in an external editor (F4).
    Edit,
    /// Control the open viewer (keys while it is open).
    Viewer(ViewerAction),
    /// Control the command palette (F1).
    Palette(PaletteAction),
    /// Control the "jump to" list (Ctrl+P); it takes the same actions as the palette.
    Jump(PaletteAction),
    /// What the path bar does with its text: go to a path or server, search, run a command.
    Go(String),
    /// Drag the active panel's marked entries (copy or move) to a panel or a folder row of it.
    DragTo(bool, crate::DropTarget),
    /// Files dropped from outside into a panel (or a folder row of it); they are copied.
    Drop(crate::DropTarget, Vec<PathBuf>),
    /// Open the context menu of the active panel (Shift+F10).
    Menu,
    /// Open the file under the cursor in the system's default application.
    Open,
    /// Describe the entry under the cursor or the marked ones in the status line.
    Info,
    /// Show or hide the Places bar (Ctrl+B).
    Places,
    /// Add a folder to the favorites or remove it.
    Favorite(crate::FavoriteAction),
    /// Rename the entry under the cursor (F2). Without a name a dialog asks for it.
    Rename(Option<String>),
    /// Move the selection to the trash (F8), after a confirmation.
    Delete,
    /// Answer a file conflict; the flag applies the answer to every later conflict of the job.
    Resolve(Resolution, bool),
    /// Cancel the running job and drop the queued ones.
    StopJob,
    /// Compare the two panels' directories and show what a synchronization would do
    /// (Alt+S). Nothing is changed.
    Sync(SyncArgs),
    /// Carry out the plan shown by `sync`.
    SyncRun,
    /// Override the direction of row `n` (1-based) of the plan: `right`, `left` or `skip`.
    SyncSet(usize, Direction),
    /// Close the synchronization view.
    SyncClose,
    /// Open the site manager (Ctrl+S).
    Sites,
    /// Create a saved site or change one field of it: `siteset Proxmox host 10.0.0.2`.
    SiteSet(String, String, String),
    SiteDelete(String),
    /// Open the network scanner (Ctrl+N), or show the last one again.
    Scan,
    /// Change one setting of the scanner: `scanset range 192.168.0.1-254`.
    ScanSet(String, String),
    ScanRun,
    ScanStop,
    ScanClose,
    /// Select a host of the scan results (the actions below the table work on it).
    ScanSelect(std::net::Ipv4Addr),
    /// Add a host to the favorites or take it out.
    ScanFavorite(std::net::Ipv4Addr),
    /// Show the favorites (`true`) or the scan results (`false`).
    ScanFavorites(bool),
    /// Copy a host's address to the clipboard.
    ScanCopy(std::net::Ipv4Addr),
    /// Look for the shares of a host.
    ScanShares(std::net::Ipv4Addr),
    /// Open a share of a host in the file manager.
    ScanShare(std::net::Ipv4Addr, String),
    /// The usual thing to do with the service on a port (browser, remote desktop, ...).
    ScanOpen(std::net::Ipv4Addr, u16),
    /// Save a server for this host in the server manager, ready for a login.
    ScanSite(std::net::Ipv4Addr, bluepane_remote::Protocol),
    /// Open the queue window (Ctrl+J): what runs, what waits, what finished.
    Queue,
    QueueClose,
    /// Let the running job finish but start no further queued ones.
    QueuePause,
    QueueResume,
    /// Drop queued job n (1-based, in the order shown) before it starts.
    QueueDelete(usize),
    /// Limit the speed of copying and transfers, or show the current limit.
    Limit(SpeedLimit),
    /// Move queued job n (1-based, in the order shown) up, down or to the front.
    QueueMove(usize, QueueMove),
    /// Queue a finished job (n = 1 is the newest) once more.
    QueueRetry(usize),
    /// Forget the list of finished jobs.
    QueueClear,
    /// Turn the pages of the open document (PDF, or a document converted by LibreOffice).
    Page(PageMove),
    /// Zoom the open document: `pagezoom in`, `out`, `fit`, or a percentage.
    PageZoom(ZoomChange),
    /// Close the page view.
    PageClose,
    /// Copy the names (or paths) of the selected entries, one per line, to the clipboard.
    CopyNames(NameFormat),
    /// Bind a key to a command line, show a key, or list the changes (no arguments).
    Bind(Option<(String, Option<String>)>),
    /// Take a key off.
    Unbind(String),
    /// Quick View: show the file under the cursor in the other panel.
    Quick(QuickAction),
    /// Count the size of the selected folders (or the one under the cursor) in the background.
    DirSize,
    /// Show the colour theme and the available ones, or switch to another.
    Theme(Option<String>),
    /// Send the file edited with F4 on a server back to it (Ctrl+Shift+F4).
    EditSend,
    /// Save the password of a site where its secret is kept (system store, Bitwarden, or
    /// the sites file): `sitepassword Proxmox <password>`.
    SitePassword(String, String),
    /// Close the site manager.
    SiteClose,
    /// Connect the active panel to a saved site; the optional text is the password (or the
    /// key's passphrase).
    Connect(String, Option<String>),
    /// Leave the server and show the folder the panel was in before.
    Disconnect,
    /// Open the search window (Alt+F7), or show the last one again.
    Find,
    /// Change one setting of the search window: `findset text needle`.
    FindSet(String, String),
    /// Start searching with the settings of the window.
    FindRun,
    /// Stop a running search (the results found so far stay).
    FindStop,
    /// Go to result n (1-based) in the active panel and close the window.
    FindGo(usize),
    /// Open result n in the viewer.
    FindView(usize),
    /// Show every result as a list in the active panel (the results of a search as a
    /// virtual folder: copy, move, delete and view work on them).
    FindPanel,
    /// Hide the search window (the results are kept until the next search).
    FindClose,
    /// Open the multi-rename window for the selection (Ctrl+F2).
    MultiRename,
    /// Change one setting of the multi-rename window: `mrset name [N]_[C]`.
    MultiRenameSet(String, String),
    /// Set (or, without a name, clear) a hand-typed new name for row n (1-based).
    MultiRenameName(usize, Option<String>),
    /// Carry out the renames shown in the window.
    MultiRenameRun,
    /// Take back the last multi-rename run.
    MultiRenameUndo,
    /// Close the multi-rename window.
    MultiRenameClose,
}

impl Command {
    /// Parses a command line such as `copy`, `mkdir Reports` or `cd /tmp`.
    pub fn parse(line: &str) -> Result<Command, EngineError> {
        let line = line.trim();
        let (head, rest) = match line.split_once(char::is_whitespace) {
            Some((head, rest)) => (head, rest.trim()),
            None => (line, ""),
        };
        let need_arg = |what: &str| {
            if rest.is_empty() {
                Err(EngineError(format!("{head}: brak argumentu ({what})")))
            } else {
                Ok(rest.to_string())
            }
        };
        match head.to_ascii_lowercase().as_str() {
            "" => Err(EngineError("pusta komenda".into())),
            "switch" => Ok(Command::SwitchPanel),
            "up" => Ok(Command::Up),
            "enter" => Ok(Command::Enter),
            "toggle" => Ok(Command::ToggleSelect),
            "click" => parse_click(rest),
            "selmask" => need_arg("maska, np. *.txt").map(Command::SelectMask),
            "unselmask" => need_arg("maska, np. *.txt").map(Command::UnselectMask),
            "selectall" => Ok(Command::SelectAll),
            "unselectall" => Ok(Command::UnselectAll),
            "invert" => Ok(Command::InvertSelection),
            "sort" => parse_sort(rest),
            "confirm" | "yes" => Ok(Command::Confirm),
            "cancel" | "no" => Ok(Command::Cancel),
            "back" => Ok(Command::Back),
            "forward" => Ok(Command::Forward),
            "refresh" => Ok(Command::Refresh),
            "filter" => Ok(Command::Filter(rest.to_string())),
            "volume" | "drive" => need_arg("numer lub nazwa").map(Command::Volume),
            "copy" | "cp" => Ok(Command::Copy),
            "move" | "mv" => Ok(Command::Move),
            "delete" | "rm" => Ok(Command::Delete),
            "cursor" => rest
                .parse::<i32>()
                .map(Command::Cursor)
                .map_err(|_| EngineError("cursor: oczekiwano liczby, np. cursor -1".into())),
            "mkdir" => Ok(Command::Mkdir(optional(rest))),
            "chmod" => parse_chmod(rest).map(Command::Chmod),
            "rename" => Ok(Command::Rename(optional(rest))),
            "stop" => Ok(Command::StopJob),
            "sync" => parse_sync(rest).map(Command::Sync),
            "syncrun" => Ok(Command::SyncRun),
            "syncset" => parse_syncset(rest),
            "syncclose" => Ok(Command::SyncClose),
            "sites" => Ok(Command::Sites),
            "siteclose" => Ok(Command::SiteClose),
            "editsend" => Ok(Command::EditSend),
            "page" => parse_page(rest).map(Command::Page),
            "pagezoom" => parse_pagezoom(rest).map(Command::PageZoom),
            "pageclose" => Ok(Command::PageClose),
            "theme" => Ok(Command::Theme(optional(rest))),
            "dirsize" => Ok(Command::DirSize),
            "quick" => Command::parse_quick(rest),
            "bind" => Ok(Command::Bind(match rest.split_once(char::is_whitespace) {
                None if rest.is_empty() => None,
                None => Some((rest.to_string(), None)),
                Some((key, line)) => Some((key.to_string(), Some(line.trim().to_string()))),
            })),
            "unbind" => need_arg("klawisz").map(Command::Unbind),
            "copynames" => match rest.to_ascii_lowercase().as_str() {
                "" | "file" | "name" | "ext" => Ok(Command::CopyNames(NameFormat::File)),
                "stem" | "short" => Ok(Command::CopyNames(NameFormat::Stem)),
                "path" | "full" => Ok(Command::CopyNames(NameFormat::Path)),
                _ => Err(EngineError(
                    "copynames: użycie: copynames [stem|file|path] (bez rozszerzenia, z rozszerzeniem, pełna ścieżka)".into(),
                )),
            },
            "scan" => Ok(Command::Scan),
            "scanset" => parse_mrset(rest).map(|c| match c {
                Command::MultiRenameSet(f, v) => Command::ScanSet(f, v),
                other => other,
            }),
            "scanrun" => Ok(Command::ScanRun),
            "scanstop" => Ok(Command::ScanStop),
            "scanclose" => Ok(Command::ScanClose),
            "scanselect" => parse_ip(rest).map(Command::ScanSelect),
            "scanfav" => parse_ip(rest).map(Command::ScanFavorite),
            "scanfavorites" => Ok(Command::ScanFavorites(!matches!(rest, "off" | "0" | "nie"))),
            "scancopy" => parse_ip(rest).map(Command::ScanCopy),
            "scanshares" => parse_ip(rest).map(Command::ScanShares),
            "scanshare" => {
                let (ip, name) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                let name = name.trim().trim_matches('"');
                if name.is_empty() {
                    Err(EngineError("scanshare: użycie: scanshare <ip> <nazwa udziału>".into()))
                } else {
                    parse_ip(ip).map(|ip| Command::ScanShare(ip, name.to_string()))
                }
            }
            "scanopen" => {
                let mut words = rest.split_whitespace();
                match (words.next(), words.next().and_then(|p| p.parse().ok())) {
                    (Some(ip), Some(port)) => parse_ip(ip).map(|ip| Command::ScanOpen(ip, port)),
                    _ => Err(EngineError("scanopen: użycie: scanopen <ip> <port>".into())),
                }
            }
            "scansite" => {
                let mut words = rest.split_whitespace();
                let protocol = words
                    .nth(1)
                    .and_then(bluepane_remote::Protocol::from_word)
                    .ok_or_else(|| EngineError("scansite: użycie: scansite <ip> sftp|ftp|ftps".into()))?;
                parse_ip(rest.split_whitespace().next().unwrap_or("")).map(|ip| Command::ScanSite(ip, protocol))
            }
            "queue" => Ok(Command::Queue),
            "queueclose" => Ok(Command::QueueClose),
            "queuepause" => Ok(Command::QueuePause),
            "queueresume" => Ok(Command::QueueResume),
            "queuedel" => parse_row("queuedel", rest).map(Command::QueueDelete),
            "queuemove" => parse_queue_move(rest),
            "limit" => parse_limit(rest).map(Command::Limit),
            "queueretry" => parse_row("queueretry", rest).map(Command::QueueRetry),
            "queueclear" => Ok(Command::QueueClear),
            "siteset" => parse_siteset(rest),
            "sitedel" => take_name("sitedel", rest).map(|(name, _)| Command::SiteDelete(name)),
            "sitepassword" => take_name("sitepassword", rest).and_then(|(name, password)| {
                if password.is_empty() {
                    Err(EngineError(
                        "sitepassword: użycie: sitepassword <nazwa> <hasło>".into(),
                    ))
                } else {
                    Ok(Command::SitePassword(name, password))
                }
            }),
            "connect" => take_name("connect", rest)
                .map(|(name, tail)| Command::Connect(name, Some(tail).filter(|t| !t.is_empty()))),
            "disconnect" => Ok(Command::Disconnect),
            "find" => Ok(Command::Find),
            "findset" => parse_findset(rest),
            "findrun" => Ok(Command::FindRun),
            "findstop" => Ok(Command::FindStop),
            "findgo" => parse_row("findgo", rest).map(Command::FindGo),
            "findview" => parse_row("findview", rest).map(Command::FindView),
            "findclose" => Ok(Command::FindClose),
            "findpanel" => Ok(Command::FindPanel),
            "multirename" | "mr" => Ok(Command::MultiRename),
            "mrset" => parse_mrset(rest),
            "mrname" => parse_mrname(rest),
            "mrrun" => Ok(Command::MultiRenameRun),
            "mrundo" => Ok(Command::MultiRenameUndo),
            "mrclose" => Ok(Command::MultiRenameClose),
            "view" | "f3" => Ok(Command::View),
            "edit" => Ok(Command::Edit),
            "viewer" => parse_viewer(rest).map(Command::Viewer),
            "palette" => parse_palette(rest).map(Command::Palette),
            "jump" => parse_palette(rest).map(Command::Jump),
            "go" => need_arg("adres").map(Command::Go),
            "dragto" => parse_dragto(rest),
            "drop" => parse_drop(rest),
            "menu" => Ok(Command::Menu),
            "open" => Ok(Command::Open),
            "info" => Ok(Command::Info),
            "places" => Ok(Command::Places),
            "fav" => Ok(Command::Favorite(parse_favorite(rest))),
            "tab" => parse_tab(rest).map(Command::Tab),
            "pack" => parse_pack(rest),
            "unpack" => parse_unpack(rest).map(Command::Unpack),
            "testarchive" => parse_unpack(rest).map(|a| Command::TestArchive(a.password)),
            "select" => need_arg("nazwa").map(Command::Select),
            "cd" => need_arg("ścieżka").map(|p| Command::Cd(PathBuf::from(p))),
            other => parse_resolution(other)
                .map(|(resolution, all)| Command::Resolve(resolution, all))
                .ok_or_else(|| EngineError(format!("nieznana komenda: {other}"))),
        }
    }
}

/// A size such as `100`, `64k`, `700m`, `1.5g` (powers of 1024; a trailing `b` is optional).
pub fn parse_size(text: &str) -> Result<u64, EngineError> {
    let bad = || {
        EngineError(format!(
            "niepoprawny rozmiar: {text:?} (np. 100m, 700m, 4g)"
        ))
    };
    let lower = text.trim().to_ascii_lowercase();
    let lower = lower.strip_suffix('b').unwrap_or(&lower);
    let (number, unit) = match lower.chars().last() {
        Some('k') => (&lower[..lower.len() - 1], 1024.0),
        Some('m') => (&lower[..lower.len() - 1], 1024.0 * 1024.0),
        Some('g') => (&lower[..lower.len() - 1], 1024.0 * 1024.0 * 1024.0),
        Some('t') => (&lower[..lower.len() - 1], 1024.0 * 1024.0 * 1024.0 * 1024.0),
        _ => (lower, 1.0),
    };
    let value: f64 = number.trim().parse().map_err(|_| bad())?;
    let bytes = value * unit;
    if !bytes.is_finite() || bytes < 1.0 || bytes >= u64::MAX as f64 {
        return Err(bad());
    }
    Ok(bytes as u64)
}

/// Words and `-x` switches of a command line argument list.
fn split_switches(args: &str) -> (Vec<&str>, Vec<&str>) {
    let mut words = Vec::new();
    let mut switches = Vec::new();
    for token in args.split_whitespace() {
        if token.starts_with('-') && token.len() > 1 {
            switches.push(token);
        } else {
            words.push(token);
        }
    }
    (words, switches)
}

fn parse_format(text: &str) -> Option<Format> {
    match text.to_ascii_lowercase().as_str() {
        "zip" => Some(Format::Zip),
        "tar" => Some(Format::Tar),
        "tgz" | "tar.gz" | "targz" | "gz" => Some(Format::TarGz),
        "7z" => Some(Format::SevenZ),
        _ => None,
    }
}

fn parse_level(text: &str) -> Option<Level> {
    match text.parse::<u8>().ok()? {
        0 => Some(Level::Store),
        1..=3 => Some(Level::Fast),
        4..=6 => Some(Level::Normal),
        7..=9 => Some(Level::Maximum),
        _ => None,
    }
}

fn parse_pack(args: &str) -> Result<Command, EngineError> {
    if args.trim().is_empty() {
        return Ok(Command::Pack(None));
    }
    let usage = |what: &str| {
        EngineError(format!(
            "pack: {what} (użycie: pack [nazwa] [-tzip|-t7z|-ttar|-ttgz] [-mx=0..9] [-p<hasło>] [-mhe] [-v<rozmiar>])"
        ))
    };
    let (words, switches) = split_switches(args);
    let mut pack = PackArgs {
        name: words.join(" "),
        ..PackArgs::default()
    };
    for switch in switches {
        if let Some(password) = switch.strip_prefix("-p") {
            pack.password = Some(password.to_string());
        } else if let Some(size) = switch.strip_prefix("-v") {
            pack.split = Some(parse_size(size)?);
        } else if switch == "-mhe" || switch == "-mhe=on" {
            pack.encrypt_names = true;
        } else if let Some(name) = switch.strip_prefix("-t") {
            pack.format = Some(parse_format(name).ok_or_else(|| usage("nieznany format"))?);
        } else if let Some(level) = switch.strip_prefix("-mx") {
            let level = level.strip_prefix('=').unwrap_or(level);
            pack.level = Some(parse_level(level).ok_or_else(|| usage("poziom to liczba 0-9"))?);
        } else {
            return Err(usage(&format!("nieznany przełącznik {switch}")));
        }
    }
    Ok(Command::Pack(Some(pack)))
}

fn parse_unpack(args: &str) -> Result<UnpackArgs, EngineError> {
    let (words, switches) = split_switches(args);
    if !words.is_empty() {
        return Err(EngineError(
            "użycie: unpack [-h] [-s] [-p<hasło>] (archiwum wskazuje kursor lub otwarty panel)"
                .into(),
        ));
    }
    let mut unpack = UnpackArgs::default();
    for switch in switches {
        if let Some(password) = switch.strip_prefix("-p") {
            unpack.password = Some(password.to_string());
        } else if switch == "-s" {
            unpack.subfolder = true;
        } else if switch == "-h" {
            unpack.here = true;
        } else {
            return Err(EngineError(format!("nieznany przełącznik {switch}")));
        }
    }
    Ok(unpack)
}

fn parse_tab(args: &str) -> Result<TabAction, EngineError> {
    let usage = || EngineError("tab: użycie: tab <new|open|close [n]|next|prev|n>".into());
    let mut words = args.split_whitespace();
    let sub = words.next().ok_or_else(usage)?.to_ascii_lowercase();
    let number = |word: Option<&str>| -> Result<usize, EngineError> {
        word.and_then(|w| w.parse::<usize>().ok())
            .filter(|&n| n >= 1)
            .ok_or_else(usage)
    };
    let action = match sub.as_str() {
        "new" => TabAction::New,
        "open" => TabAction::Open,
        "close" => TabAction::Close(match words.next() {
            Some(word) => Some(number(Some(word))?),
            None => None,
        }),
        "next" => TabAction::Next,
        "prev" => TabAction::Prev,
        digits => TabAction::Goto(number(Some(digits))?),
    };
    if words.next().is_some() {
        return Err(usage());
    }
    Ok(action)
}

fn parse_panel_number(word: &str) -> Option<usize> {
    match word {
        "1" => Some(0),
        "2" => Some(1),
        _ => None,
    }
}

/// `dragto copy|move <1|2> [row]`
fn parse_dragto(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("dragto: użycie: dragto copy|move <1|2> [wiersz]".into());
    let words: Vec<&str> = args.split_whitespace().collect();
    let remove = match words.first().map(|w| w.to_ascii_lowercase()).as_deref() {
        Some("copy") => false,
        Some("move") => true,
        _ => return Err(usage()),
    };
    let panel = words
        .get(1)
        .and_then(|w| parse_panel_number(w))
        .ok_or_else(usage)?;
    let row = match words.get(2) {
        Some(word) => Some(
            word.parse::<usize>()
                .ok()
                .filter(|r| *r >= 1)
                .ok_or_else(usage)?,
        ),
        None => None,
    };
    if words.len() > 3 {
        return Err(usage());
    }
    Ok(Command::DragTo(remove, crate::DropTarget { panel, row }))
}

/// `drop <1|2> [row] "path"...`
fn parse_drop(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("drop: użycie: drop <1|2> [wiersz] \"ścieżka\"…".into());
    let mut words = crate::dragdrop::split_quoted(args).into_iter().peekable();
    let panel = words
        .next()
        .and_then(|w| parse_panel_number(&w))
        .ok_or_else(usage)?;
    let row = match words.peek().and_then(|w| w.parse::<usize>().ok()) {
        Some(row) if row >= 1 => {
            words.next();
            Some(row)
        }
        _ => None,
    };
    let paths: Vec<PathBuf> = words.map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err(usage());
    }
    Ok(Command::Drop(crate::DropTarget { panel, row }, paths))
}

fn parse_favorite(rest: &str) -> crate::FavoriteAction {
    use crate::FavoriteAction;
    let rest = rest.trim();
    match rest.split_once(char::is_whitespace) {
        Some(("del", path)) if !path.trim().is_empty() => {
            FavoriteAction::Remove(PathBuf::from(path.trim()))
        }
        _ if rest.is_empty() => FavoriteAction::Toggle(None),
        _ => FavoriteAction::Toggle(Some(PathBuf::from(rest))),
    }
}

fn parse_palette(rest: &str) -> Result<PaletteAction, EngineError> {
    let (word, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let tail = tail.trim();
    match word {
        "close" => Ok(PaletteAction::Close),
        "run" => Ok(PaletteAction::Run),
        "move" => tail
            .parse()
            .map(PaletteAction::Move)
            .map_err(|_| EngineError("palette: użycie: palette move <n>".into())),
        "query" => Ok(PaletteAction::Query(tail.to_string())),
        _ => Ok(PaletteAction::Query(rest.trim().to_string())),
    }
}

fn parse_viewer(args: &str) -> Result<ViewerAction, EngineError> {
    let usage = || {
        EngineError(
            "viewer: użycie: viewer <scroll n|page n|hscroll n|top|bottom|mode|goto x|find [tekst]|next|prev|close>"
                .into(),
        )
    };
    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((sub, rest)) => (sub, rest.trim()),
        None => (args, ""),
    };
    let number = |default: Option<i64>| match (rest.is_empty(), default) {
        (true, Some(n)) => Ok(n),
        _ => rest.parse::<i64>().map_err(|_| usage()),
    };
    Ok(match sub.to_ascii_lowercase().as_str() {
        "scroll" => ViewerAction::Scroll(number(None)?),
        "page" => ViewerAction::Page(number(Some(1))?),
        "hscroll" => ViewerAction::HScroll(number(None)?),
        "top" => ViewerAction::Top,
        "bottom" => ViewerAction::Bottom,
        "mode" => ViewerAction::Mode,
        "goto" if !rest.is_empty() => ViewerAction::Goto(rest.to_string()),
        "find" => ViewerAction::Find(optional(rest)),
        "next" => ViewerAction::Next,
        "prev" => ViewerAction::Prev,
        "close" => ViewerAction::Close,
        _ => return Err(usage()),
    })
}

fn parse_chmod(text: &str) -> Result<u32, EngineError> {
    let usage =
        || EngineError("chmod: podaj uprawnienia ósemkowo, np. chmod 755 albo chmod 0644".into());
    if !(3..=4).contains(&text.len()) {
        return Err(usage());
    }
    u32::from_str_radix(text, 8).map_err(|_| usage())
}

fn optional(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

fn parse_sync(args: &str) -> Result<SyncArgs, EngineError> {
    let usage = |what: &str| {
        EngineError(format!(
            "sync: {what} (użycie: sync [-twoway|-mirror|-update] [-time|-size|-content] [-tol=<s>] [-i<wzorce>] [-x<wzorce>] [-delete=trash|permanent|versions:<katalog>])"
        ))
    };
    let (words, switches) = split_switches(args);
    if !words.is_empty() {
        return Err(usage("foldery wskazują panele"));
    }
    let mut sync = SyncArgs::default();
    for switch in switches {
        match switch {
            "-twoway" => sync.variant = Some(Variant::TwoWay),
            "-mirror" => sync.variant = Some(Variant::Mirror),
            "-update" => sync.variant = Some(Variant::Update),
            "-time" => sync.compare = Some(CompareBy::default()),
            "-size" => sync.compare = Some(CompareBy::Size),
            "-content" => sync.compare = Some(CompareBy::Content),
            "-delete=trash" => sync.deletion = Some(Deletion::Trash),
            "-delete=permanent" => sync.deletion = Some(Deletion::Permanent),
            _ => {
                if let Some(secs) = switch.strip_prefix("-tol=") {
                    sync.tolerance = Some(
                        secs.parse()
                            .map_err(|_| usage("tolerancja to liczba sekund"))?,
                    );
                } else if let Some(folder) = switch.strip_prefix("-delete=versions:") {
                    if folder.is_empty() {
                        return Err(usage("podaj katalog wersji"));
                    }
                    sync.deletion = Some(Deletion::Versioning(PathBuf::from(folder)));
                } else if let Some(patterns) = switch.strip_prefix("-i") {
                    sync.include = Some(patterns.to_string());
                } else if let Some(patterns) = switch.strip_prefix("-x") {
                    sync.exclude = Some(patterns.to_string());
                } else {
                    return Err(usage(&format!("nieznany przełącznik {switch}")));
                }
            }
        }
    }
    if let Some(secs) = sync.tolerance {
        match sync.compare {
            None | Some(CompareBy::TimeAndSize { .. }) => {
                sync.compare = Some(CompareBy::TimeAndSize {
                    tolerance: Duration::from_secs(secs),
                });
            }
            Some(_) => return Err(usage("-tol działa tylko z porównaniem po dacie")),
        }
    }
    Ok(sync)
}

fn parse_page(args: &str) -> Result<PageMove, EngineError> {
    match args.trim().to_ascii_lowercase().as_str() {
        "next" | "+" => Ok(PageMove::Next),
        "prev" | "-" => Ok(PageMove::Prev),
        "first" => Ok(PageMove::First),
        "last" => Ok(PageMove::Last),
        number => number
            .parse()
            .ok()
            .filter(|n| *n >= 1)
            .map(PageMove::To)
            .ok_or_else(|| {
                EngineError("page: użycie: page next|prev|first|last|<numer strony>".into())
            }),
    }
}

fn parse_pagezoom(args: &str) -> Result<ZoomChange, EngineError> {
    match args.trim().to_ascii_lowercase().as_str() {
        "in" | "+" => Ok(ZoomChange::In),
        "out" | "-" => Ok(ZoomChange::Out),
        "fit" => Ok(ZoomChange::Fit),
        percent => percent
            .trim_end_matches('%')
            .parse()
            .ok()
            .map(ZoomChange::Percent)
            .ok_or_else(|| EngineError("pagezoom: użycie: pagezoom in|out|fit|<procent>".into())),
    }
}

/// A site name at the start of `text`: one word, or several in double quotes
/// (`"Home Assistant" rest`). Returns the name and what follows it.
fn take_name(command: &str, text: &str) -> Result<(String, String), EngineError> {
    let text = text.trim();
    let (name, tail) = if let Some(quoted) = text.strip_prefix('"') {
        match quoted.split_once('"') {
            Some((name, tail)) => (name, tail),
            None => (quoted, ""),
        }
    } else {
        match text.split_once(char::is_whitespace) {
            Some((name, tail)) => (name, tail),
            None => (text, ""),
        }
    };
    if name.trim().is_empty() {
        return Err(EngineError(format!(
            "{command}: podaj nazwę witryny (w cudzysłowie, jeśli ma spacje)"
        )));
    }
    Ok((name.trim().to_string(), tail.trim().to_string()))
}

/// `siteset <name> <field> <value>` with fields host, port, user, key, start, secret.
fn parse_siteset(args: &str) -> Result<Command, EngineError> {
    let usage = || {
        EngineError(
            "siteset: użycie: siteset <nazwa> <protocol|host|port|user|key|start|secret> <wartość> (secret: ask, local:<hasło> albo bitwarden:<element>)"
                .into(),
        )
    };
    let (name, tail) = take_name("siteset", args)?;
    let (field, value) = match tail.split_once(char::is_whitespace) {
        Some((field, value)) => (field, value.trim()),
        None => (tail.as_str(), ""),
    };
    if field.is_empty() {
        return Err(usage());
    }
    Ok(Command::SiteSet(
        name,
        field.to_ascii_lowercase(),
        value.to_string(),
    ))
}

/// `findset <field> <value>`; quotes around the value are removed like for `mrset`.
fn parse_findset(args: &str) -> Result<Command, EngineError> {
    let usage = || {
        EngineError(
            "findset: użycie: findset <roots|name|namergx|namecase|exclude|text|regex|case|words|enc|binary|archives|password|hidden|min|max|after|before|limit|reset> <wartość>"
                .into(),
        )
    };
    match parse_mrset(args) {
        Ok(Command::MultiRenameSet(field, value)) => Ok(Command::FindSet(field, value)),
        _ => Err(usage()),
    }
}

fn parse_ip(text: &str) -> Result<std::net::Ipv4Addr, EngineError> {
    text.trim()
        .parse()
        .map_err(|_| EngineError(format!("niepoprawny adres IP: {}", text.trim())))
}

fn parse_limit(args: &str) -> Result<SpeedLimit, EngineError> {
    let args = args.trim();
    match args.to_ascii_lowercase().as_str() {
        "" => Ok(SpeedLimit::Show),
        "off" | "none" | "brak" | "0" => Ok(SpeedLimit::Off),
        _ => parse_size(args.trim_end_matches("/s"))
            .map(SpeedLimit::BytesPerSecond)
            .map_err(|_| {
                EngineError(
                    "limit: podaj prędkość na sekundę, np. limit 2m, limit 500k albo limit off"
                        .into(),
                )
            }),
    }
}

fn parse_queue_move(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("queuemove: użycie: queuemove <numer> up|down|top".into());
    let (row, direction) = args.split_once(' ').ok_or_else(usage)?;
    let row = parse_row("queuemove", row)?;
    let direction = match direction.trim() {
        "up" => QueueMove::Up,
        "down" => QueueMove::Down,
        "top" => QueueMove::Top,
        _ => return Err(usage()),
    };
    Ok(Command::QueueMove(row, direction))
}

fn parse_row(name: &str, args: &str) -> Result<usize, EngineError> {
    args.trim()
        .parse()
        .ok()
        .filter(|n| *n >= 1)
        .ok_or_else(|| EngineError(format!("{name}: użycie: {name} <numer wyniku>")))
}

/// `mrset <field> <value>`; a value in double quotes keeps its spaces (`mrset replace " "`).
fn parse_mrset(args: &str) -> Result<Command, EngineError> {
    let usage = || {
        EngineError(
            "mrset: użycie: mrset <name|ext|find|replace|regex|once|inclext|ignorecase|case|start|step|digits|reset> <wartość>"
                .into(),
        )
    };
    let (field, value) = match args.split_once(char::is_whitespace) {
        Some((field, value)) => (field, value.trim()),
        None => (args, ""),
    };
    if field.is_empty() {
        return Err(usage());
    }
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    Ok(Command::MultiRenameSet(
        field.to_ascii_lowercase(),
        value.to_string(),
    ))
}

/// `click <row> [ctrl|shift|ctrlshift]`
fn parse_click(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("click: użycie: click <wiersz> [ctrl|shift|ctrlshift]".into());
    let mut words = args.split_whitespace();
    let row: usize = words
        .next()
        .and_then(|n| n.parse().ok())
        .filter(|n| *n >= 1)
        .ok_or_else(usage)?;
    let mode = match words.next().map(str::to_ascii_lowercase).as_deref() {
        None => ClickMode::Plain,
        Some("ctrl") => ClickMode::Ctrl,
        Some("shift") => ClickMode::Shift,
        Some("ctrlshift") => ClickMode::CtrlShift,
        Some(_) => return Err(usage()),
    };
    if words.next().is_some() {
        return Err(usage());
    }
    Ok(Command::Click(row, mode))
}

fn parse_mrname(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("mrname: użycie: mrname <numer> [nowa nazwa]".into());
    let (n, name) = match args.split_once(char::is_whitespace) {
        Some((n, name)) => (n, Some(name.trim())),
        None => (args, None),
    };
    let n: usize = n.parse().ok().filter(|n| *n >= 1).ok_or_else(usage)?;
    Ok(Command::MultiRenameName(
        n,
        name.filter(|name| !name.is_empty()).map(str::to_string),
    ))
}

fn parse_syncset(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("syncset: użycie: syncset <numer> right|left|skip".into());
    let mut words = args.split_whitespace();
    let (Some(n), Some(way), None) = (words.next(), words.next(), words.next()) else {
        return Err(usage());
    };
    let n: usize = n.parse().ok().filter(|n| *n >= 1).ok_or_else(usage)?;
    let direction = match way.to_ascii_lowercase().as_str() {
        "right" | "r" | "->" => Direction::ToRight,
        "left" | "l" | "<-" => Direction::ToLeft,
        "skip" | "s" | "none" => Direction::Skip,
        _ => return Err(usage()),
    };
    Ok(Command::SyncSet(n, direction))
}

/// `overwrite`, `newer`, `skip`, `keepboth`, each optionally followed by `-all`.
fn parse_resolution(word: &str) -> Option<(Resolution, bool)> {
    let (base, all) = match word.strip_suffix("-all") {
        Some(base) => (base, true),
        None => (word, false),
    };
    let resolution = match base {
        "overwrite" => Resolution::Overwrite,
        "newer" => Resolution::OverwriteOlder,
        "skip" => Resolution::Skip,
        "keepboth" => Resolution::KeepBoth,
        _ => return None,
    };
    Some((resolution, all))
}

fn parse_sort(args: &str) -> Result<Command, EngineError> {
    let usage = || EngineError("sort: użycie: sort <name|ext|size|date> [asc|desc]".into());
    let mut words = args.split_whitespace();
    let key = words.next().and_then(SortKey::parse).ok_or_else(usage)?;
    let descending = match words.next().map(str::to_ascii_lowercase).as_deref() {
        None => None,
        Some("asc") => Some(false),
        Some("desc") => Some(true),
        Some(_) => return Err(usage()),
    };
    if words.next().is_some() {
        return Err(usage());
    }
    Ok(Command::Sort(key, descending))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_commands_and_aliases() {
        assert_eq!(Command::parse("copy"), Ok(Command::Copy));
        assert_eq!(Command::parse("  CP "), Ok(Command::Copy));
        assert_eq!(Command::parse("rm"), Ok(Command::Delete));
        assert_eq!(Command::parse("cursor -3"), Ok(Command::Cursor(-3)));
        assert_eq!(Command::parse("chmod 755"), Ok(Command::Chmod(0o755)));
        assert_eq!(Command::parse("chmod 0644"), Ok(Command::Chmod(0o644)));
        for bad in ["chmod", "chmod 75", "chmod 789", "chmod rwx", "chmod 07555"] {
            assert!(Command::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_commands_with_arguments() {
        assert_eq!(
            Command::parse("mkdir New folder"),
            Ok(Command::Mkdir(Some("New folder".into())))
        );
        assert_eq!(
            Command::parse("cd /tmp"),
            Ok(Command::Cd(PathBuf::from("/tmp")))
        );
        assert_eq!(Command::parse("volume 2"), Ok(Command::Volume("2".into())));
        assert_eq!(
            Command::parse("filter my file"),
            Ok(Command::Filter("my file".into()))
        );
    }

    #[test]
    fn parses_selection_and_sort_commands() {
        assert_eq!(
            Command::parse("selmask *.txt;*.md"),
            Ok(Command::SelectMask("*.txt;*.md".into()))
        );
        assert_eq!(Command::parse("selectall"), Ok(Command::SelectAll));
        assert_eq!(Command::parse("invert"), Ok(Command::InvertSelection));
        assert_eq!(
            Command::parse("sort size"),
            Ok(Command::Sort(SortKey::Size, None))
        );
        assert_eq!(
            Command::parse("sort DATE desc"),
            Ok(Command::Sort(SortKey::Date, Some(true)))
        );
        assert!(Command::parse("selmask").is_err());
        assert!(Command::parse("sort").is_err());
        assert!(Command::parse("sort color").is_err());
        assert!(Command::parse("sort name up").is_err());
        assert!(Command::parse("sort name asc extra").is_err());
    }

    #[test]
    fn mkdir_and_rename_take_an_optional_name() {
        assert_eq!(Command::parse("mkdir"), Ok(Command::Mkdir(None)));
        assert_eq!(
            Command::parse("mkdir New folder"),
            Ok(Command::Mkdir(Some("New folder".into())))
        );
        assert_eq!(Command::parse("rename"), Ok(Command::Rename(None)));
        assert_eq!(
            Command::parse("rename b.txt"),
            Ok(Command::Rename(Some("b.txt".into())))
        );
    }

    #[test]
    fn parses_conflict_answers() {
        assert_eq!(
            Command::parse("overwrite"),
            Ok(Command::Resolve(Resolution::Overwrite, false))
        );
        assert_eq!(
            Command::parse("newer-all"),
            Ok(Command::Resolve(Resolution::OverwriteOlder, true))
        );
        assert_eq!(
            Command::parse("SKIP"),
            Ok(Command::Resolve(Resolution::Skip, false))
        );
        assert_eq!(
            Command::parse("keepboth-all"),
            Ok(Command::Resolve(Resolution::KeepBoth, true))
        );
        assert_eq!(Command::parse("stop"), Ok(Command::StopJob));
        assert!(Command::parse("skip-everything").is_err());
    }

    #[test]
    fn parses_view_edit_and_viewer_actions() {
        assert_eq!(Command::parse("view"), Ok(Command::View));
        assert_eq!(Command::parse("edit"), Ok(Command::Edit));
        let viewer = |text: &str| {
            Command::parse(text).map(|c| match c {
                Command::Viewer(action) => action,
                other => panic!("{other:?}"),
            })
        };
        assert_eq!(viewer("viewer scroll -3"), Ok(ViewerAction::Scroll(-3)));
        assert_eq!(viewer("viewer page"), Ok(ViewerAction::Page(1)));
        assert_eq!(viewer("viewer page -1"), Ok(ViewerAction::Page(-1)));
        assert_eq!(viewer("viewer hscroll 8"), Ok(ViewerAction::HScroll(8)));
        assert_eq!(viewer("viewer top"), Ok(ViewerAction::Top));
        assert_eq!(viewer("viewer bottom"), Ok(ViewerAction::Bottom));
        assert_eq!(viewer("viewer mode"), Ok(ViewerAction::Mode));
        assert_eq!(
            viewer("viewer goto 50%"),
            Ok(ViewerAction::Goto("50%".into()))
        );
        assert_eq!(viewer("viewer find"), Ok(ViewerAction::Find(None)));
        assert_eq!(
            viewer("viewer find two words"),
            Ok(ViewerAction::Find(Some("two words".into())))
        );
        assert_eq!(viewer("viewer next"), Ok(ViewerAction::Next));
        assert_eq!(viewer("viewer prev"), Ok(ViewerAction::Prev));
        assert_eq!(viewer("viewer close"), Ok(ViewerAction::Close));
        for bad in [
            "viewer",
            "viewer scroll",
            "viewer scroll x",
            "viewer goto",
            "viewer fly",
        ] {
            assert!(Command::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_tab_commands() {
        let tab = |text: &str| Command::parse(text);
        assert_eq!(tab("tab new"), Ok(Command::Tab(TabAction::New)));
        assert_eq!(tab("tab open"), Ok(Command::Tab(TabAction::Open)));
        assert_eq!(tab("tab close"), Ok(Command::Tab(TabAction::Close(None))));
        assert_eq!(
            tab("tab close 2"),
            Ok(Command::Tab(TabAction::Close(Some(2))))
        );
        assert_eq!(tab("TAB next"), Ok(Command::Tab(TabAction::Next)));
        assert_eq!(tab("tab prev"), Ok(Command::Tab(TabAction::Prev)));
        assert_eq!(tab("tab 3"), Ok(Command::Tab(TabAction::Goto(3))));
        for bad in [
            "tab",
            "tab 0",
            "tab close x",
            "tab new now",
            "tab sideways",
            "tab -1",
        ] {
            assert!(tab(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sizes_accept_units_and_decimals() {
        assert_eq!(parse_size("100"), Ok(100));
        assert_eq!(parse_size("64k"), Ok(64 * 1024));
        assert_eq!(parse_size("64KB"), Ok(64 * 1024));
        assert_eq!(parse_size("700m"), Ok(700 * 1024 * 1024));
        assert_eq!(parse_size("1.5g"), Ok(1_610_612_736));
        assert_eq!(parse_size(" 2G "), Ok(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("10b"), Ok(10));
        for bad in ["", "k", "abc", "0", "-5", "1x", "0.0001", "1e30000"] {
            assert!(parse_size(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parses_pack_with_seven_zip_style_switches() {
        assert_eq!(Command::parse("pack"), Ok(Command::Pack(None)));
        assert_eq!(
            Command::parse("pack photos.zip"),
            Ok(Command::Pack(Some(PackArgs {
                name: "photos.zip".into(),
                ..PackArgs::default()
            })))
        );
        assert_eq!(
            Command::parse("pack my files.tgz -mx=9 -psekret -v100m"),
            Ok(Command::Pack(Some(PackArgs {
                name: "my files.tgz".into(),
                format: None,
                level: Some(Level::Maximum),
                password: Some("sekret".into()),
                split: Some(100 * 1024 * 1024),
                encrypt_names: false,
            })))
        );
        assert_eq!(
            Command::parse("pack -ttar -mx0"),
            Ok(Command::Pack(Some(PackArgs {
                format: Some(Format::Tar),
                level: Some(Level::Store),
                ..PackArgs::default()
            })))
        );
        for bad in [
            "pack x -tfoo",
            "pack x -mx=12",
            "pack x -v",
            "pack x -z",
            "pack x -vabc",
        ] {
            assert!(Command::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_sync_switches_and_rows() {
        use bluepane_sync::{CompareBy, Deletion, Direction, Variant};
        assert_eq!(
            Command::parse("sync"),
            Ok(Command::Sync(SyncArgs::default()))
        );
        let Ok(Command::Sync(args)) =
            Command::parse("sync -mirror -content -x*.tmp;cache/ -iDocs -delete=permanent")
        else {
            panic!("did not parse");
        };
        assert_eq!(args.variant, Some(Variant::Mirror));
        assert_eq!(args.compare, Some(CompareBy::Content));
        assert_eq!(args.exclude.as_deref(), Some("*.tmp;cache/"));
        assert_eq!(args.include.as_deref(), Some("Docs"));
        assert_eq!(args.deletion, Some(Deletion::Permanent));
        let Ok(Command::Sync(args)) = Command::parse("sync -tol=5 -delete=versions:/tmp/v") else {
            panic!("did not parse");
        };
        assert_eq!(
            args.compare,
            Some(CompareBy::TimeAndSize {
                tolerance: Duration::from_secs(5)
            })
        );
        assert_eq!(args.deletion, Some(Deletion::Versioning("/tmp/v".into())));
        assert!(Command::parse("sync -size -tol=5").is_err());
        assert!(Command::parse("sync -tol=x").is_err());
        assert!(Command::parse("sync -nope").is_err());
        assert!(Command::parse("sync /some/folder").is_err());
        assert_eq!(
            Command::parse("syncset 3 left"),
            Ok(Command::SyncSet(3, Direction::ToLeft))
        );
        assert_eq!(
            Command::parse("syncset 1 skip"),
            Ok(Command::SyncSet(1, Direction::Skip))
        );
        assert!(Command::parse("syncset 0 left").is_err());
        assert!(Command::parse("syncset 1 up").is_err());
        assert!(Command::parse("syncset 1").is_err());
        assert_eq!(Command::parse("syncrun"), Ok(Command::SyncRun));
        assert_eq!(Command::parse("syncclose"), Ok(Command::SyncClose));
    }

    #[test]
    fn parses_unpack_and_testarchive() {
        assert_eq!(
            Command::parse("unpack"),
            Ok(Command::Unpack(UnpackArgs::default()))
        );
        assert_eq!(
            Command::parse("unpack -s -ppw"),
            Ok(Command::Unpack(UnpackArgs {
                password: Some("pw".into()),
                subfolder: true,
                here: false,
            }))
        );
        assert_eq!(
            Command::parse("unpack -h"),
            Ok(Command::Unpack(UnpackArgs {
                here: true,
                ..UnpackArgs::default()
            }))
        );
        assert_eq!(
            Command::parse("testarchive"),
            Ok(Command::TestArchive(None))
        );
        assert_eq!(
            Command::parse("testarchive -pxyz"),
            Ok(Command::TestArchive(Some("xyz".into())))
        );
        assert!(Command::parse("unpack somewhere").is_err());
        assert!(Command::parse("unpack -q").is_err());
    }

    #[test]
    fn parses_confirm_and_cancel() {
        assert_eq!(Command::parse("confirm"), Ok(Command::Confirm));
        assert_eq!(Command::parse("yes"), Ok(Command::Confirm));
        assert_eq!(Command::parse("cancel"), Ok(Command::Cancel));
    }

    #[test]
    fn parses_history_and_filter_clearing() {
        assert_eq!(Command::parse("back"), Ok(Command::Back));
        assert_eq!(Command::parse("forward"), Ok(Command::Forward));
        assert_eq!(Command::parse("filter"), Ok(Command::Filter(String::new())));
        assert!(Command::parse("volume").is_err());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Command::parse("").is_err());
        assert!(Command::parse("cursor x").is_err());
        assert!(Command::parse("frobnicate").is_err());
    }

    #[test]
    fn multirename_commands_parse() {
        assert_eq!(Command::parse("multirename"), Ok(Command::MultiRename));
        assert_eq!(
            Command::parse("mrset name [N]_[C]"),
            Ok(Command::MultiRenameSet("name".into(), "[N]_[C]".into()))
        );
        assert_eq!(
            Command::parse("mrset replace \" \""),
            Ok(Command::MultiRenameSet("replace".into(), " ".into()))
        );
        assert_eq!(
            Command::parse("mrset reset"),
            Ok(Command::MultiRenameSet("reset".into(), String::new()))
        );
        assert!(Command::parse("mrset").is_err());
        assert_eq!(
            Command::parse("mrname 2 new.txt"),
            Ok(Command::MultiRenameName(2, Some("new.txt".into())))
        );
        assert_eq!(
            Command::parse("mrname 2"),
            Ok(Command::MultiRenameName(2, None))
        );
        assert!(Command::parse("mrname x").is_err());
        assert_eq!(Command::parse("mrrun"), Ok(Command::MultiRenameRun));
        assert_eq!(Command::parse("mrundo"), Ok(Command::MultiRenameUndo));
        assert_eq!(Command::parse("mrclose"), Ok(Command::MultiRenameClose));
    }

    #[test]
    fn click_parses_row_and_modifier() {
        assert_eq!(
            Command::parse("click 3"),
            Ok(Command::Click(3, ClickMode::Plain))
        );
        assert_eq!(
            Command::parse("click 3 ctrl"),
            Ok(Command::Click(3, ClickMode::Ctrl))
        );
        assert_eq!(
            Command::parse("click 2 shift"),
            Ok(Command::Click(2, ClickMode::Shift))
        );
        assert_eq!(
            Command::parse("click 2 ctrlshift"),
            Ok(Command::Click(2, ClickMode::CtrlShift))
        );
        for bad in [
            "click",
            "click 0",
            "click x",
            "click 1 alt",
            "click 1 ctrl x",
        ] {
            assert!(Command::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_listed_name_parses_or_needs_an_argument() {
        for name in COMMAND_NAMES {
            match Command::parse(name) {
                Ok(_) => {}
                Err(e) => assert!(!e.0.contains("nieznana"), "{name} is not recognised"),
            }
        }
    }
}
