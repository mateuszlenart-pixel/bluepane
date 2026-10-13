//! File operations.
//!
//! Operations are synchronous functions that report progress and honour cancellation
//! through a shared [`Progress`] handle; the layer above runs them on a background thread.
//! Everything that needs a decision from the user (an existing target) or that differs
//! between operating systems (symlinks, read-only files) goes through the [`Host`] trait,
//! so this crate stays free of UI and platform code.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Size of one read/write step when copying. Progress and cancellation are checked after
/// every chunk, so even a huge file can be stopped within a fraction of a second.
const COPY_CHUNK: usize = 256 * 1024;

#[derive(Debug)]
pub enum OpError {
    Io(io::Error),
    Cancelled,
    /// The target already exists and nobody decided what to do about it.
    Exists(PathBuf),
    InvalidName(String),
    Trash(String),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Io(e) => write!(f, "{e}"),
            OpError::Cancelled => write!(f, "anulowano"),
            OpError::Exists(p) => write!(f, "już istnieje: {}", p.display()),
            OpError::InvalidName(n) => write!(f, "niepoprawna nazwa: {n:?}"),
            OpError::Trash(e) => write!(f, "kosz: {e}"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<io::Error> for OpError {
    fn from(e: io::Error) -> Self {
        OpError::Io(e)
    }
}

fn invalid_input(message: impl Into<String>) -> OpError {
    OpError::Io(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

/// Shared progress and cancellation state. Cloning shares the same counters, so the
/// worker thread and the UI can hold one each.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    cancel: Arc<AtomicBool>,
    bytes_done: Arc<AtomicU64>,
    bytes_total: Arc<AtomicU64>,
    items_done: Arc<AtomicU64>,
    items_total: Arc<AtomicU64>,
    current: Arc<Mutex<String>>,
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn bytes_done(&self) -> u64 {
        self.bytes_done.load(Ordering::Relaxed)
    }

    /// Total bytes to process, or 0 while still unknown.
    pub fn bytes_total(&self) -> u64 {
        self.bytes_total.load(Ordering::Relaxed)
    }

    /// Files (or, for deleting, entries) finished so far.
    pub fn items_done(&self) -> u64 {
        self.items_done.load(Ordering::Relaxed)
    }

    /// Total files to process, or 0 while still unknown.
    pub fn items_total(&self) -> u64 {
        self.items_total.load(Ordering::Relaxed)
    }

    /// Name of the file being processed right now.
    pub fn current(&self) -> String {
        self.current.lock().map(|c| c.clone()).unwrap_or_default()
    }

    pub fn set_total(&self, bytes: u64, items: u64) {
        self.bytes_total.store(bytes, Ordering::Relaxed);
        self.items_total.store(items, Ordering::Relaxed);
    }

    pub fn set_current(&self, name: &str) {
        if let Ok(mut current) = self.current.lock() {
            current.clear();
            current.push_str(name);
        }
    }

    /// Counts bytes as processed (for operations implemented outside this crate).
    pub fn add(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn finish_item(&self) {
        self.items_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Fails with [`OpError::Cancelled`] once the operation was cancelled.
    pub fn check(&self) -> Result<(), OpError> {
        if self.is_cancelled() {
            Err(OpError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// What to do when the target of a copy or move already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Replace the existing file with the new one.
    Overwrite,
    /// Replace the existing file only when the new one is strictly newer; otherwise leave
    /// it alone. Equal or unknown modification times count as "not newer" (the safe side).
    OverwriteOlder,
    /// Leave the existing file alone and do not copy this one.
    Skip,
    /// Keep both: the new file gets a free name such as `a (kopia).txt`.
    KeepBoth,
}

/// An existing file that stands in the way of a copy or move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub src_size: u64,
    pub dst_size: u64,
    pub src_modified: Option<SystemTime>,
    pub dst_modified: Option<SystemTime>,
}

/// The caller's side of an operation: decisions and platform specifics.
pub trait Host {
    /// Decides what happens to a conflicting target. Returning an error (typically
    /// [`OpError::Cancelled`]) aborts the whole operation.
    fn conflict(&mut self, conflict: &Conflict) -> Result<Resolution, OpError>;

    /// Called right before an existing file is replaced (Windows: clears the read-only flag).
    fn make_writable(&self, _path: &Path) {}

    /// Recreates the symlink `src` at `dst`. When this fails (or is not supported) the
    /// link is followed and the target's content is copied instead.
    fn copy_symlink(&self, _src: &Path, _dst: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    /// Sets Unix permission bits on an extracted file or directory (a no-op where the
    /// platform has none).
    fn set_mode(&self, _path: &Path, _mode: u32) {}

    /// Unix permission bits of a file, to store them in an archive (`None` where the
    /// platform has none).
    fn file_mode(&self, _path: &Path) -> Option<u32> {
        None
    }

    /// Called after every copied chunk.
    fn tick(&mut self, _progress: &Progress) {}
}

/// A [`Host`] that never overwrites: an existing target is an error.
#[derive(Debug, Default)]
pub struct FailOnConflict;

impl Host for FailOnConflict {
    fn conflict(&mut self, conflict: &Conflict) -> Result<Resolution, OpError> {
        Err(OpError::Exists(conflict.dst.clone()))
    }
}

/// What a copy or move did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// Files (and symlinks) written.
    pub files: u64,
    /// Files left alone because the user chose "skip". The source of a move that skipped
    /// anything is not removed.
    pub skipped: u64,
}

struct Ctx<'a> {
    progress: &'a Progress,
    host: &'a mut dyn Host,
    outcome: Outcome,
}

fn target_name(src: &Path) -> Result<&std::ffi::OsStr, OpError> {
    src.file_name()
        .ok_or_else(|| OpError::InvalidName(src.display().to_string()))
}

fn target_for(src: &Path, dst_dir: &Path) -> Result<PathBuf, OpError> {
    let target = dst_dir.join(target_name(src)?);
    if target.exists() {
        return Err(OpError::Exists(target));
    }
    Ok(target)
}

/// Copies a file or a directory tree into `dst_dir`, keeping its name. An existing target
/// is an error. Returns the path of the copy.
pub fn copy(src: &Path, dst_dir: &Path, progress: &Progress) -> Result<PathBuf, OpError> {
    let target = target_for(src, dst_dir)?;
    copy_to(src, &target, progress)
}

/// Copies a file or a directory tree to the exact path `target` (which must not exist).
/// Returns `target`.
pub fn copy_to(src: &Path, target: &Path, progress: &Progress) -> Result<PathBuf, OpError> {
    if target.exists() {
        return Err(OpError::Exists(target.to_path_buf()));
    }
    copy_path(src, target, progress, &mut FailOnConflict)?;
    Ok(target.to_path_buf())
}

/// Copies `src` into `dst_dir`, asking `host` what to do about files that already exist.
/// Directories that exist on both sides are merged.
pub fn copy_with(
    src: &Path,
    dst_dir: &Path,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<Outcome, OpError> {
    let target = dst_dir.join(target_name(src)?);
    copy_path(src, &target, progress, host)
}

fn copy_path(
    src: &Path,
    target: &Path,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<Outcome, OpError> {
    if src.is_dir() && target.starts_with(src) {
        return Err(invalid_input("nie można kopiować katalogu do jego wnętrza"));
    }
    let mut ctx = Ctx {
        progress,
        host,
        outcome: Outcome::default(),
    };
    copy_item(&mut ctx, src, target)?;
    Ok(ctx.outcome)
}

fn copy_item(ctx: &mut Ctx<'_>, src: &Path, dst: &Path) -> Result<(), OpError> {
    ctx.progress.check()?;
    let meta = fs::symlink_metadata(src)?;
    if meta.is_symlink() {
        copy_link(ctx, src, dst, &meta)
    } else if meta.is_dir() {
        copy_dir(ctx, src, dst, &meta)
    } else {
        copy_file(ctx, src, dst, &meta)
    }
}

/// What to do with one file after looking at its target.
enum Target {
    Skip,
    /// Write a new file at this path.
    Write(PathBuf),
    /// Replace the existing file at this path.
    Replace(PathBuf),
}

fn resolve_target(
    ctx: &mut Ctx<'_>,
    src: &Path,
    src_size: u64,
    src_modified: Option<SystemTime>,
    dst: &Path,
) -> Result<Target, OpError> {
    let dst_meta = match fs::symlink_metadata(dst) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Target::Write(dst.to_path_buf()))
        }
        Err(e) => return Err(e.into()),
    };
    if dst_meta.is_dir() {
        return Err(invalid_input(format!(
            "cel jest katalogiem, a źródło plikiem: {}",
            dst.display()
        )));
    }
    let conflict = Conflict {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        src_size,
        dst_size: dst_meta.len(),
        src_modified,
        dst_modified: dst_meta.modified().ok(),
    };
    let resolution = match ctx.host.conflict(&conflict)? {
        Resolution::OverwriteOlder => match (conflict.src_modified, conflict.dst_modified) {
            (Some(new), Some(old)) if new > old => Resolution::Overwrite,
            _ => Resolution::Skip,
        },
        other => other,
    };
    match resolution {
        Resolution::Skip => {
            ctx.outcome.skipped += 1;
            ctx.progress.add(src_size);
            ctx.progress.finish_item();
            Ok(Target::Skip)
        }
        Resolution::Overwrite | Resolution::OverwriteOlder => {
            Ok(Target::Replace(dst.to_path_buf()))
        }
        Resolution::KeepBoth => Ok(Target::Write(duplicate_path(dst)?)),
    }
}

fn copy_link(
    ctx: &mut Ctx<'_>,
    src: &Path,
    dst: &Path,
    meta: &fs::Metadata,
) -> Result<(), OpError> {
    let target = match resolve_target(ctx, src, meta.len(), meta.modified().ok(), dst)? {
        Target::Skip => return Ok(()),
        Target::Write(path) => path,
        Target::Replace(path) => {
            ctx.host.make_writable(&path);
            fs::remove_file(&path)?;
            path
        }
    };
    if ctx.host.copy_symlink(src, &target).is_ok() {
        ctx.outcome.files += 1;
        ctx.progress.finish_item();
        return Ok(());
    }
    // Could not recreate the link: follow it and copy what it points to.
    let followed = fs::metadata(src)?;
    if followed.is_dir() {
        copy_dir(ctx, src, &target, &followed)
    } else {
        let mut input = File::open(src)?;
        write_stream(
            ctx,
            src,
            &mut input,
            followed.len(),
            &target,
            Finish::Copy(&followed),
        )
    }
}

fn copy_dir(ctx: &mut Ctx<'_>, src: &Path, dst: &Path, meta: &fs::Metadata) -> Result<(), OpError> {
    match fs::symlink_metadata(dst) {
        Ok(existing) if existing.is_dir() => {}
        Ok(_) => {
            return Err(invalid_input(format!(
                "cel jest plikiem, a źródło katalogiem: {}",
                dst.display()
            )))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(dst)?,
        Err(e) => return Err(e.into()),
    }
    for item in fs::read_dir(src)? {
        let item = item?;
        copy_item(ctx, &item.path(), &dst.join(item.file_name()))?;
    }
    // Set last so a read-only source directory does not block writing its content.
    let _ = fs::set_permissions(dst, meta.permissions());
    Ok(())
}

fn copy_file(
    ctx: &mut Ctx<'_>,
    src: &Path,
    dst: &Path,
    meta: &fs::Metadata,
) -> Result<(), OpError> {
    let mut input = File::open(src)?;
    store_file(
        ctx,
        src,
        &mut input,
        meta.len(),
        meta.modified().ok(),
        dst,
        Finish::Copy(meta),
    )
}

/// How a written file gets its permissions and time once it is complete.
#[derive(Clone, Copy)]
enum Finish<'a> {
    /// A copy of a file: take the permissions of the original.
    Copy(&'a fs::Metadata),
    /// A stream (an archive entry): set the modification time and Unix mode if known.
    Stream {
        mode: Option<u32>,
        modified: Option<SystemTime>,
    },
}

/// Writes `reader` to `dst` after settling what to do if `dst` exists. A replaced file is
/// written next to the old one first, so a failure or a cancel leaves the old one intact.
fn store_file(
    ctx: &mut Ctx<'_>,
    label: &Path,
    reader: &mut dyn Read,
    size: u64,
    modified: Option<SystemTime>,
    dst: &Path,
    finish: Finish<'_>,
) -> Result<(), OpError> {
    match resolve_target(ctx, label, size, modified, dst)? {
        Target::Skip => Ok(()),
        Target::Write(path) => write_stream(ctx, label, reader, size, &path, finish),
        Target::Replace(path) => {
            let temp = temp_sibling(&path);
            write_stream(ctx, label, reader, size, &temp, finish)?;
            ctx.host.make_writable(&path);
            if let Err(e) = fs::rename(&temp, &path) {
                let _ = fs::remove_file(&temp);
                return Err(e.into());
            }
            Ok(())
        }
    }
}

fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.bluepane-tmp"))
}

/// Copies one stream into a new file in chunks and counts it as an item. A partial file is
/// removed when the copy fails or is cancelled.
fn write_stream(
    ctx: &mut Ctx<'_>,
    label: &Path,
    reader: &mut dyn Read,
    size: u64,
    dst: &Path,
    finish: Finish<'_>,
) -> Result<(), OpError> {
    ctx.progress
        .set_current(&label.file_name().unwrap_or_default().to_string_lossy());
    let mut output = OpenOptions::new().write(true).create_new(true).open(dst)?;
    let result = pump(ctx, reader, &mut output, size).and_then(|copied| match finish {
        // An archive entry that ends early (damaged data, a wrong password) must not leave a
        // plausible-looking truncated file behind.
        Finish::Stream { .. } if copied != size => Err(OpError::Io(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("dane kończą się po {copied} z {size} bajtów"),
        ))),
        _ => Ok(()),
    });
    match result {
        Ok(()) => {
            match finish {
                Finish::Copy(meta) => {
                    // The copy keeps the original's modification time (comparisons and
                    // synchronization rely on it), set before a read-only mode is applied.
                    if let Ok(time) = meta.modified() {
                        let _ = output.set_modified(time);
                    }
                    let _ = output.set_permissions(meta.permissions());
                }
                Finish::Stream { mode, modified } => {
                    if let Some(time) = modified {
                        let _ = output.set_modified(time);
                    }
                    drop(output);
                    if let Some(mode) = mode {
                        ctx.host.set_mode(dst, mode);
                    }
                }
            }
            ctx.outcome.files += 1;
            ctx.progress.finish_item();
            Ok(())
        }
        Err(e) => {
            drop(output);
            let _ = fs::remove_file(dst);
            Err(e)
        }
    }
}

fn pump(
    ctx: &mut Ctx<'_>,
    input: &mut dyn Read,
    output: &mut File,
    size_hint: u64,
) -> Result<u64, OpError> {
    // Small files get a small buffer: allocating 256 KiB per file hurts with 100k files.
    let len = usize::try_from(size_hint)
        .unwrap_or(COPY_CHUNK)
        .clamp(1, COPY_CHUNK);
    let mut buffer = vec![0u8; len];
    let mut total = 0u64;
    loop {
        ctx.progress.check()?;
        let read = input.read(&mut buffer)?;
        if read == 0 {
            return Ok(total);
        }
        output.write_all(&buffer[..read])?;
        total += read as u64;
        ctx.progress.add(read as u64);
        ctx.host.tick(ctx.progress);
    }
}

/// Writes files that do not come from the file system (entries of an archive) with the same
/// conflict handling, chunking, progress and cleanup as a file copy.
pub struct Writer<'a> {
    ctx: Ctx<'a>,
    dir_modes: Vec<(PathBuf, u32)>,
}

impl<'a> Writer<'a> {
    pub fn new(progress: &'a Progress, host: &'a mut dyn Host) -> Writer<'a> {
        Writer {
            ctx: Ctx {
                progress,
                host,
                outcome: Outcome::default(),
            },
            dir_modes: Vec::new(),
        }
    }

    /// Writes `reader` (`size` bytes) to `dst`, creating missing parent directories (archives
    /// often list a file without listing its directory), but only once the stream has
    /// delivered its first chunk. `label` names the source in
    /// conflict questions. An existing target is resolved through the host.
    pub fn write_file(
        &mut self,
        label: &Path,
        reader: &mut dyn Read,
        size: u64,
        modified: Option<SystemTime>,
        mode: Option<u32>,
        dst: &Path,
    ) -> Result<(), OpError> {
        self.ctx.progress.check()?;
        // Read the first chunk before touching the file system: a stream that fails at once
        // (an archive entry with a wrong password) must not leave directories behind.
        let mut head = Vec::new();
        (&mut *reader).take(8 * 1024).read_to_end(&mut head)?;
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut stream = io::Cursor::new(head).chain(reader);
        store_file(
            &mut self.ctx,
            label,
            &mut stream,
            size,
            modified,
            dst,
            Finish::Stream { mode, modified },
        )
    }

    /// Creates a directory (and missing parents). An existing directory is merged into.
    /// Its mode is applied by [`Writer::finish`], so a read-only directory does not block
    /// writing its content.
    pub fn make_dir(&mut self, dst: &Path, mode: Option<u32>) -> Result<(), OpError> {
        self.ctx.progress.check()?;
        match fs::symlink_metadata(dst) {
            Ok(existing) if existing.is_dir() => {}
            Ok(_) => {
                return Err(invalid_input(format!(
                    "cel jest plikiem, a źródło katalogiem: {}",
                    dst.display()
                )))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir_all(dst)?,
            Err(e) => return Err(e.into()),
        }
        if let Some(mode) = mode {
            self.dir_modes.push((dst.to_path_buf(), mode));
        }
        Ok(())
    }

    /// Counts a file that was left out on purpose (for the progress bar).
    pub fn note_skipped(&mut self, size: u64) {
        self.ctx.outcome.skipped += 1;
        self.ctx.progress.add(size);
        self.ctx.progress.finish_item();
    }

    /// Applies directory modes and returns what was done.
    pub fn finish(self) -> Outcome {
        for (path, mode) in self.dir_modes.iter().rev() {
            self.ctx.host.set_mode(path, *mode);
        }
        self.ctx.outcome
    }
}

/// Name for a duplicate of `path` placed next to it: `report.txt` becomes
/// `report (kopia).txt`, then `report (kopia 2).txt` and so on. The suffix goes before the
/// extension of files; directories and dot-files get it at the end.
pub fn duplicate_path(src: &Path) -> Result<PathBuf, OpError> {
    let invalid = || OpError::InvalidName(src.display().to_string());
    let dir = src.parent().ok_or_else(invalid)?;
    let name = src.file_name().ok_or_else(invalid)?.to_string_lossy();
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 && !src.is_dir() => (&name[..dot], &name[dot..]),
        _ => (&name[..], ""),
    };
    for n in 1.. {
        let label = if n == 1 {
            "kopia".to_string()
        } else {
            format!("kopia {n}")
        };
        let candidate = dir.join(format!("{stem} ({label}){ext}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }
    unreachable!("an unused duplicate name always exists")
}

/// Copies `src` next to itself under the name given by [`duplicate_path`].
pub fn duplicate(src: &Path, progress: &Progress) -> Result<PathBuf, OpError> {
    copy_to(src, &duplicate_path(src)?, progress)
}

/// Moves a file or directory into `dst_dir`. An existing target is an error. Falls back to
/// copy + delete when a plain rename is impossible (for example across volumes).
pub fn move_to(src: &Path, dst_dir: &Path, progress: &Progress) -> Result<PathBuf, OpError> {
    let target = target_for(src, dst_dir)?;
    move_path(src, &target, progress, &mut FailOnConflict)?;
    Ok(target)
}

/// Moves `src` into `dst_dir`, asking `host` about existing files. The source is removed
/// only when nothing was skipped.
pub fn move_with(
    src: &Path,
    dst_dir: &Path,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<Outcome, OpError> {
    let target = dst_dir.join(target_name(src)?);
    move_path(src, &target, progress, host)
}

fn move_path(
    src: &Path,
    target: &Path,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<Outcome, OpError> {
    progress.check()?;
    if fs::symlink_metadata(target).is_err() && fs::rename(src, target).is_ok() {
        progress.finish_item();
        return Ok(Outcome {
            files: 1,
            skipped: 0,
        });
    }
    let outcome = copy_path(src, target, progress, host)?;
    if outcome.skipped == 0 {
        let is_dir = fs::symlink_metadata(src)?.is_dir();
        if is_dir {
            fs::remove_dir_all(src)?;
        } else {
            fs::remove_file(src).or_else(|_| fs::remove_dir(src))?;
        }
    }
    Ok(outcome)
}

/// Counts what a copy or move of `paths` will process and stores it in `progress`
/// (`bytes_total`, `items_total`). Unreadable entries are ignored: the operation itself
/// reports them.
pub fn measure(paths: &[PathBuf], progress: &Progress) -> Result<(), OpError> {
    let (mut bytes, mut items) = (0u64, 0u64);
    let mut stack: Vec<PathBuf> = paths.to_vec();
    while let Some(path) = stack.pop() {
        progress.check()?;
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(read) = fs::read_dir(&path) {
                stack.extend(read.flatten().map(|e| e.path()));
            }
        } else {
            bytes += meta.len();
            items += 1;
        }
    }
    progress.set_total(bytes, items);
    Ok(())
}

fn check_name(name: &str) -> Result<&str, OpError> {
    let name = name.trim();
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || name.contains(':') && cfg!(windows);
    if bad {
        return Err(OpError::InvalidName(name.to_string()));
    }
    Ok(name)
}

/// Creates a directory called `name` inside `parent`.
pub fn mkdir(parent: &Path, name: &str) -> Result<PathBuf, OpError> {
    let name = check_name(name)?;
    let path = parent.join(name);
    if path.exists() {
        return Err(OpError::Exists(path));
    }
    fs::create_dir(&path)?;
    Ok(path)
}

/// Renames a file or directory within its directory. Never replaces an existing entry.
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, OpError> {
    let name = check_name(new_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| OpError::InvalidName(path.display().to_string()))?;
    let target = parent.join(name);
    if target == path {
        return Ok(target);
    }
    if let Ok(existing) = fs::symlink_metadata(&target) {
        // On a case-insensitive file system `a.txt` -> `A.txt` finds "itself": allow
        // that, but never anything else, and never through symlinks.
        let same_file = !existing.is_symlink()
            && !fs::symlink_metadata(path)?.is_symlink()
            && matches!(
                (fs::canonicalize(&target), fs::canonicalize(path)),
                (Ok(a), Ok(b)) if a == b
            );
        if !same_file {
            return Err(OpError::Exists(target));
        }
    }
    fs::rename(path, &target)?;
    Ok(target)
}

/// Moves a file or directory to the system trash (Recycle Bin on Windows, XDG trash on
/// Linux). Nothing in bluepane deletes permanently by default.
pub fn delete_to_trash(path: &Path) -> Result<(), OpError> {
    trash::delete(path).map_err(|e| OpError::Trash(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_keeps_the_modification_time() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.txt");
        fs::write(&src, "x").unwrap();
        let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
        File::options()
            .write(true)
            .open(&src)
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::create_dir(tmp.path().join("d")).unwrap();
        let copy = super::copy(&src, &tmp.path().join("d"), &Progress::new()).unwrap();
        assert_eq!(fs::metadata(copy).unwrap().modified().unwrap(), old);
    }

    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
        fs::write(tmp.path().join("src/a.txt"), "hello").unwrap();
        fs::write(tmp.path().join("src/sub/b.txt"), "world!").unwrap();
        fs::create_dir(tmp.path().join("dst")).unwrap();
        tmp
    }

    #[test]
    fn copies_a_tree_and_counts_bytes() {
        let tmp = tree();
        let progress = Progress::new();
        let out = copy(&tmp.path().join("src"), &tmp.path().join("dst"), &progress).unwrap();
        assert_eq!(fs::read_to_string(out.join("sub/b.txt")).unwrap(), "world!");
        assert_eq!(progress.bytes_done(), 11);
        assert!(tmp.path().join("src/a.txt").exists());
    }

    #[test]
    fn copy_refuses_to_overwrite() {
        let tmp = tree();
        let src = tmp.path().join("src/a.txt");
        let dst = tmp.path().join("dst");
        copy(&src, &dst, &Progress::new()).unwrap();
        assert!(matches!(
            copy(&src, &dst, &Progress::new()),
            Err(OpError::Exists(_))
        ));
    }

    #[test]
    fn copy_into_itself_is_rejected() {
        let tmp = tree();
        let src = tmp.path().join("src");
        assert!(copy(&src, &src.join("sub"), &Progress::new()).is_err());
    }

    #[test]
    fn duplicate_names_keep_the_extension() {
        let tmp = tree();
        let dir = tmp.path().join("src");
        assert_eq!(
            duplicate_path(&dir.join("a.txt")).unwrap(),
            dir.join("a (kopia).txt")
        );
        // Directories and dot-files have no extension to preserve.
        assert_eq!(
            duplicate_path(&dir.join("sub")).unwrap(),
            dir.join("sub (kopia)")
        );
        fs::write(dir.join(".rc"), "").unwrap();
        assert_eq!(
            duplicate_path(&dir.join(".rc")).unwrap(),
            dir.join(".rc (kopia)")
        );
        fs::write(dir.join("noext"), "").unwrap();
        assert_eq!(
            duplicate_path(&dir.join("noext")).unwrap(),
            dir.join("noext (kopia)")
        );
    }

    #[test]
    fn duplicate_never_overwrites() {
        let tmp = tree();
        let src = tmp.path().join("src/a.txt");
        let first = duplicate(&src, &Progress::new()).unwrap();
        let second = duplicate(&src, &Progress::new()).unwrap();
        let third = duplicate(&first, &Progress::new()).unwrap();
        assert!(first.ends_with("a (kopia).txt"));
        assert!(second.ends_with("a (kopia 2).txt"));
        assert!(third.ends_with("a (kopia) (kopia).txt"));
        assert_eq!(fs::read_to_string(&second).unwrap(), "hello");
        assert_eq!(fs::read_to_string(&src).unwrap(), "hello");
    }

    #[test]
    fn duplicate_copies_a_directory_tree() {
        let tmp = tree();
        let out = duplicate(&tmp.path().join("src"), &Progress::new()).unwrap();
        assert!(out.ends_with("src (kopia)"));
        assert_eq!(fs::read_to_string(out.join("sub/b.txt")).unwrap(), "world!");
    }

    #[test]
    fn copy_to_refuses_existing_target() {
        let tmp = tree();
        let src = tmp.path().join("src/a.txt");
        let target = tmp.path().join("dst/x.txt");
        copy_to(&src, &target, &Progress::new()).unwrap();
        assert!(matches!(
            copy_to(&src, &target, &Progress::new()),
            Err(OpError::Exists(_))
        ));
    }

    #[test]
    fn move_removes_the_source() {
        let tmp = tree();
        let src = tmp.path().join("src");
        let out = move_to(&src, &tmp.path().join("dst"), &Progress::new()).unwrap();
        assert!(!src.exists());
        assert!(out.join("a.txt").exists());
    }

    #[test]
    fn cancelled_copy_stops() {
        let tmp = tree();
        let progress = Progress::new();
        progress.cancel();
        let res = copy(&tmp.path().join("src"), &tmp.path().join("dst"), &progress);
        assert!(matches!(res, Err(OpError::Cancelled)));
    }

    #[test]
    fn mkdir_validates_names() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(mkdir(tmp.path(), "ok").unwrap().is_dir());
        for bad in ["", "..", "a/b", "a\\b"] {
            assert!(matches!(
                mkdir(tmp.path(), bad),
                Err(OpError::InvalidName(_))
            ));
        }
        assert!(matches!(mkdir(tmp.path(), "ok"), Err(OpError::Exists(_))));
    }

    /// Host with scripted answers that records what it was asked.
    #[derive(Default)]
    struct Script {
        answers: Vec<Result<Resolution, ()>>,
        asked: Vec<Conflict>,
        cancel_after_ticks: Option<u32>,
        ticks: u32,
    }

    impl Script {
        fn answering(answers: Vec<Resolution>) -> Script {
            Script {
                answers: answers.into_iter().map(Ok).collect(),
                ..Script::default()
            }
        }
    }

    impl Host for Script {
        fn conflict(&mut self, conflict: &Conflict) -> Result<Resolution, OpError> {
            self.asked.push(conflict.clone());
            if self.answers.is_empty() {
                return Err(OpError::Cancelled);
            }
            self.answers.remove(0).map_err(|()| OpError::Cancelled)
        }

        fn tick(&mut self, progress: &Progress) {
            self.ticks += 1;
            if Some(self.ticks) == self.cancel_after_ticks {
                progress.cancel();
            }
        }
    }

    /// `src/a.txt` ("new") and `dst/a.txt` ("old").
    fn clash() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::create_dir_all(tmp.path().join("dst")).unwrap();
        fs::write(tmp.path().join("src/a.txt"), "new").unwrap();
        fs::write(tmp.path().join("dst/a.txt"), "old").unwrap();
        tmp
    }

    fn text(path: PathBuf) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn overwrite_replaces_the_target() {
        let tmp = clash();
        let mut host = Script::answering(vec![Resolution::Overwrite]);
        let out = copy_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                files: 1,
                skipped: 0
            }
        );
        assert_eq!(text(tmp.path().join("dst/a.txt")), "new");
        assert_eq!(host.asked.len(), 1);
        assert_eq!(host.asked[0].src_size, 3);
        // No temporary file is left behind.
        assert_eq!(fs::read_dir(tmp.path().join("dst")).unwrap().count(), 1);
    }

    #[test]
    fn skip_leaves_the_target_and_is_counted() {
        let tmp = clash();
        let progress = Progress::new();
        let mut host = Script::answering(vec![Resolution::Skip]);
        let out = copy_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &progress,
            &mut host,
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                files: 0,
                skipped: 1
            }
        );
        assert_eq!(text(tmp.path().join("dst/a.txt")), "old");
        assert_eq!(progress.bytes_done(), 3);
        assert_eq!(progress.items_done(), 1);
    }

