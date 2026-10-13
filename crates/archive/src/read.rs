//! Reading archives: listing, extracting and testing.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bluepane_ops::{Host, OpError, Outcome, Progress, Writer};
use flate2::read::MultiGzDecoder;
use zip::result::ZipError;
use zip::ZipArchive;

use crate::format::{detect, volumes_of, Format};
use crate::listing::{is_selected, normalize_name, ArchiveEntry, Listing};
use crate::split::Joined;
use crate::time::from_zip;
use crate::ArchiveError;

/// Which part of an archive to extract and where it is rooted.
#[derive(Debug, Clone, Copy)]
pub struct Selection<'a> {
    /// The directory inside the archive that the extracted names are relative to (`""` for
    /// the top level). Extracting `docs/a.txt` with `strip = "docs"` writes `a.txt`.
    pub strip: &'a str,
    /// Items (files or directories) to extract; empty means the whole archive.
    pub entries: &'a [String],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractReport {
    pub outcome: Outcome,
    /// Symlinks, hard links and devices that were left out on purpose.
    pub special_skipped: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifyReport {
    pub files: u64,
    pub bytes: u64,
    /// One message per damaged entry.
    pub damaged: Vec<String>,
}

/// Counts compressed bytes read and stops reading once the operation is cancelled.
struct Counting<'a, R> {
    inner: R,
    progress: &'a Progress,
    count: bool,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.progress.is_cancelled() {
            // Not `Interrupted`: std would retry that and never stop.
            return Err(io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        if self.count {
            self.progress.add(n as u64);
        }
        Ok(n)
    }
}

fn zip_error(error: ZipError) -> ArchiveError {
    match error {
        ZipError::Io(e) => ArchiveError::Io(e),
        ZipError::InvalidPassword => ArchiveError::WrongPassword,
        ZipError::UnsupportedArchive(what) if what == ZipError::PASSWORD_REQUIRED => {
            ArchiveError::NeedsPassword
        }
        ZipError::UnsupportedArchive(what) => ArchiveError::Unsupported(what.to_string()),
        ZipError::CompressionMethodNotSupported(id) => {
            ArchiveError::Unsupported(format!("metoda kompresji ZIP numer {id}"))
        }
        ZipError::InvalidArchive(what) => ArchiveError::Corrupt(what.to_string()),
        ZipError::FileNotFound => ArchiveError::Corrupt("brak wpisu w archiwum".into()),
        other => ArchiveError::Corrupt(other.to_string()),
    }
}

/// An I/O failure while reading an archive: a cancel, damage or a real I/O problem.
fn io_failure(progress: &Progress, error: io::Error, encrypted: bool) -> ArchiveError {
    if progress.is_cancelled() {
        return ArchiveError::Cancelled;
    }
    let text = error.to_string().to_lowercase();
    if encrypted && (text.contains("checksum") || error.kind() == io::ErrorKind::InvalidData) {
        // The check value of a ZipCrypto password is one byte, so a wrong password can pass
        // the open and only fail here.
        return ArchiveError::WrongPassword;
    }
    match error.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => {
            ArchiveError::Corrupt(error.to_string())
        }
        _ => ArchiveError::Io(error),
    }
}

pub(crate) fn op_failure(progress: &Progress, error: OpError, encrypted: bool) -> ArchiveError {
    match error {
        OpError::Io(e) => io_failure(progress, e, encrypted),
        other => other.into(),
    }
}

fn open_zip(volumes: &[PathBuf]) -> Result<ZipArchive<Joined>, ArchiveError> {
    let joined = Joined::open(volumes)?;
    ZipArchive::new(joined).map_err(zip_error)
}

fn tar_reader<'a>(
    volumes: &[PathBuf],
    format: Format,
    progress: &'a Progress,
    count: bool,
) -> Result<Box<dyn Read + 'a>, ArchiveError> {
    let reader = Counting {
        inner: Joined::open(volumes)?,
        progress,
        count,
    };
    Ok(match format {
        Format::TarGz => Box::new(MultiGzDecoder::new(reader)),
        _ => Box::new(reader),
    })
}

