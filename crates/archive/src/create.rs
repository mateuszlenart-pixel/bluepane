//! Creating archives.
//!
//! The archive is first built in a temporary file next to the final one and only then put in
//! place (renamed, or cut into volumes), so a failure or a cancel never leaves a half-written
//! archive and never damages an existing one. Splitting therefore needs room for the archive
//! twice while it runs.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bluepane_ops::{Conflict, Host, Progress, Resolution};
use flate2::write::GzEncoder;
use flate2::Compression;
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

use crate::format::Format;
use crate::split::SplitWriter;
use crate::time::to_zip;
use crate::ArchiveError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// No compression.
    Store,
    Fast,
    Normal,
    Maximum,
}

impl Level {
    fn deflate_level(self) -> i64 {
        match self {
            Level::Store => 0,
            Level::Fast => 1,
            Level::Normal => 6,
            Level::Maximum => 9,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateOptions {
    pub format: Format,
    pub level: Level,
    /// ZIP only (AES-256). Empty counts as none.
    pub password: Option<String>,
    /// Cut the archive into volumes of at most this many bytes (`name.zip.001`, ...).
    pub split: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateReport {
    /// Files stored.
    pub files: u64,
    /// Symlinks, devices and other entries that were left out.
    pub left_out: u64,
    /// The file(s) that make up the archive: one, or every volume.
    pub volumes: Vec<PathBuf>,
    /// The user chose to skip because the archive already existed; nothing was written.
    pub skipped: bool,
}

enum Kind {
    Dir,
    File { size: u64 },
}

struct Item {
    path: PathBuf,
    /// Name inside the archive, `/` separated.
    name: String,
    kind: Kind,
    modified: Option<SystemTime>,
    mode: Option<u32>,
}

/// Removes the temporary file unless it was handed over.
struct TempFile {
    path: PathBuf,
    keep: bool,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Counts bytes read from a source file and stops reading once the job is cancelled.
struct SourceReader<'a> {
    file: File,
    progress: &'a Progress,
}

impl Read for SourceReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.progress.is_cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        let n = self.file.read(buf)?;
        self.progress.add(n as u64);
        Ok(n)
    }
}

fn volume_name(base: &Path, number: usize) -> PathBuf {
    let name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.with_file_name(format!("{name}.{number:03}"))
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Collects what goes into the archive. Symlinks to files are followed; symlinks to
/// directories, broken links and special files are left out (and counted).
fn collect(
    sources: &[PathBuf],
    own_files: &[PathBuf],
    progress: &Progress,
) -> Result<(Vec<Item>, u64), ArchiveError> {
    let mut items = Vec::new();
    let mut left_out = 0;
    let mut stack: Vec<(PathBuf, String)> = Vec::new();
    for source in sources.iter().rev() {
        let name = source
            .file_name()
            .ok_or_else(|| ArchiveError::Op(format!("niepoprawna ścieżka: {}", source.display())))?
            .to_string_lossy()
            .into_owned();
        stack.push((source.clone(), name));
    }
    while let Some((path, name)) = stack.pop() {
        progress.check()?;
        if own_files.contains(&path) {
            continue; // never pack the archive into itself
        }
        let link = fs::symlink_metadata(&path)?;
        let meta = if link.is_symlink() {
            match fs::metadata(&path) {
                Ok(target) if target.is_file() => target,
                _ => {
                    left_out += 1;
                    continue;
                }
            }
        } else {
            link
        };
        if meta.is_dir() {
            items.push(Item {
                path: path.clone(),
                name: name.clone(),
                kind: Kind::Dir,
                modified: meta.modified().ok(),
                mode: None,
            });
            let mut children: Vec<_> = fs::read_dir(&path)?
                .filter_map(Result::ok)
                .map(|e| (e.path(), e.file_name().to_string_lossy().into_owned()))
                .collect();
            // Reverse name order on the stack gives name order when popped.
            children.sort_by(|a, b| b.1.cmp(&a.1));
            for (child, child_name) in children {
                stack.push((child, format!("{name}/{child_name}")));
            }
        } else if meta.is_file() {
            items.push(Item {
                path,
                name,
                kind: Kind::File { size: meta.len() },
                modified: meta.modified().ok(),
                mode: None,
            });
        } else {
            left_out += 1;
        }
    }
    Ok((items, left_out))
}

fn io_failure(progress: &Progress, error: io::Error) -> ArchiveError {
    if progress.is_cancelled() {
        ArchiveError::Cancelled
    } else {
        ArchiveError::Io(error)
    }
}

fn zip_failure(progress: &Progress, error: zip::result::ZipError) -> ArchiveError {
    match error {
        zip::result::ZipError::Io(e) => io_failure(progress, e),
        other => ArchiveError::Op(other.to_string()),
    }
}

fn write_zip(
    items: &[Item],
    temp: &Path,
    options: &CreateOptions,
    password: Option<&str>,
    progress: &Progress,
) -> Result<(), ArchiveError> {
    let mut zip = ZipWriter::new(File::create(temp)?);
    let method = if options.level == Level::Store {
        CompressionMethod::Stored
    } else {
        CompressionMethod::Deflated
    };
    for item in items {
        progress.check()?;
        progress.set_current(&item.name);
        let mut file_options = SimpleFileOptions::default()
            .compression_method(method)
            .compression_level(match options.level {
                Level::Store => None,
                level => Some(level.deflate_level()),
            });
        if let Some(stamp) = item.modified.and_then(to_zip) {
            file_options = file_options.last_modified_time(stamp);
        }
        if let Some(mode) = item.mode {
            file_options = file_options.unix_permissions(mode);
        }
        match item.kind {
            Kind::Dir => {
                zip.add_directory(format!("{}/", item.name), file_options)
                    .map_err(|e| zip_failure(progress, e))?;
                progress.finish_item();
            }
            Kind::File { size } => {
                file_options = file_options.large_file(size >= u64::from(u32::MAX));
                if let Some(password) = password {
                    file_options = file_options.with_aes_encryption(AesMode::Aes256, password);
                }
                zip.start_file(item.name.as_str(), file_options)
                    .map_err(|e| zip_failure(progress, e))?;
                let mut reader = SourceReader {
                    file: File::open(&item.path)?,
                    progress,
                };
                io::copy(&mut reader, &mut zip).map_err(|e| io_failure(progress, e))?;
                progress.finish_item();
            }
        }
    }
    zip.finish().map_err(|e| zip_failure(progress, e))?;
    Ok(())
}

fn tar_header(item: &Item, size: u64, entry_type: tar::EntryType) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_size(size);
    header.set_entry_type(entry_type);
    header.set_mode(item.mode.unwrap_or(match entry_type {
        tar::EntryType::Directory => 0o755,
        _ => 0o644,
    }));
    let secs = item
        .modified
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    header.set_mtime(secs);
    header
}

fn write_tar<W: Write>(items: &[Item], sink: W, progress: &Progress) -> Result<W, ArchiveError> {
    let mut builder = tar::Builder::new(sink);
    for item in items {
        progress.check()?;
        progress.set_current(&item.name);
        match item.kind {
            Kind::Dir => {
                let mut header = tar_header(item, 0, tar::EntryType::Directory);
                builder
                    .append_data(&mut header, format!("{}/", item.name), io::empty())
                    .map_err(|e| io_failure(progress, e))?;
            }
            Kind::File { size } => {
                let mut header = tar_header(item, size, tar::EntryType::Regular);
                let reader = SourceReader {
                    file: File::open(&item.path)?,
                    progress,
                };
                builder
                    .append_data(&mut header, &item.name, reader)
                    .map_err(|e| io_failure(progress, e))?;
            }
        }
        progress.finish_item();
    }
    builder.finish().map_err(|e| io_failure(progress, e))?;
    builder.into_inner().map_err(|e| io_failure(progress, e))
}

/// Finds a free name for the archive when the user chose "keep both".
fn free_output(output: &Path, split: bool) -> Result<PathBuf, ArchiveError> {
    let mut candidate = output.to_path_buf();
    loop {
        candidate = bluepane_ops::duplicate_path(&candidate)?;
        let taken = if split {
            exists(&volume_name(&candidate, 1))
        } else {
            exists(&candidate)
        };
        if !taken {
            return Ok(candidate);
        }
    }
}

/// Packs `sources` into the archive `output` (its full path, including the extension).
///
/// If the archive (or its first volume) already exists, `host` decides: overwrite, skip or
/// keep both. Nothing is changed until the new archive is complete.
pub fn create(
    sources: &[PathBuf],
    output: &Path,
    options: &CreateOptions,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<CreateReport, ArchiveError> {
    let password = options.password.as_deref().filter(|p| !p.is_empty());
    if password.is_some() && !options.format.supports_password() {
        return Err(ArchiveError::Unsupported(
            "hasło można ustawić tylko dla ZIP (TAR nie szyfruje)".into(),
        ));
    }
    if options.split == Some(0) {
        return Err(ArchiveError::Unsupported(
            "rozmiar części musi być większy od zera".into(),
        ));
    }
    if sources.is_empty() {
        return Err(ArchiveError::Op("nic do spakowania".into()));
    }
    let split = options.split.is_some();
    let temp_path = {
        let name = output
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        output.with_file_name(format!(".{name}.bluepane-tmp"))
    };
    let mut own_files = vec![output.to_path_buf(), temp_path.clone()];
    own_files.extend(
        (1..=999)
            .map(|n| volume_name(output, n))
            .take_while(|v| exists(v)),
    );
    let (items, left_out) = collect(sources, &own_files, progress)?;
    let (bytes, files) = items
        .iter()
        .fold((0u64, 0u64), |(b, f), item| match item.kind {
            Kind::File { size } => (b + size, f + 1),
            Kind::Dir => (b, f),
        });
    progress.set_total(bytes, items.len() as u64);

    // Settle what to do about an existing archive before spending time on a new one.
    let first = if split {
        volume_name(output, 1)
    } else {
        output.to_path_buf()
    };
    let mut output = output.to_path_buf();
    let mut replace = false;
    if exists(&first) {
        let existing = fs::metadata(&first).ok();
        let conflict = Conflict {
            src: sources[0].clone(),
            dst: first.clone(),
            src_size: bytes,
            dst_size: existing.as_ref().map_or(0, |m| m.len()),
            // A new archive is newer than any existing one.
            src_modified: Some(SystemTime::now()),
            dst_modified: existing.and_then(|m| m.modified().ok()),
        };
        match host.conflict(&conflict)? {
            Resolution::Skip => {
                return Ok(CreateReport {
                    left_out,
                    skipped: true,
                    ..CreateReport::default()
                })
            }
            Resolution::Overwrite | Resolution::OverwriteOlder => replace = true,
            Resolution::KeepBoth => output = free_output(&output, split)?,
        }
    }

    // Permissions are platform specific: ask the host for each file.
    let items: Vec<Item> = items
        .into_iter()
        .map(|mut item| {
            if matches!(item.kind, Kind::File { .. }) {
                item.mode = host.file_mode(&item.path);
            }
            item
        })
        .collect();

    let mut temp = TempFile {
        path: temp_path,
        keep: false,
    };
    match options.format {
        Format::Zip => write_zip(&items, &temp.path, options, password, progress)?,
        Format::Tar => {
            let mut file = write_tar(&items, File::create(&temp.path)?, progress)?;
            file.flush().map_err(|e| io_failure(progress, e))?;
        }
        Format::TarGz => {
            let level = match options.level {
                Level::Store => Compression::none(),
                Level::Fast => Compression::fast(),
                Level::Normal => Compression::default(),
                Level::Maximum => Compression::best(),
            };
            let encoder = GzEncoder::new(File::create(&temp.path)?, level);
            let encoder = write_tar(&items, encoder, progress)?;
            encoder.finish().map_err(|e| io_failure(progress, e))?;
        }
    }
    progress.check()?;

    // Put the finished archive in place.
    let mut volumes = Vec::new();
    if split {
        if replace {
            for n in 1.. {
                let old = volume_name(&output, n);
                if !exists(&old) {
                    break;
                }
                host.make_writable(&old);
                fs::remove_file(&old)?;
            }
        }
        let mut writer = SplitWriter::new(&output, options.split.unwrap_or(u64::MAX));
        let copied = File::open(&temp.path)
            .and_then(|mut source| io::copy(&mut source, &mut writer))
            .and_then(|_| writer.flush());
        if let Err(e) = copied {
            for part in writer.parts() {
                let _ = fs::remove_file(part);
            }
            return Err(io_failure(progress, e));
        }
        volumes.extend_from_slice(writer.parts());
    } else {
        if replace {
            host.make_writable(&output);
            fs::remove_file(&output)?;
        }
        fs::rename(&temp.path, &output)?;
        temp.keep = true; // moved: nothing left to clean up
        volumes.push(output);
    }
    Ok(CreateReport {
        files,
        left_out,
        volumes,
        skipped: false,
    })
}
