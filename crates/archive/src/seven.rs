//! The 7z format: listing, extracting, testing and writing.
//!
//! 7z archives are usually *solid*: files are compressed together in blocks, so reading one
//! file means decoding what precedes it in its block. That is inherent to the format (7-Zip
//! has the same cost). Passwords use AES-256 and may also hide the file names.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bluepane_ops::{Host, Progress, Writer};
use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
use sevenz_rust2::{
    Archive, ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderMethod, Error, NtTime, Password,
};

use crate::create::{CreateOptions, Item, Kind, Level, SourceReader};
use crate::listing::{normalize_name, ArchiveEntry as Entry};
use crate::read::{below, op_failure, wanted, Selection, VerifyReport};
use crate::split::Joined;
use crate::ArchiveError;

/// 7-Zip's marker that the high 16 bits of the attributes hold a Unix `st_mode`.
const UNIX_EXTENSION: u32 = 0x8000;
const ATTRIBUTE_DIRECTORY: u32 = 0x10;
const ATTRIBUTE_ARCHIVE: u32 = 0x20;
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;

fn password_of(password: Option<&str>) -> Password {
    password.map_or_else(Password::empty, Password::new)
}

/// Maps a 7z library error. `had_password` tells whether a password was supplied, which
/// decides whether a checksum failure means "wrong password" or "damaged".
fn failure(progress: &Progress, error: Error, had_password: bool) -> ArchiveError {
    if progress.is_cancelled() {
        return ArchiveError::Cancelled;
    }
    match error {
        Error::PasswordRequired => ArchiveError::NeedsPassword,
        Error::MaybeBadPassword(_) => ArchiveError::WrongPassword,
        Error::ChecksumVerificationFailed if had_password => ArchiveError::WrongPassword,
        Error::BadSignature(_) => ArchiveError::Corrupt("to nie jest archiwum 7z".into()),
        Error::UnsupportedCompressionMethod(method) => {
            ArchiveError::Unsupported(format!("metoda kompresji 7z: {method}"))
        }
        Error::MaxMemLimited { max_kb, actaul_kb } => ArchiveError::Unsupported(format!(
            "archiwum 7z wymaga {} MB pamięci (limit {} MB)",
            actaul_kb / 1024,
            max_kb / 1024
        )),
        Error::Io(e, _) | Error::FileOpen(e, _) => ArchiveError::Io(e),
        other => ArchiveError::Corrupt(other.to_string()),
    }
}

/// Unix permission bits and "is a symlink" from an entry's attributes.
fn unix_attributes(entry: &ArchiveEntry) -> (Option<u32>, bool) {
    let attributes = entry.windows_attributes();
    if !entry.has_windows_attributes || attributes & UNIX_EXTENSION == 0 {
        return (None, false);
    }
    let unix = attributes >> 16;
    (Some(unix & 0o777), unix & S_IFMT == S_IFLNK)
}

fn modified_of(entry: &ArchiveEntry) -> Option<SystemTime> {
    entry
        .has_last_modified_date
        .then(|| SystemTime::from(entry.last_modified_date()))
}