fn packed_size(volumes: &[PathBuf]) -> u64 {
    volumes
        .iter()
        .filter_map(|v| std::fs::metadata(v).ok())
        .map(|m| m.len())
        .sum()
}

/// What kind of thing a tar header describes.
enum TarKind {
    File,
    Dir,
    Special,
}

fn tar_kind(kind: tar::EntryType) -> TarKind {
    match kind {
        tar::EntryType::Regular | tar::EntryType::Continuous | tar::EntryType::GNUSparse => {
            TarKind::File
        }
        tar::EntryType::Directory => TarKind::Dir,
        _ => TarKind::Special,
    }
}

fn tar_time(header: &tar::Header) -> Option<SystemTime> {
    header
        .mtime()
        .ok()
        .map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
}

fn format_of(path: &Path) -> Result<Format, ArchiveError> {
    detect(path).ok_or_else(|| ArchiveError::Unsupported("nieznany format archiwum".into()))
}

/// Reads the table of contents. For ZIP that is quick (the central directory); TAR and
/// TAR.GZ have to be read through, so the progress counts the bytes read of the file. The
/// password matters only for a 7z archive whose file names are encrypted.
pub fn list(
    path: &Path,
    password: Option<&str>,
    progress: &Progress,
) -> Result<Listing, ArchiveError> {
    let format = format_of(path)?;
    let volumes = volumes_of(path);
    let packed = packed_size(&volumes);
    progress.set_total(packed, 0);
    progress.set_current(&path.file_name().unwrap_or_default().to_string_lossy());
    let mut raw = Vec::new();
    let mut unsafe_skipped = 0;
    match format {
        Format::Zip => {
            let mut archive = open_zip(&volumes)?;
            for index in 0..archive.len() {
                progress.check()?;
                let file = archive.by_index_raw(index).map_err(zip_error)?;
                let name = file.name().map_err(zip_error)?.into_owned();
                let Some(path) = normalize_name(&name) else {
                    unsafe_skipped += 1;
                    continue;
                };
                let is_dir = file.is_dir();
                raw.push(ArchiveEntry {
                    path,
                    is_dir,
                    size: if is_dir { 0 } else { file.size() },
                    modified: file.last_modified().and_then(from_zip),
                    encrypted: file.encrypted(),
                    mode: file.unix_mode().map(|m| m & 0o777),
                    special: file.is_symlink(),
                });
            }
            progress.add(packed);
        }
        Format::SevenZ => {
            let (entries, skipped) = crate::seven::list(&volumes, password, progress)?;
            raw = entries;
            unsafe_skipped = skipped;
            progress.add(packed);
        }
        Format::Tar | Format::TarGz => {
            let reader = tar_reader(&volumes, format, progress, true)?;
            let mut archive = tar::Archive::new(reader);
            let entries = archive
                .entries()
                .map_err(|e| io_failure(progress, e, false))?;
            for entry in entries {
                let entry = entry.map_err(|e| io_failure(progress, e, false))?;
                let name = entry.path().map_err(|e| io_failure(progress, e, false))?;
                let Some(path) = normalize_name(&name.to_string_lossy()) else {
                    unsafe_skipped += 1;
                    continue;
                };
                let header = entry.header();
                let kind = tar_kind(header.entry_type());
                raw.push(ArchiveEntry {
                    path,
                    is_dir: matches!(kind, TarKind::Dir),
                    size: if matches!(kind, TarKind::File) {
                        header.size().unwrap_or(0)
                    } else {
                        0
                    },
                    modified: tar_time(header),
                    encrypted: false,
                    mode: header.mode().ok().map(|m| m & 0o777),
                    special: matches!(kind, TarKind::Special),
                });
            }
        }
    }
    Ok(Listing::build(
        format,
        volumes.len(),
        packed,
        raw,
        unsafe_skipped,
    ))
}

/// `path` below `strip`, or `None` when it is `strip` itself or lies elsewhere.
pub(crate) fn below<'p>(path: &'p str, strip: &str) -> Option<&'p str> {
    if strip.is_empty() {
        return Some(path);
    }
    path.strip_prefix(strip)?.strip_prefix('/')
}

