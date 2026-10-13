//! Changing an existing archive: add files, remove entries, rename, make a directory.
//!
//! Every change rewrites the archive into a temporary file next to it and swaps it in only
//! when that is complete, so a failure or a cancel never damages the original. ZIP entries
//! are copied raw (still compressed and encrypted): fast, and no password is needed to
//! remove or rename. TAR, TAR.GZ and 7z have to be decoded and encoded again, which takes
//! time proportional to the whole archive.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bluepane_ops::{Conflict, Host, Progress, Resolution};
use zip::ZipArchive;

use crate::create::{collect, place, Item, Kind, Level, SourceReader, TempFile};
use crate::format::{detect, volumes_of, Format};
use crate::listing::{is_selected, normalize_name, Listing};
use crate::read::{stream_tar, zip_error};
use crate::sink::{open_sink, Existing, Meta, Sink, SinkSettings, ZipSink};
use crate::split::Joined;
use crate::ArchiveError;

#[derive(Debug, Clone)]
pub enum Change {
    /// Put these files and directories into the archive directory `into` (`""` = top level).
    Add { sources: Vec<PathBuf>, into: String },
    /// Remove these entries (files, or directories with everything below them).
    Remove { entries: Vec<String> },
    /// Give an entry (or a directory and everything below it) a new path.
    Rename { from: String, to: String },
    /// Add an empty directory.
    NewDir { path: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifyReport {
    /// Entries written for `Add` / `NewDir`.
    pub added: u64,
    /// Entries dropped by `Remove`.
    pub removed: u64,
    /// Entries given a new path by `Rename`.
    pub renamed: u64,
    /// Files not added because the user chose "skip".
    pub skipped: u64,
    /// Symlinks and the like left out of an `Add`.
    pub left_out: u64,
}

/// What a change does, worked out before anything is written.
struct Plan {
    /// Existing paths that disappear (and replaced files).
    dropped: Vec<String>,
    rename: Option<(String, String)>,
    /// Entries to append, with their final archive paths.
    additions: Vec<Item>,
    skipped: u64,
    left_out: u64,
}

impl Plan {
    /// The path an existing entry has afterwards, or `None` when it is dropped.
    fn target(&self, path: &str) -> Option<String> {
        if self.dropped.iter().any(|d| is_selected(path, d)) {
            return None;
        }
        match &self.rename {
            Some((from, to)) if is_selected(path, from) => {
                Some(format!("{to}{}", &path[from.len()..]))
            }
            _ => Some(path.to_string()),
        }
    }
}

/// Counts bytes read from an entry of the old archive and stops once the job is cancelled.
struct Counted<'a, 'b> {
    inner: &'b mut dyn Read,
    progress: &'a Progress,
}

impl Read for Counted<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.progress.is_cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.progress.add(n as u64);
        Ok(n)
    }
}