/// For each file entry: whether its data sits in an AES-encrypted block.
fn encrypted_files(archive: &Archive) -> Vec<bool> {
    (0..archive.files.len())
        .map(|index| {
            archive
                .stream_map
                .file_block_index
                .get(index)
                .copied()
                .flatten()
                .is_some_and(|block| {
                    archive.blocks[block]
                        .coders
                        .iter()
                        .any(|c| c.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
                })
        })
        .collect()
}

/// Checks the password by fully reading one small encrypted file (with its checksum).
///
/// A 7z password has no check value, and a wrong key makes the LZMA2 decoder produce noise
/// that can look valid for a while, so the failure would otherwise only show part-way into
/// extraction, after directories and partial files exist. The probe picks the smallest
/// encrypted file that starts its block (cheap even in a solid archive) and reads it first.
fn probe_password(
    reader: &mut ArchiveReader<Joined>,
    progress: &Progress,
) -> Result<(), ArchiveError> {
    let archive = reader.archive();
    let encrypted = encrypted_files(archive);
    let probe = archive
        .files
        .iter()
        .enumerate()
        .filter(|(index, entry)| {
            encrypted.get(*index).copied().unwrap_or(false)
                && entry.has_stream
                && entry.size > 0
                && archive.stream_map.file_block_index[*index]
                    .is_some_and(|block| archive.stream_map.block_first_file_index[block] == *index)
        })
        .min_by_key(|(_, entry)| entry.size)
        .map(|(_, entry)| (entry.name().to_string(), entry.size));
    let Some((name, size)) = probe else {
        return Ok(());
    };
    let data = reader
        .read_file(&name)
        .map_err(|e| match failure(progress, e, true) {
            keep @ (ArchiveError::Cancelled | ArchiveError::Unsupported(_)) => keep,
            _ => ArchiveError::WrongPassword,
        })?;
    // The library checks the checksum only when it read exactly the declared size: noise
    // that ends early passes silently, so compare the length ourselves.
    if data.len() as u64 != size {
        return Err(ArchiveError::WrongPassword);
    }
    Ok(())
}

/// Reads the table of contents. With encrypted file names this needs the password.
pub fn list(
    volumes: &[PathBuf],
    password: Option<&str>,
    progress: &Progress,
) -> Result<(Vec<Entry>, usize), ArchiveError> {
    let mut joined = Joined::open(volumes)?;
    let archive = Archive::read(&mut joined, &password_of(password)).map_err(|e| {
        let mapped = failure(progress, e, password.is_some());
        // An encrypted header decrypted with a wrong password is just noise, which fails in
        // many ways: with a password given, an unreadable header means the password.
        match mapped {
            ArchiveError::Corrupt(_) if password.is_some() => ArchiveError::WrongPassword,
            other => other,
        }
    })?;
    let encrypted = encrypted_files(&archive);
    let mut raw = Vec::new();
    let mut unsafe_skipped = 0;
    for (index, entry) in archive.files.iter().enumerate() {
        if entry.is_anti_item() {
            continue;
        }
        let Some(path) = normalize_name(entry.name()) else {
            unsafe_skipped += 1;
            continue;
        };
        let (mode, special) = unix_attributes(entry);
        let is_dir = entry.is_directory();
        raw.push(Entry {
            path,
            is_dir,
            size: if is_dir { 0 } else { entry.size() },
            modified: modified_of(entry),
            encrypted: encrypted.get(index).copied().unwrap_or(false),
            mode,
            special,
        });
    }
    Ok((raw, unsafe_skipped))
}

fn drain(data: &mut dyn Read) -> io::Result<u64> {
    io::copy(data, &mut io::sink())
}

/// Extracts the selection. Returns how many links and devices were left out.
pub fn extract(
    volumes: &[PathBuf],
    archive_name: &Path,
    selection: &Selection<'_>,
    dest: &Path,
    password: Option<&str>,
    progress: &Progress,
    writer: &mut Writer<'_>,
) -> Result<u64, ArchiveError> {
    let had_password = password.is_some();
    let mut reader = ArchiveReader::new(Joined::open(volumes)?, password_of(password))
        .map_err(|e| failure(progress, e, had_password))?;

    // Settle a missing password before anything is written (as for ZIP).
    if password.is_none() {
        let encrypted = encrypted_files(reader.archive());
        let needed = reader
            .archive()
            .files
            .iter()
            .enumerate()
            .any(|(index, entry)| {
                encrypted.get(index).copied().unwrap_or(false)
                    && !entry.is_directory()
                    && normalize_name(entry.name()).is_some_and(|path| {
                        wanted(&path, selection) && below(&path, selection.strip).is_some()
                    })
            });
        if needed {
            return Err(ArchiveError::NeedsPassword);
        }
    }

    if had_password {
        probe_password(&mut reader, progress)?;
    }

    let encrypted_names: std::collections::HashSet<String> = {
        let flags = encrypted_files(reader.archive());
        reader
            .archive()
            .files
            .iter()
            .zip(flags)
            .filter(|(_, encrypted)| *encrypted)
            .map(|(entry, _)| entry.name().to_string())
            .collect()
    };
    let mut wrote_encrypted = false;
    let mut special_skipped = 0u64;
    let mut stopped: Option<ArchiveError> = None;
    let result = reader.for_each_entries(|entry, data| {
        // Every entry must be read to the end (or drained), or the next one would start in
        // the middle of this one's data.
        let skip = |data: &mut dyn Read| -> Result<bool, Error> {
            drain(data).map_err(|e| Error::Io(e, "".into()))?;
            Ok(true)
        };
        if entry.is_anti_item() {
            return skip(data);
        }
        let Some(path) = normalize_name(entry.name()) else {
            return skip(data);
        };
        if !wanted(&path, selection) {
            return skip(data);
        }
        let Some(relative) = below(&path, selection.strip) else {
            return skip(data);
        };
        let (mode, special) = unix_attributes(entry);
        if special {
            special_skipped += 1;
            return skip(data);
        }
        let target = dest.join(relative);
        let written = if entry.is_directory() {
            writer.make_dir(&target, mode)
        } else {
            writer.write_file(
                &archive_name.join(&path),
                data,
                entry.size(),
                modified_of(entry),
                mode,
                &target,
            )
        };
        let is_encrypted = encrypted_names.contains(entry.name());
        match written {
            Ok(()) => {
                wrote_encrypted |= is_encrypted && !entry.is_directory();
                Ok(true)
            }
            Err(error) => {
                // A wrong 7z password only shows as garbage while decoding: a failure on the
                // first encrypted file, with a password given, means the password.
                stopped = Some(
                    if is_encrypted
                        && had_password
                        && !wrote_encrypted
                        && matches!(error, bluepane_ops::OpError::Io(_))
                    {
                        ArchiveError::WrongPassword
                    } else {
                        op_failure(progress, error, had_password)
                    },
                );
                Err(Error::Other("stopped".into()))
            }
        }
    });
    if let Some(error) = stopped {
        return Err(error);
    }
    result.map_err(|e| {
        let mapped = failure(progress, e, had_password);
        match mapped {
            ArchiveError::Cancelled
            | ArchiveError::NeedsPassword
            | ArchiveError::Unsupported(_) => mapped,
            // The decoder itself failed before the first encrypted file came out: noise.
            _ if had_password && !encrypted_names.is_empty() && !wrote_encrypted => {
                ArchiveError::WrongPassword
            }
            other => other,
        }
    })?;
    Ok(special_skipped)
}

/// Reads every file without writing anything; reports the first damaged one (a solid block
/// cannot be decoded past damage).
pub fn verify(
    volumes: &[PathBuf],
    password: Option<&str>,
    progress: &Progress,
) -> Result<VerifyReport, ArchiveError> {
    let had_password = password.is_some();
    let mut reader = ArchiveReader::new(Joined::open(volumes)?, password_of(password))
        .map_err(|e| failure(progress, e, had_password))?;
    let any_encrypted = encrypted_files(reader.archive()).iter().any(|&e| e);
    if password.is_none() && any_encrypted {
        return Err(ArchiveError::NeedsPassword);
    }
    if had_password {
        probe_password(&mut reader, progress)?;
    }
    let mut report = VerifyReport::default();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut damaged: Option<String> = None;
    let result = reader.for_each_entries(|entry, data| {
        if entry.is_directory() || entry.is_anti_item() {
            return Ok(true);
        }
        let mut bytes = 0u64;
        loop {
            if progress.is_cancelled() {
                return Err(Error::Other("cancelled".into()));
            }
            match data.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    progress.add(n as u64);
                    bytes += n as u64;
                }
                Err(e) => {
                    damaged = Some(format!("{}: {e}", entry.name()));
                    return Err(Error::Io(e, "".into()));
                }
            }
        }
        report.files += 1;
        report.bytes += bytes;
        Ok(true)
    });
    match result {
        Ok(()) => Ok(report),
        Err(error) => match failure(progress, error, had_password) {
            ArchiveError::Cancelled => Err(ArchiveError::Cancelled),
            ArchiveError::NeedsPassword => Err(ArchiveError::NeedsPassword),
            // A 7z password has no check value: wrong data and a wrong password look the
            // same. Failing on the very first file with a password given means the password.
            _ if had_password && any_encrypted && report.files == 0 => {
                Err(ArchiveError::WrongPassword)
            }
            ArchiveError::WrongPassword if damaged.is_none() => Err(ArchiveError::WrongPassword),
            other => {
                report
                    .damaged
                    .push(damaged.unwrap_or_else(|| other.to_string()));
                Ok(report)
            }
        },
    }
}