pub(crate) fn wanted(path: &str, selection: &Selection<'_>) -> bool {
    selection.entries.is_empty() || selection.entries.iter().any(|s| is_selected(path, s))
}

/// Extracts the selection into `dest`. Existing files are handled through `host` (ask,
/// overwrite, skip, keep both). Names that would escape `dest` are never written, and links
/// and devices are skipped and counted.
pub fn extract(
    path: &Path,
    selection: &Selection<'_>,
    dest: &Path,
    password: Option<&str>,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<ExtractReport, ArchiveError> {
    let format = format_of(path)?;
    let volumes = volumes_of(path);
    let label_root = PathBuf::from(path.file_name().unwrap_or_default());
    let mut writer = Writer::new(progress, host);
    let mut special_skipped = 0;
    match format {
        Format::Zip => {
            let mut archive = open_zip(&volumes)?;
            // Settle the password before writing anything, so a missing or wrong one does
            // not leave a half-extracted tree of empty directories behind.
            for index in 0..archive.len() {
                let (name, is_dir, special, encrypted) = {
                    let file = archive.by_index_raw(index).map_err(zip_error)?;
                    (
                        file.name().map_err(zip_error)?.into_owned(),
                        file.is_dir(),
                        file.is_symlink(),
                        file.encrypted(),
                    )
                };
                let wanted_file = !is_dir
                    && !special
                    && encrypted
                    && normalize_name(&name).is_some_and(|path| {
                        wanted(&path, selection) && below(&path, selection.strip).is_some()
                    });
                if wanted_file {
                    match password {
                        None => return Err(ArchiveError::NeedsPassword),
                        Some(password) => {
                            archive
                                .by_index_decrypt(index, password.as_bytes())
                                .map_err(zip_error)?;
                        }
                    }
                    break;
                }
            }
            for index in 0..archive.len() {
                progress.check()?;
                // Look at the entry without decrypting it.
                let (name, is_dir, special, encrypted, size, modified, mode) = {
                    let file = archive.by_index_raw(index).map_err(zip_error)?;
                    (
                        file.name().map_err(zip_error)?.into_owned(),
                        file.is_dir(),
                        file.is_symlink(),
                        file.encrypted(),
                        file.size(),
                        file.last_modified().and_then(from_zip),
                        file.unix_mode().map(|m| m & 0o777),
                    )
                };
                let Some(entry_path) = normalize_name(&name) else {
                    continue;
                };
                if !wanted(&entry_path, selection) {
                    continue;
                }
                let Some(relative) = below(&entry_path, selection.strip) else {
                    continue;
                };
                if special {
                    special_skipped += 1;
                    continue;
                }
                let target = dest.join(relative);
                if is_dir {
                    writer
                        .make_dir(&target, mode)
                        .map_err(|e| op_failure(progress, e, false))?;
                    continue;
                }
                let mut file = match password {
                    _ if !encrypted => archive.by_index(index),
                    Some(password) => archive.by_index_decrypt(index, password.as_bytes()),
                    None => return Err(ArchiveError::NeedsPassword),
                }
                .map_err(zip_error)?;
                writer
                    .write_file(
                        &label_root.join(&entry_path),
                        &mut file,
                        size,
                        modified,
                        mode,
                        &target,
                    )
                    .map_err(|e| op_failure(progress, e, encrypted))?;
            }
        }
        Format::SevenZ => {
            special_skipped = crate::seven::extract(
                &volumes,
                &label_root,
                selection,
                dest,
                password,
                progress,
                &mut writer,
            )?;
        }
        Format::Tar | Format::TarGz => {
            let reader = tar_reader(&volumes, format, progress, false)?;
            let mut archive = tar::Archive::new(reader);
            let entries = archive
                .entries()
                .map_err(|e| io_failure(progress, e, false))?;
            for entry in entries {
                let mut entry = entry.map_err(|e| io_failure(progress, e, false))?;
                let name = entry
                    .path()
                    .map_err(|e| io_failure(progress, e, false))?
                    .to_string_lossy()
                    .into_owned();
                let Some(entry_path) = normalize_name(&name) else {
                    continue;
                };
                if !wanted(&entry_path, selection) {
                    continue;
                }
                let Some(relative) = below(&entry_path, selection.strip) else {
                    continue;
                };
                let (kind, size, modified, mode) = {
                    let header = entry.header();
                    (
                        tar_kind(header.entry_type()),
                        header.size().unwrap_or(0),
                        tar_time(header),
                        header.mode().ok().map(|m| m & 0o777),
                    )
                };
                let target = dest.join(relative);
                match kind {
                    TarKind::Special => special_skipped += 1,
                    TarKind::Dir => writer
                        .make_dir(&target, mode)
                        .map_err(|e| op_failure(progress, e, false))?,
                    TarKind::File => writer
                        .write_file(
                            &label_root.join(&entry_path),
                            &mut entry,
                            size,
                            modified,
                            mode,
                            &target,
                        )
                        .map_err(|e| op_failure(progress, e, false))?,
                }
            }
        }
    }
    Ok(ExtractReport {
        outcome: writer.finish(),
        special_skipped,
    })
}

/// Reads every file of the archive without writing anything and reports the damaged ones.
/// A wrong or missing password is an error (so the caller can ask), not a damaged entry.
pub fn verify(
    path: &Path,
    password: Option<&str>,
    progress: &Progress,
) -> Result<VerifyReport, ArchiveError> {
    let format = format_of(path)?;
    let volumes = volumes_of(path);
    let mut report = VerifyReport::default();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut drain = |reader: &mut dyn Read, progress: &Progress| -> io::Result<u64> {
        let mut total = 0;
        loop {
            if progress.is_cancelled() {
                return Err(io::Error::other("cancelled"));
            }
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                return Ok(total);
            }
            progress.add(n as u64);
            total += n as u64;
        }
    };
    match format {
        Format::Zip => {
            let mut archive = open_zip(&volumes)?;
            for index in 0..archive.len() {
                progress.check()?;
                let (name, is_dir, special, encrypted) = {
                    let file = archive.by_index_raw(index).map_err(zip_error)?;
                    (
                        file.name().map_err(zip_error)?.into_owned(),
                        file.is_dir(),
                        file.is_symlink(),
                        file.encrypted(),
                    )
                };
                if is_dir || special {
                    continue;
                }
                let opened = match password {
                    _ if !encrypted => archive.by_index(index),
                    Some(password) => archive.by_index_decrypt(index, password.as_bytes()),
                    None => return Err(ArchiveError::NeedsPassword),
                };
                let mut file = opened.map_err(zip_error)?;
                match drain(&mut file, progress) {
                    Ok(bytes) => {
                        report.files += 1;
                        report.bytes += bytes;
                    }
                    Err(e) => match io_failure(progress, e, encrypted) {
                        ArchiveError::Cancelled => return Err(ArchiveError::Cancelled),
                        ArchiveError::WrongPassword => return Err(ArchiveError::WrongPassword),
                        other => report.damaged.push(format!("{name}: {other}")),
                    },
                }
            }
        }
        Format::SevenZ => return crate::seven::verify(&volumes, password, progress),
        Format::Tar | Format::TarGz => {
            let reader = tar_reader(&volumes, format, progress, false)?;
            let mut archive = tar::Archive::new(reader);
            let outcome = (|| -> io::Result<()> {
                for entry in archive.entries()? {
                    let mut entry = entry?;
                    if !matches!(tar_kind(entry.header().entry_type()), TarKind::File) {
                        continue;
                    }
                    report.bytes += drain(&mut entry, progress)?;
                    report.files += 1;
                }
                Ok(())
            })();
            if let Err(e) = outcome {
                match io_failure(progress, e, false) {
                    ArchiveError::Cancelled => return Err(ArchiveError::Cancelled),
                    other => report.damaged.push(other.to_string()),
                }
            } else if format == Format::TarGz {
                // The gzip checksum sits after the tar data: read on to the end to check it.
                let mut rest = archive.into_inner();
                if let Err(e) = drain(&mut rest, progress) {
                    match io_failure(progress, e, false) {
                        ArchiveError::Cancelled => return Err(ArchiveError::Cancelled),
                        other => report.damaged.push(other.to_string()),
                    }
                }
            }
        }
    }
    Ok(report)
}