/// `dir/a.txt` becomes `dir/a (kopia).txt`, then `dir/a (kopia 2).txt`, ... until free.
fn unique_path(path: &str, taken: &HashSet<String>) -> String {
    let (parent, name) = path.rsplit_once('/').map_or(("", path), |(p, n)| (p, n));
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    for n in 1.. {
        let label = if n == 1 {
            "kopia".to_string()
        } else {
            format!("kopia {n}")
        };
        let file = format!("{stem} ({label}){ext}");
        let candidate = if parent.is_empty() {
            file
        } else {
            format!("{parent}/{file}")
        };
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("a free name always exists")
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn invalid(message: impl Into<String>) -> ArchiveError {
    ArchiveError::Op(message.into())
}

fn build_plan(
    existing: &Listing,
    change: &Change,
    own_files: &[PathBuf],
    archive_name: &Path,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<Plan, ArchiveError> {
    let mut plan = Plan {
        dropped: Vec::new(),
        rename: None,
        additions: Vec::new(),
        skipped: 0,
        left_out: 0,
    };
    match change {
        Change::Remove { entries } => {
            if entries.is_empty() {
                return Err(invalid("nic do usunięcia"));
            }
            for entry in entries {
                if existing.find(entry).is_none() {
                    return Err(invalid(format!("brak w archiwum: {entry}")));
                }
            }
            plan.dropped = entries.clone();
        }
        Change::Rename { from, to } => {
            let to = normalize_name(to).ok_or_else(|| invalid("niepoprawna nowa nazwa"))?;
            if existing.find(from).is_none() {
                return Err(invalid(format!("brak w archiwum: {from}")));
            }
            if existing.find(&to).is_some() {
                return Err(invalid(format!("w archiwum już jest: {to}")));
            }
            if is_selected(&to, from) {
                return Err(invalid("nie można przenieść katalogu do jego wnętrza"));
            }
            plan.rename = Some((from.clone(), to));
        }
        Change::NewDir { path } => {
            let path = normalize_name(path).ok_or_else(|| invalid("niepoprawna nazwa katalogu"))?;
            if existing.find(&path).is_some() {
                return Err(invalid(format!("w archiwum już jest: {path}")));
            }
            plan.additions.push(Item {
                path: PathBuf::new(),
                name: path,
                kind: Kind::Dir,
                modified: Some(SystemTime::now()),
                mode: Some(0o755),
            });
        }
        Change::Add { sources, into } => {
            let into = if into.is_empty() {
                String::new()
            } else {
                normalize_name(into).ok_or_else(|| invalid("niepoprawny katalog w archiwum"))?
            };
            if existing.find(&into).is_some_and(|e| !e.is_dir) {
                return Err(invalid(format!("to plik, nie katalog: {into}")));
            }
            let (items, left_out) = collect(sources, own_files, progress)?;
            plan.left_out = left_out;
            let mut taken: HashSet<String> =
                existing.entries.iter().map(|e| e.path.clone()).collect();
            for mut item in items {
                let name = join(&into, &item.name);
                match item.kind {
                    Kind::Dir => {
                        if existing.find(&name).is_some_and(|e| !e.is_dir) {
                            return Err(invalid(format!("w archiwum jest plik o nazwie: {name}")));
                        }
                        // A directory that is there already is merged into.
                        if taken.insert(name.clone()) {
                            item.name = name;
                            plan.additions.push(item);
                        }
                    }
                    Kind::File { size } => {
                        let mut name = name;
                        if let Some(old) = existing.find(&name) {
                            if old.is_dir {
                                return Err(invalid(format!(
                                    "w archiwum jest katalog o nazwie: {name}"
                                )));
                            }
                            let conflict = Conflict {
                                src: item.path.clone(),
                                dst: archive_name.join(&name),
                                src_size: size,
                                dst_size: old.size,
                                src_modified: item.modified,
                                dst_modified: old.modified,
                            };
                            let resolution = match host.conflict(&conflict)? {
                                Resolution::OverwriteOlder => {
                                    match (conflict.src_modified, conflict.dst_modified) {
                                        (Some(new), Some(old)) if new > old => {
                                            Resolution::Overwrite
                                        }
                                        _ => Resolution::Skip,
                                    }
                                }
                                other => other,
                            };
                            match resolution {
                                Resolution::Skip | Resolution::OverwriteOlder => {
                                    plan.skipped += 1;
                                    continue;
                                }
                                Resolution::Overwrite => plan.dropped.push(name.clone()),
                                Resolution::KeepBoth => name = unique_path(&name, &taken),
                            }
                        } else if taken.contains(&name) {
                            // Two sources with the same name: keep both.
                            name = unique_path(&name, &taken);
                        }
                        taken.insert(name.clone());
                        item.mode = host.file_mode(&item.path);
                        item.name = name;
                        plan.additions.push(item);
                    }
                }
            }
        }
    }
    Ok(plan)
}

/// Checks that `password` really opens the archive's encrypted content, before it is used to
/// encrypt anything new (a wrong one would leave entries nobody can read together).
fn verify_password(
    format: Format,
    volumes: &[PathBuf],
    password: &str,
    progress: &Progress,
) -> Result<(), ArchiveError> {
    match format {
        Format::SevenZ => crate::seven::check_password(volumes, password, progress),
        Format::Zip => {
            let mut archive = ZipArchive::new(Joined::open(volumes)?).map_err(zip_error)?;
            for index in 0..archive.len() {
                let encrypted = {
                    let file = archive.by_index_raw(index).map_err(zip_error)?;
                    file.encrypted() && !file.is_dir()
                };
                if encrypted {
                    archive
                        .by_index_decrypt(index, password.as_bytes())
                        .map_err(zip_error)?;
                    break;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn add_new(sink: &mut dyn Sink, items: &[Item], progress: &Progress) -> Result<(), ArchiveError> {
    for item in items {
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
    Ok(())
}

fn rewrite_zip(
    volumes: &[PathBuf],
    plan: &Plan,
    temp: &Path,
    settings: &SinkSettings<'_>,
    progress: &Progress,
) -> Result<(), ArchiveError> {
    let mut old = ZipArchive::new(Joined::open(volumes)?).map_err(zip_error)?;
    let mut sink = ZipSink::open(temp, settings, progress)?;
    for index in 0..old.len() {
        progress.check()?;
        let file = old.by_index_raw(index).map_err(zip_error)?;
        let stored = file.name().map_err(zip_error)?.into_owned();
        let is_dir = file.is_dir();
        let size = if is_dir { 0 } else { file.size() };
        let target = match normalize_name(&stored) {
            Some(path) => plan
                .target(&path)
                .map(|n| if is_dir { format!("{n}/") } else { n }),
            // An entry with an unsafe name is carried over untouched.
            None => Some(stored),
        };
        let Some(target) = target else { continue };
        progress.set_current(&target);
        sink.raw_copy(file, &target)?;
        progress.add(size);
        progress.finish_item();
    }
    add_new(&mut sink, &plan.additions, progress)?;
    sink.finish_zip()
}

fn rewrite_streaming(
    format: Format,
    volumes: &[PathBuf],
    plan: &Plan,
    temp: &Path,
    settings: &SinkSettings<'_>,
    read_password: Option<&str>,
    progress: &Progress,
) -> Result<(), ArchiveError> {
    let mut sink = open_sink(temp, settings, progress)?;
    {
        let mut each = |entry: &Existing, data: &mut dyn Read| -> Result<(), ArchiveError> {
            progress.check()?;
            if entry.special {
                // Checked up front from the listing; this guards a listing that is stale.
                return Err(ArchiveError::Unsupported(
                    "wpis specjalny (dowiązanie) nie przetrwałby przepisania".into(),
                ));
            }
            let target = match &entry.path {
                Some(path) => plan.target(path),
                None => Some(entry.name.clone()),
            };
            let Some(target) = target else {
                return Ok(());
            };
            progress.set_current(&target);
            if entry.is_dir {
                sink.add_dir(&target, &entry.meta)?;
            } else {
                let mut counted = Counted {
                    inner: data,
                    progress,
                };
                sink.add_file(&target, entry.size, &entry.meta, &mut counted)?;
            }
            progress.finish_item();
            Ok(())
        };
        match format {
            Format::SevenZ => {
                crate::seven::stream_entries(volumes, read_password, progress, &mut each)?
            }
            _ => stream_tar(volumes, format, progress, &mut each)?,
        }
    }
    add_new(&mut *sink, &plan.additions, progress)?;
    sink.finish()
}

/// Applies `change` to the archive at `path`. `existing` is its table of contents as the
/// user sees it; if the archive on disk has changed since, the change is refused.
///
/// The archive stays in its format and, for a split archive, in volumes of the same size.
/// New entries are encrypted with `password` when the archive already holds encrypted
/// entries (a 7z archive with hidden names keeps them hidden).
pub fn modify(
    path: &Path,
    existing: &Listing,
    change: &Change,
    password: Option<&str>,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<ModifyReport, ArchiveError> {
    let format = detect(path).ok_or_else(|| ArchiveError::Unsupported("nieznany format".into()))?;
    let volumes = volumes_of(path);
    let on_disk: u64 = volumes
        .iter()
        .filter_map(|v| fs::metadata(v).ok())
        .map(|m| m.len())
        .sum();
    if on_disk != existing.packed_size {
        return Err(invalid(
            "archiwum zmieniło się od otwarcia: otwórz je ponownie",
        ));
    }
    let password = password.filter(|p| !p.is_empty());
    let encrypted = existing.has_encrypted();
    // A ZIP is copied raw, so removing or renaming never touches the encryption.
    let needs_password =
        encrypted && !(format == Format::Zip && !matches!(change, Change::Add { .. }));
    if needs_password {
        match password {
            None => return Err(ArchiveError::NeedsPassword),
            Some(password) => verify_password(format, &volumes, password, progress)?,
        }
    }
    // Rewriting TAR and 7z decodes and encodes every entry: links and devices would be lost.
    if format != Format::Zip && existing.entries.iter().any(|e| e.special) {
        return Err(ArchiveError::Unsupported(
            "archiwum zawiera dowiązania lub pliki specjalne, których przepisanie by nie zachowało"
                .into(),
        ));
    }

    let first = volumes[0].clone();
    let is_split = first
        .file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".001"));
    let output = if is_split {
        let name = first.file_name().unwrap_or_default().to_string_lossy();
        first.with_file_name(name.trim_end_matches(".001"))
    } else {
        first.clone()
    };
    let temp_path = {
        let name = output
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        output.with_file_name(format!(".{name}.bluepane-tmp"))
    };
    let mut own_files = volumes.clone();
    own_files.push(temp_path.clone());
    let archive_name = PathBuf::from(output.file_name().unwrap_or_default());

    let plan = build_plan(existing, change, &own_files, &archive_name, progress, host)?;
    let (kept, dropped): (Vec<_>, Vec<_>) = existing
        .entries
        .iter()
        .partition(|e| plan.target(&e.path).is_some());
    let new_bytes: u64 = plan
        .additions
        .iter()
        .map(|i| match i.kind {
            Kind::File { size } => size,
            Kind::Dir => 0,
        })
        .sum();
    let kept_bytes: u64 = kept.iter().map(|e| e.size).sum();
    progress.set_total(
        kept_bytes + new_bytes,
        (kept.len() + plan.additions.len()) as u64,
    );

    let settings = SinkSettings {
        format,
        level: Level::Normal,
        password: if encrypted { password } else { None },
        encrypt_names: existing.names_encrypted,
    };
    let mut temp = TempFile {
        path: temp_path,
        keep: false,
    };
    match format {
        Format::Zip => rewrite_zip(&volumes, &plan, &temp.path, &settings, progress)?,
        _ => rewrite_streaming(
            format,
            &volumes,
            &plan,
            &temp.path,
            &settings,
            if encrypted { password } else { None },
            progress,
        )?,
    }
    progress.check()?;

    let split = is_split.then(|| {
        let first_len = fs::metadata(&first).map_or(0, |m| m.len());
        // With several volumes the first one is a full part; a lone `.001` gets some room.
        if volumes.len() > 1 {
            first_len.max(1)
        } else {
            first_len.max(1 << 20)
        }
    });
    place(&mut temp, &output, split, true, progress, host)?;

    let renamed = match &plan.rename {
        Some((from, _)) => existing
            .entries
            .iter()
            .filter(|e| is_selected(&e.path, from))
            .count() as u64,
        None => 0,
    };
    let removed = if matches!(change, Change::Remove { .. }) {
        dropped.len() as u64
    } else {
        0
    };
    Ok(ModifyReport {
        added: plan.additions.len() as u64,
        removed,
        renamed,
        skipped: plan.skipped,
        left_out: plan.left_out,
    })
}