fn attributes_for(item: &Item) -> (bool, u32) {
    let is_dir = matches!(item.kind, Kind::Dir);
    let base = if is_dir {
        ATTRIBUTE_DIRECTORY
    } else {
        ATTRIBUTE_ARCHIVE
    };
    match (item.mode, is_dir) {
        (Some(mode), _) => {
            let file_type = if is_dir { S_IFDIR } else { S_IFREG };
            (
                true,
                base | UNIX_EXTENSION | ((file_type | (mode & 0o777)) << 16),
            )
        }
        (None, true) => (true, base | UNIX_EXTENSION | ((S_IFDIR | 0o755) << 16)),
        (None, false) => (true, base),
    }
}

/// Builds the archive in `temp`.
pub fn write(
    items: &[Item],
    temp: &Path,
    options: &CreateOptions,
    password: Option<&str>,
    progress: &Progress,
    _host: &mut dyn Host,
) -> Result<(), ArchiveError> {
    let map = |e: Error| failure(progress, e, false);
    let mut writer = ArchiveWriter::new(File::create(temp)?).map_err(map)?;
    let compression = match options.level {
        Level::Store => EncoderMethod::COPY.into(),
        Level::Fast => Lzma2Options::from_level(1).into(),
        Level::Normal => Lzma2Options::from_level(5).into(),
        Level::Maximum => Lzma2Options::from_level(9).into(),
    };
    let mut methods = Vec::new();
    if let Some(password) = password {
        methods.push(AesEncoderOptions::new(Password::new(password)).into());
    }
    methods.push(compression);
    writer.set_content_methods(methods);
    // Hiding the names only makes sense (and only works) with a password.
    writer.set_encrypt_header(password.is_some() && options.encrypt_names);

    for item in items {
        progress.check()?;
        progress.set_current(&item.name);
        let mut entry = match item.kind {
            Kind::Dir => ArchiveEntry::new_directory(&item.name),
            Kind::File { .. } => ArchiveEntry::new_file(&item.name),
        };
        if let Some(stamp) = item.modified.and_then(|m| NtTime::try_from(m).ok()) {
            entry.last_modified_date = stamp;
            entry.has_last_modified_date = true;
        }
        let (has_attributes, attributes) = attributes_for(item);
        entry.has_windows_attributes = has_attributes;
        entry.windows_attributes = attributes;
        match item.kind {
            Kind::Dir => {
                writer
                    .push_archive_entry(entry, None::<io::Empty>)
                    .map_err(map)?;
            }
            Kind::File { .. } => {
                let source = SourceReader {
                    file: File::open(&item.path)?,
                    progress,
                };
                writer
                    .push_archive_entry(entry, Some(source))
                    .map_err(map)?;
            }
        }
        progress.finish_item();
    }
    writer.finish()?;
    Ok(())
}
