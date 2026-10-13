//! Creating archives.
//!
//! The archive is first built in a temporary file next to the final one and only then put in
//! place (renamed, or cut into volumes), so a failure or a cancel never leaves a half-written
//! archive and never damages an existing one. Splitting therefore needs room for the archive
//! twice while it runs.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::format::Format;
use crate::sink::{io_failure, open_sink, Meta, SinkSettings};
use crate::split::SplitWriter;
use crate::ArchiveError;
use bluepane_ops::{Conflict, Host, Progress, Resolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// No compression.
    Store,
    Fast,
    Normal,
    Maximum,
}

impl Level {
    pub(crate) fn deflate_level(self) -> i64 {
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
    /// 7z only: hide the file names too (needs a password).
    pub encrypt_names: bool,
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

pub(crate) enum Kind {
    Dir,
    File { size: u64 },
}

pub(crate) struct Item {
    pub path: PathBuf,
    /// Name inside the archive, `/` separated.
    pub name: String,
    pub kind: Kind,
    pub modified: Option<SystemTime>,
    pub mode: Option<u32>,
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
pub(crate) struct SourceReader<'a> {
    pub file: File,
    pub progress: &'a Progress,
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
    if options.encrypt_names && (password.is_none() || !options.format.can_encrypt_names()) {
        return Err(ArchiveError::Unsupported(
            "szyfrowanie nazw plików wymaga formatu 7z i hasła".into(),
        ));
    }
    let settings = SinkSettings {
        format: options.format,
        level: options.level,
        password,
        encrypt_names: options.encrypt_names,
    };
    let mut sink = open_sink(&temp.path, &settings, progress)?;
    for item in &items {
        progress.check()?;
        progress.set_current(&item.name);
        let meta = Meta {
            modified: item.modified,
            mode: item.mode,
        };
        match item.kind {
            Kind::Dir => sink.add_dir(&item.name, &meta)?,
            Kind::File { size } => {
                let mut reader = SourceReader {
                    file: File::open(&item.path)?,
                    progress,
                };
                sink.add_file(&item.name, size, &meta, &mut reader)?;
            }
        }
        progress.finish_item();
    }
    sink.finish()?;
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
