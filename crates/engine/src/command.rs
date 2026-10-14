use std::path::PathBuf;

use bluepane_archive::{Format, Level};
use bluepane_ops::Resolution;
use bluepane_sync::{CompareBy, Deletion, Direction, Variant};
use std::time::Duration;

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
    "scan",
    "scanset",
    "scanrun",
    "scanstop",
    "scanclose",
    "queue",
    "queueclose",
    "queuepause",
    "queueresume",
    "queuedel",
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
    /// Open the queue window (Ctrl+J): what runs, what waits, what finished.
    Queue,
    QueueClose,
    /// Let the running job finish but start no further queued ones.
    QueuePause,
    QueueResume,
    /// Drop queued job n (1-based, in the order shown) before it starts.
    QueueDelete(usize),
    /// Queue a finished job (n = 1 is the newest) once more.
    QueueRetry(usize),
    /// Forget the list of finished jobs.
    QueueClear,
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
            "rename" => Ok(Command::Rename(optional(rest))),
            "stop" => Ok(Command::StopJob),
            "sync" => parse_sync(rest).map(Command::Sync),
            "syncrun" => Ok(Command::SyncRun),
            "syncset" => parse_syncset(rest),
            "syncclose" => Ok(Command::SyncClose),
            "sites" => Ok(Command::Sites),
            "siteclose" => Ok(Command::SiteClose),
            "editsend" => Ok(Command::EditSend),
            "scan" => Ok(Command::Scan),
            "scanset" => parse_mrset(rest).map(|c| match c {
                Command::MultiRenameSet(f, v) => Command::ScanSet(f, v),
                other => other,
            }),
            "scanrun" => Ok(Command::ScanRun),
            "scanstop" => Ok(Command::ScanStop),
            "scanclose" => Ok(Command::ScanClose),
            "queue" => Ok(Command::Queue),
            "queueclose" => Ok(Command::QueueClose),
            "queuepause" => Ok(Command::QueuePause),
            "queueresume" => Ok(Command::QueueResume),
            "queuedel" => parse_row("queuedel", rest).map(Command::QueueDelete),
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
            "użycie: unpack [-s] [-p<hasło>] (archiwum wskazuje kursor lub otwarty panel)".into(),
        ));
    }
    let mut unpack = UnpackArgs::default();
    for switch in switches {
        if let Some(password) = switch.strip_prefix("-p") {
            unpack.password = Some(password.to_string());
        } else if switch == "-s" {
            unpack.subfolder = true;
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
                subfolder: true
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
