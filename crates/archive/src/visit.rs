//! Reading every file of an archive in turn, without extracting it to disk (used by search).

use std::io::{self, Read};
use std::path::Path;
use std::time::SystemTime;

use bluepane_ops::Progress;

use crate::format::{volumes_of, Format};
use crate::listing::normalize_name;
use crate::read::{format_of, open_zip, stream_tar, zip_error};
use crate::time::from_zip;
use crate::ArchiveError;

/// One file inside an archive, as handed to the visitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisitedEntry {
    /// `/` separated path inside the archive.
    pub path: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// False when the content is encrypted and the password is missing or wrong: the
    /// reader given alongside is empty then, but the name is still real.
    pub readable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VisitReport {
    /// Files whose content could not be read for lack of a (correct) password.
    pub unreadable: u64,
}

/// Calls `each` for every regular file with its data. Directories, links, devices and
/// entries with unsafe names are left out. The reader is only valid during the call.
///
/// A 7z archive that needs a password and gets none or a wrong one is an error
/// (`NeedsPassword` / `WrongPassword`): it is solid, so there is no way to see the names
/// of the rest. For ZIP such files are still visited with `readable = false`.
pub fn visit(
    path: &Path,
    password: Option<&str>,
    progress: &Progress,
    each: &mut dyn FnMut(&VisitedEntry, &mut dyn Read) -> Result<(), ArchiveError>,
) -> Result<VisitReport, ArchiveError> {
    let format = format_of(path)?;
    let volumes = volumes_of(path);
    let mut report = VisitReport::default();
    match format {
        Format::Zip => {
            let mut archive = open_zip(&volumes)?;
            for index in 0..archive.len() {
                progress.check()?;
                let (name, size, modified, encrypted) = {
                    let file = archive.by_index_raw(index).map_err(zip_error)?;
                    if file.is_dir() || file.is_symlink() {
                        continue;
                    }
                    (
                        file.name().map_err(zip_error)?.into_owned(),
                        file.size(),
                        file.last_modified().and_then(from_zip),
                        file.encrypted(),
                    )
                };
                let Some(entry_path) = normalize_name(&name) else {
                    continue;
                };
                let opened = match (encrypted, password) {
                    (false, _) => Some(archive.by_index(index)),
                    (true, Some(password)) => {
                        Some(archive.by_index_decrypt(index, password.as_bytes()))
                    }
                    (true, None) => None,
                };
                match opened {
                    Some(Ok(mut file)) => {
                        let entry = VisitedEntry {
                            path: entry_path,
                            size,
                            modified,
                            readable: true,
                        };
                        each(&entry, &mut file)?;
                    }
                    Some(Err(zip::result::ZipError::InvalidPassword)) | None => {
                        report.unreadable += 1;
                        let entry = VisitedEntry {
                            path: entry_path,
                            size,
                            modified,
                            readable: false,
                        };
                        each(&entry, &mut io::empty())?;
                    }
                    Some(Err(other)) => return Err(zip_error(other)),
                }
            }
        }
        Format::SevenZ => {
            crate::seven::stream_entries(&volumes, password, progress, &mut |existing, data| {
                progress.check()?;
                let Some(path) = existing.path.clone() else {
                    return Ok(());
                };
                if existing.is_dir || existing.special {
                    return Ok(());
                }
                let entry = VisitedEntry {
                    path,
                    size: existing.size,
                    modified: existing.meta.modified,
                    readable: true,
                };
                each(&entry, data)
            })?;
        }
        Format::Tar | Format::TarGz => {
            stream_tar(&volumes, format, progress, &mut |existing, data| {
                progress.check()?;
                let Some(path) = existing.path.clone() else {
                    return Ok(());
                };
                if existing.is_dir || existing.special {
                    return Ok(());
                }
                let entry = VisitedEntry {
                    path,
                    size: existing.size,
                    modified: existing.meta.modified,
                    readable: true,
                };
                each(&entry, data)
            })?;
        }
    }
    Ok(report)
}