    #[test]
    fn keep_both_writes_under_a_free_name() {
        let tmp = clash();
        let mut host = Script::answering(vec![Resolution::KeepBoth]);
        copy_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        )
        .unwrap();
        assert_eq!(text(tmp.path().join("dst/a.txt")), "old");
        assert_eq!(text(tmp.path().join("dst/a (kopia).txt")), "new");
    }

    #[test]
    fn cancelling_at_a_conflict_aborts_and_changes_nothing() {
        let tmp = clash();
        let mut host = Script::default(); // no answers: cancels
        let res = copy_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        );
        assert!(matches!(res, Err(OpError::Cancelled)));
        assert_eq!(text(tmp.path().join("dst/a.txt")), "old");
    }

    #[test]
    fn existing_directories_are_merged_and_only_clashing_files_are_asked_about() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src/d"), tmp.path().join("dst/d"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("same.txt"), "from src").unwrap();
        fs::write(src.join("only_src.txt"), "1").unwrap();
        fs::write(dst.join("same.txt"), "from dst").unwrap();
        fs::write(dst.join("only_dst.txt"), "2").unwrap();
        let mut host = Script::answering(vec![Resolution::Skip]);
        let out = copy_with(&src, &tmp.path().join("dst"), &Progress::new(), &mut host).unwrap();
        assert_eq!(
            out,
            Outcome {
                files: 1,
                skipped: 1
            }
        );
        assert_eq!(host.asked.len(), 1);
        assert_eq!(text(dst.join("same.txt")), "from dst");
        assert_eq!(text(dst.join("only_src.txt")), "1");
        assert_eq!(text(dst.join("only_dst.txt")), "2");
    }

    #[test]
    fn a_file_cannot_replace_a_directory_or_the_other_way_round() {
        let tmp = tree();
        fs::write(tmp.path().join("dst/sub"), "file").unwrap();
        let res = copy_with(
            &tmp.path().join("src/sub"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut Script::default(),
        );
        assert!(matches!(res, Err(OpError::Io(_))));
        fs::create_dir(tmp.path().join("dst/a.txt")).unwrap();
        let res = copy_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut Script::default(),
        );
        assert!(matches!(res, Err(OpError::Io(_))));
    }

    #[test]
    fn copy_reports_bytes_in_chunks_and_a_cancel_removes_the_partial_file() {
        let tmp = tree();
        let big = tmp.path().join("src/big.bin");
        fs::write(&big, vec![7u8; 4 * COPY_CHUNK]).unwrap();
        let progress = Progress::new();
        let mut host = Script {
            cancel_after_ticks: Some(2),
            ..Script::default()
        };
        let res = copy_with(&big, &tmp.path().join("dst"), &progress, &mut host);
        assert!(matches!(res, Err(OpError::Cancelled)));
        assert_eq!(progress.bytes_done(), 2 * COPY_CHUNK as u64);
        assert!(!tmp.path().join("dst/big.bin").exists());
        assert_eq!(progress.current(), "big.bin");
    }

    #[test]
    fn a_cancelled_overwrite_keeps_the_old_file_and_leaves_no_temp_file() {
        let tmp = clash();
        fs::write(tmp.path().join("src/a.txt"), vec![1u8; 3 * COPY_CHUNK]).unwrap();
        let mut host = Script::answering(vec![Resolution::Overwrite]);
        host.cancel_after_ticks = Some(1);
        let res = copy_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        );
        assert!(matches!(res, Err(OpError::Cancelled)));
        assert_eq!(text(tmp.path().join("dst/a.txt")), "old");
        assert_eq!(fs::read_dir(tmp.path().join("dst")).unwrap().count(), 1);
    }

    #[test]
    fn move_with_skip_keeps_the_source() {
        let tmp = clash();
        let mut host = Script::answering(vec![Resolution::Skip]);
        let out = move_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        )
        .unwrap();
        assert_eq!(out.skipped, 1);
        assert!(tmp.path().join("src/a.txt").exists());
        assert_eq!(text(tmp.path().join("dst/a.txt")), "old");
    }

    #[test]
    fn move_with_overwrite_removes_the_source() {
        let tmp = clash();
        let mut host = Script::answering(vec![Resolution::Overwrite]);
        move_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        )
        .unwrap();
        assert!(!tmp.path().join("src/a.txt").exists());
        assert_eq!(text(tmp.path().join("dst/a.txt")), "new");
    }

    #[test]
    fn move_without_a_clash_does_not_ask() {
        let tmp = tree();
        let mut host = Script::default();
        move_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        )
        .unwrap();
        assert!(host.asked.is_empty());
        assert!(tmp.path().join("dst/a.txt").exists());
    }

    #[test]
    fn measure_counts_bytes_and_files_recursively() {
        let tmp = tree();
        let progress = Progress::new();
        measure(&[tmp.path().join("src")], &progress).unwrap();
        assert_eq!(progress.bytes_total(), 11);
        assert_eq!(progress.items_total(), 2);
        let cancelled = Progress::new();
        cancelled.cancel();
        assert!(matches!(
            measure(&[tmp.path().join("src")], &cancelled),
            Err(OpError::Cancelled)
        ));
    }

    #[test]
    fn rename_changes_the_name_in_place() {
        let tmp = tree();
        let out = rename(&tmp.path().join("src/a.txt"), " b.txt ").unwrap();
        assert_eq!(out, tmp.path().join("src/b.txt"));
        assert!(!tmp.path().join("src/a.txt").exists());
        assert_eq!(rename(&out, "b.txt").unwrap(), out);
    }

    #[test]
    fn rename_never_replaces_or_accepts_bad_names() {
        let tmp = tree();
        fs::write(tmp.path().join("src/b.txt"), "other").unwrap();
        let a = tmp.path().join("src/a.txt");
        assert!(matches!(rename(&a, "b.txt"), Err(OpError::Exists(_))));
        assert_eq!(text(tmp.path().join("src/b.txt")), "other");
        for bad in ["", "..", "x/y", "x\\y"] {
            assert!(matches!(rename(&a, bad), Err(OpError::InvalidName(_))));
        }
        assert!(a.exists());
    }

    #[test]
    fn rename_moves_directories_too() {
        let tmp = tree();
        let out = rename(&tmp.path().join("src/sub"), "renamed").unwrap();
        assert!(out.join("b.txt").exists());
    }

    fn set_mtime(path: &Path, secs_since_epoch: u64) {
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs_since_epoch);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn overwrite_older_replaces_only_when_the_source_is_newer() {
        let cases = [
            (2_000, 1_000, "new"),
            (1_000, 2_000, "old"),
            (1_000, 1_000, "old"),
        ];
        for (src_time, dst_time, expected) in cases {
            let tmp = clash();
            set_mtime(&tmp.path().join("src/a.txt"), src_time);
            set_mtime(&tmp.path().join("dst/a.txt"), dst_time);
            let mut host = Script::answering(vec![Resolution::OverwriteOlder]);
            let out = copy_with(
                &tmp.path().join("src/a.txt"),
                &tmp.path().join("dst"),
                &Progress::new(),
                &mut host,
            )
            .unwrap();
            assert_eq!(
                text(tmp.path().join("dst/a.txt")),
                expected,
                "src {src_time}, dst {dst_time}"
            );
            assert_eq!(out.skipped, u64::from(expected == "old"));
        }
    }

    #[test]
    fn a_move_that_skipped_an_older_source_keeps_it() {
        let tmp = clash();
        set_mtime(&tmp.path().join("src/a.txt"), 1_000);
        set_mtime(&tmp.path().join("dst/a.txt"), 2_000);
        let mut host = Script::answering(vec![Resolution::OverwriteOlder]);
        move_with(
            &tmp.path().join("src/a.txt"),
            &tmp.path().join("dst"),
            &Progress::new(),
            &mut host,
        )
        .unwrap();
        assert!(tmp.path().join("src/a.txt").exists());
        assert_eq!(text(tmp.path().join("dst/a.txt")), "old");
    }

    /// A host that records the modes it was asked to set.
    #[derive(Default)]
    struct ModeHost {
        script: Script,
        modes: std::cell::RefCell<Vec<(PathBuf, u32)>>,
    }

    impl Host for ModeHost {
        fn conflict(&mut self, conflict: &Conflict) -> Result<Resolution, OpError> {
            self.script.conflict(conflict)
        }

        fn set_mode(&self, path: &Path, mode: u32) {
            self.modes.borrow_mut().push((path.to_path_buf(), mode));
        }

        fn tick(&mut self, progress: &Progress) {
            self.script.tick(progress);
        }
    }

    #[test]
    fn writer_stores_a_stream_with_its_time() {
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = ModeHost::default();
        let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let mut writer = Writer::new(&progress, &mut host);
        let mut data: &[u8] = b"streamed";
        writer
            .write_file(
                Path::new("in.zip/a.txt"),
                &mut data,
                8,
                Some(stamp),
                None,
                &tmp.path().join("a.txt"),
            )
            .unwrap();
        let out = writer.finish();
        assert_eq!(
            out,
            Outcome {
                files: 1,
                skipped: 0
            }
        );
        assert_eq!(text(tmp.path().join("a.txt")), "streamed");
        assert_eq!(
            fs::metadata(tmp.path().join("a.txt"))
                .unwrap()
                .modified()
                .unwrap(),
            stamp
        );
        assert_eq!(progress.bytes_done(), 8);
        assert_eq!(progress.current(), "a.txt");
    }

    #[test]
    fn writer_asks_about_existing_files_like_a_copy() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), "old").unwrap();
        let progress = Progress::new();
        let mut host = ModeHost {
            script: Script::answering(vec![Resolution::KeepBoth, Resolution::Skip]),
            ..ModeHost::default()
        };
        let mut writer = Writer::new(&progress, &mut host);
        for _ in 0..2 {
            let mut data: &[u8] = b"new";
            writer
                .write_file(
                    Path::new("x/a.txt"),
                    &mut data,
                    3,
                    None,
                    None,
                    &tmp.path().join("a.txt"),
                )
                .unwrap();
        }
        let out = writer.finish();
        assert_eq!(
            out,
            Outcome {
                files: 1,
                skipped: 1
            }
        );
        assert_eq!(text(tmp.path().join("a.txt")), "old");
        assert_eq!(text(tmp.path().join("a (kopia).txt")), "new");
    }

    #[test]
    fn writer_removes_a_partial_file_when_the_stream_fails() {
        struct Failing(usize);
        impl Read for Failing {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0 == 0 {
                    return Err(io::Error::other("corrupt data"));
                }
                self.0 -= 1;
                buf[..4].copy_from_slice(b"data");
                Ok(4)
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = FailOnConflict;
        let mut writer = Writer::new(&progress, &mut host);
        let res = writer.write_file(
            Path::new("x/f"),
            &mut Failing(5000), // fails after about 20 KB, well past the first chunk
            100_000,
            None,
            None,
            &tmp.path().join("f"),
        );
        assert!(matches!(res, Err(OpError::Io(_))));
        assert!(!tmp.path().join("f").exists());
    }

    #[test]
    fn writer_makes_directories_and_merges_existing_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = FailOnConflict;
        let mut writer = Writer::new(&progress, &mut host);
        writer
            .make_dir(&tmp.path().join("a/b"), Some(0o755))
            .unwrap();
        writer.make_dir(&tmp.path().join("a/b"), None).unwrap();
        assert!(tmp.path().join("a/b").is_dir());
        fs::write(tmp.path().join("file"), "x").unwrap();
        assert!(writer.make_dir(&tmp.path().join("file"), None).is_err());
        writer.finish();
    }

    #[test]
    fn writer_counts_skipped_entries_and_honours_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = FailOnConflict;
        let mut writer = Writer::new(&progress, &mut host);
        writer.note_skipped(40);
        progress.cancel();
        let mut data: &[u8] = b"x";
        let res = writer.write_file(
            Path::new("x/a"),
            &mut data,
            1,
            None,
            None,
            &tmp.path().join("a"),
        );
        assert!(matches!(res, Err(OpError::Cancelled)));
        let out = writer.finish();
        assert_eq!(out.skipped, 1);
        assert_eq!(progress.bytes_done(), 40);
    }

    #[test]
    fn writer_creates_missing_parent_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = FailOnConflict;
        let mut writer = Writer::new(&progress, &mut host);
        let mut data: &[u8] = b"deep";
        writer
            .write_file(
                Path::new("x/f"),
                &mut data,
                4,
                None,
                None,
                &tmp.path().join("a/b/c/f"),
            )
            .unwrap();
        assert_eq!(text(tmp.path().join("a/b/c/f")), "deep");
    }

    #[test]
    fn a_stream_that_fails_at_once_leaves_nothing_behind() {
        struct FailsAtOnce;
        impl Read for FailsAtOnce {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("wrong password"))
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = FailOnConflict;
        let mut writer = Writer::new(&progress, &mut host);
        let res = writer.write_file(
            Path::new("x/f"),
            &mut FailsAtOnce,
            10,
            None,
            None,
            &tmp.path().join("new/dir/f"),
        );
        assert!(matches!(res, Err(OpError::Io(_))));
        assert!(
            !tmp.path().join("new").exists(),
            "no directory may be created"
        );
    }

    #[test]
    fn a_stream_shorter_than_declared_is_an_error_and_leaves_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let progress = Progress::new();
        let mut host = FailOnConflict;
        let mut writer = Writer::new(&progress, &mut host);
        let mut data: &[u8] = b"only 10 b.";
        let res = writer.write_file(
            Path::new("x/f"),
            &mut data,
            50,
            None,
            None,
            &tmp.path().join("f"),
        );
        assert!(matches!(res, Err(OpError::Io(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof));
        assert!(!tmp.path().join("f").exists());
    }
}
