//! The worker side of SFTP jobs: connecting, listing, transfers with conflict questions,
//! making folders, renaming and deleting on the server.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bluepane_ops::{Conflict, Host, OpError, Progress, Resolution};
use bluepane_sftp::{Connection, SftpError, TransferOptions};

use crate::job::{Event, HostPrompt, JobSpec, RemoteOp, Report, WorkerHost};

/// `dir` + `name` as a server path.
pub(crate) fn remote_join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The parent folder of a server path (`/` stays `/`).
pub(crate) fn remote_parent(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(at) => trimmed[..at].to_string(),
    }
}

pub(crate) fn remote_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

pub(crate) fn run_remote(spec: &JobSpec, progress: &Progress, tx: &Sender<Event>) -> Report {
    let mut report = Report::default();
    let Some(job) = spec.archive.as_ref().and_then(|a| a.remote.as_ref()) else {
        report.failed.push("brak ustawień zadania SFTP".into());
        return report;
    };
    let mut host = WorkerHost {
        tx: tx.clone(),
        progress: progress.clone(),
        sticky: None,
    };
    if let RemoteOp::Connect {
        site,
        secret,
        trust,
        known,
    } = &job.op
    {
        let secret = match secret {
            Some(secret) => Some(secret.clone()),
            None => match site.stored_secret() {
                Ok(secret) => secret,
                Err(error) => {
                    report.failed.push(error.to_string());
                    return report;
                }
            },
        };
        match Connection::connect(&site.params(secret.clone()), known, trust.as_deref()) {
            Ok(conn) => {
                let start = if site.start_dir.is_empty() {
                    conn.home()
                } else {
                    conn.canonical(&site.start_dir)
                };
                let listing = start.and_then(|path| conn.list(&path).map(|list| (path, list)));
                match listing {
                    Ok(listing) => {
                        report.remote_listing = Some(listing);
                        report.connected = Some(Arc::new(conn));
                        report.done = 1;
                    }
                    Err(error) => report.failed.push(error.to_string()),
                }
            }
            Err(SftpError::UnknownHost {
                host,
                port,
                algorithm,
                fingerprint,
            }) => {
                report.host_prompt = Some(HostPrompt {
                    site: site.clone(),
                    secret,
                    host,
                    port,
                    algorithm,
                    fingerprint,
                });
            }
            Err(error) => report.failed.push(error.to_string()),
        }
        return report;
    }

    let Some(conn) = job.conn.clone() else {
        report.failed.push("brak połączenia z serwerem".into());
        return report;
    };
    let result = match &job.op {
        RemoteOp::Connect { .. } => unreachable!("handled above"),
        RemoteOp::List { path } => conn
            .canonical(path)
            .and_then(|path| conn.list(&path).map(|list| (path, list)))
            .map(|listing| {
                report.remote_listing = Some(listing);
                report.done = 1;
            }),
        RemoteOp::Mkdir { path } => conn.mkdir(path).map(|()| report.done = 1),
        RemoteOp::Rename { from, to } => conn.rename(from, to).map(|()| report.done = 1),
        RemoteOp::Delete { paths } => {
            progress.set_total(0, paths.len() as u64);
            delete_all(&conn, paths, progress, &mut report)
        }
        RemoteOp::Download {
            sources,
            dest,
            remove_source,
            ..
        } => download_all(
            &conn,
            sources,
            dest,
            *remove_source,
            progress,
            &mut host,
            &mut report,
        ),
        RemoteOp::Upload {
            sources,
            dest,
            remove_source,
        } => upload_all(
            &conn,
            sources,
            dest,
            *remove_source,
            progress,
            &mut host,
            &mut report,
        ),
        RemoteOp::Relay {
            target,
            sources,
            dest,
            remove_source,
        } => relay_all(
            &conn,
            target,
            sources,
            dest,
            *remove_source,
            progress,
            &mut host,
            &mut report,
        ),
        RemoteOp::MoveWithin { sources, dest } => {
            move_within(&conn, sources, dest, progress, &mut report)
        }
    };
    match result {
        Ok(()) => {}
        Err(SftpError::Cancelled) => report.cancelled = true,
        Err(error) => report.failed.push(error.to_string()),
    }
    report
}

fn cancelled(progress: &Progress) -> Result<(), SftpError> {
    if progress.is_cancelled() {
        Err(SftpError::Cancelled)
    } else {
        Ok(())
    }
}

fn op_error(error: OpError) -> SftpError {
    match error {
        OpError::Cancelled => SftpError::Cancelled,
        other => SftpError::Remote(other.to_string()),
    }
}

// ---- delete ------------------------------------------------------------------------

fn delete_all(
    conn: &Connection,
    paths: &[String],
    progress: &Progress,
    report: &mut Report,
) -> Result<(), SftpError> {
    for path in paths {
        cancelled(progress)?;
        progress.set_current(remote_name(path));
        match delete_tree(conn, path, progress) {
            Ok(()) => report.done += 1,
            Err(SftpError::Cancelled) => return Err(SftpError::Cancelled),
            Err(error) => report
                .failed
                .push(format!("{}: {error}", remote_name(path))),
        }
        progress.finish_item();
    }
    Ok(())
}

fn delete_tree(conn: &Connection, path: &str, progress: &Progress) -> Result<(), SftpError> {
    cancelled(progress)?;
    let Some(entry) = conn.stat(path)? else {
        return Ok(());
    };
    if entry.is_dir {
        for child in conn.list(path)? {
            delete_tree(conn, &remote_join(path, &child.name), progress)?;
        }
        conn.remove_dir(path)
    } else {
        conn.remove_file(path)
    }
}

// ---- download ------------------------------------------------------------------------

fn measure_remote(
    conn: &Connection,
    path: &str,
    files: &mut u64,
    bytes: &mut u64,
) -> Result<(), SftpError> {
    let Some(entry) = conn.stat(path)? else {
        return Ok(());
    };
    if entry.is_dir {
        for child in conn.list(path)? {
            measure_remote(conn, &remote_join(path, &child.name), files, bytes)?;
        }
    } else {
        *files += 1;
        *bytes += entry.size;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn download_all(
    conn: &Connection,
    sources: &[String],
    dest: &Path,
    remove_source: bool,
    progress: &Progress,
    host: &mut WorkerHost,
    report: &mut Report,
) -> Result<(), SftpError> {
    let (mut files, mut bytes) = (0, 0);
    for source in sources {
        measure_remote(conn, source, &mut files, &mut bytes)?;
    }
    progress.set_total(bytes, files);
    fs::create_dir_all(dest)?;
    for source in sources {
        cancelled(progress)?;
        let name = remote_name(source);
        let skipped_before = report.skipped;
        match download_tree(conn, source, &dest.join(name), progress, host, report) {
            Ok(()) => {
                report.done += 1;
                // A move deletes the original only when every file of it arrived: a skipped
                // conflict keeps it.
                if remove_source && report.skipped == skipped_before {
                    if let Err(error) = delete_tree(conn, source, progress) {
                        report.failed.push(format!(
                            "{name}: pobrano, ale nie udało się usunąć z serwera: {error}"
                        ));
                    }
                }
            }
            Err(SftpError::Cancelled) => return Err(SftpError::Cancelled),
            Err(error) => report.failed.push(format!("{name}: {error}")),
        }
    }
    Ok(())
}

fn download_tree(
    conn: &Connection,
    remote: &str,
    local: &Path,
    progress: &Progress,
    host: &mut WorkerHost,
    report: &mut Report,
) -> Result<(), SftpError> {
    cancelled(progress)?;
    let Some(entry) = conn.stat(remote)? else {
        return Err(SftpError::Remote("plik zniknął z serwera".into()));
    };
    if entry.is_dir {
        match fs::metadata(local) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(SftpError::Exists(local.display().to_string())),
            Err(_) => fs::create_dir_all(local)?,
        }
        for child in conn.list(remote)? {
            download_tree(
                conn,
                &remote_join(remote, &child.name),
                &local.join(&child.name),
                progress,
                host,
                report,
            )?;
        }
        return Ok(());
    }
    progress.set_current(&entry.name);
    let mut target = local.to_path_buf();
    let mut overwrite = false;
    if let Ok(existing) = fs::symlink_metadata(local) {
        let conflict = Conflict {
            src: PathBuf::from(remote),
            dst: local.to_path_buf(),
            src_size: entry.size,
            dst_size: existing.len(),
            src_modified: entry.modified,
            dst_modified: existing.modified().ok(),
        };
        match host.conflict(&conflict).map_err(op_error)? {
            Resolution::Overwrite => overwrite = true,
            Resolution::OverwriteOlder => {
                if newer(entry.modified, existing.modified().ok()) {
                    overwrite = true;
                } else {
                    skip(progress, entry.size, report);
                    return Ok(());
                }
            }
            Resolution::Skip => {
                skip(progress, entry.size, report);
                return Ok(());
            }
            Resolution::KeepBoth => target = free_local_name(local),
        }
    }
    conn.download(
        remote,
        &target,
        progress,
        TransferOptions {
            resume: true,
            overwrite,
        },
    )?;
    progress.finish_item();
    report.files += 1;
    Ok(())
}

/// A skipped file still counts as done for the progress bar.
fn skip(progress: &Progress, size: u64, report: &mut Report) {
    progress.add(size);
    progress.finish_item();
    report.skipped += 1;
}

fn secs(time: Option<SystemTime>) -> Option<u64> {
    time.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Strictly newer, in whole seconds (SFTP times have no finer resolution).
fn newer(source: Option<SystemTime>, target: Option<SystemTime>) -> bool {
    matches!((secs(source), secs(target)), (Some(a), Some(b)) if a > b)
}

/// `name (kopia).ext`, `name (kopia 2).ext`, ...
fn kopia_name(name: &str, n: u32) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    if n <= 1 {
        format!("{stem} (kopia){ext}")
    } else {
        format!("{stem} (kopia {n}){ext}")
    }
}

fn free_local_name(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (1..)
        .map(|n| path.with_file_name(kopia_name(&name, n)))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .expect("an unused name exists")
}

fn free_remote_name(conn: &Connection, path: &str) -> Result<String, SftpError> {
    let (dir, name) = (remote_parent(path), remote_name(path).to_string());
    for n in 1.. {
        let candidate = remote_join(&dir, &kopia_name(&name, n));
        if conn.stat(&candidate)?.is_none() {
            return Ok(candidate);
        }
    }
    unreachable!("an unused name exists")
}

// ---- server to server -------------------------------------------------------------

/// Copies through a temporary local folder: a download, then an upload (so the progress
/// counts every byte twice).
#[allow(clippy::too_many_arguments)]
fn relay_all(
    from: &Connection,
    to: &Connection,
    sources: &[String],
    dest: &str,
    remove_source: bool,
    progress: &Progress,
    host: &mut WorkerHost,
    report: &mut Report,
) -> Result<(), SftpError> {
    let (mut files, mut bytes) = (0, 0);
    for source in sources {
        measure_remote(from, source, &mut files, &mut bytes)?;
    }
    progress.set_total(bytes * 2, files * 2);
    let temp = tempfile::Builder::new()
        .prefix("bluepane-relay-")
        .tempdir()?;
    for source in sources {
        cancelled(progress)?;
        let name = remote_name(source);
        let staged = temp.path().join(name);
        let skipped_before = report.skipped;
        // Nothing is staged before, so the download asks no questions; the upload asks them
        // against the target server.
        let result = download_tree(from, source, &staged, progress, host, report).and_then(|()| {
            upload_tree(
                to,
                &staged,
                &remote_join(dest, name),
                progress,
                host,
                report,
            )
        });
        let _ = fs::remove_dir_all(&staged);
        let _ = fs::remove_file(&staged);
        match result {
            Ok(()) => {
                report.done += 1;
                if remove_source && report.skipped == skipped_before {
                    if let Err(error) = delete_tree(from, source, progress) {
                        report.failed.push(format!(
                            "{name}: skopiowano, ale nie udało się usunąć z serwera: {error}"
                        ));
                    }
                }
            }
            Err(SftpError::Cancelled) => return Err(SftpError::Cancelled),
            Err(error) => report.failed.push(format!("{name}: {error}")),
        }
    }
    Ok(())
}

fn move_within(
    conn: &Connection,
    sources: &[String],
    dest: &str,
    progress: &Progress,
    report: &mut Report,
) -> Result<(), SftpError> {
    progress.set_total(0, sources.len() as u64);
    for source in sources {
        cancelled(progress)?;
        let name = remote_name(source);
        progress.set_current(name);
        let target = remote_join(dest, name);
        if &target == source {
            report.skipped += 1;
        } else if target.starts_with(&format!("{}/", source.trim_end_matches('/'))) {
            report.failed.push(format!(
                "{name}: nie można przenieść katalogu do jego wnętrza"
            ));
        } else {
            // `rename` refuses to replace anything.
            match conn.rename(source, &target) {
                Ok(()) => report.done += 1,
                Err(error) => report.failed.push(format!("{name}: {error}")),
            }
        }
        progress.finish_item();
    }
    Ok(())
}

// ---- upload --------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn upload_all(
    conn: &Connection,
    sources: &[PathBuf],
    dest: &str,
    remove_source: bool,
    progress: &Progress,
    host: &mut WorkerHost,
    report: &mut Report,
) -> Result<(), SftpError> {
    bluepane_ops::measure(sources, progress).map_err(op_error)?;
    for source in sources {
        cancelled(progress)?;
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let skipped_before = report.skipped;
        match upload_tree(
            conn,
            source,
            &remote_join(dest, &name),
            progress,
            host,
            report,
        ) {
            Ok(()) => {
                report.done += 1;
                if remove_source && report.skipped == skipped_before {
                    if let Err(error) = bluepane_ops::delete_to_trash(source) {
                        report.failed.push(format!(
                            "{name}: wysłano, ale nie udało się przenieść do kosza: {error}"
                        ));
                    }
                }
            }
            Err(SftpError::Cancelled) => return Err(SftpError::Cancelled),
            Err(error) => report.failed.push(format!("{name}: {error}")),
        }
    }
    Ok(())
}

fn upload_tree(
    conn: &Connection,
    local: &Path,
    remote: &str,
    progress: &Progress,
    host: &mut WorkerHost,
    report: &mut Report,
) -> Result<(), SftpError> {
    cancelled(progress)?;
    let meta = fs::metadata(local)?;
    if meta.is_dir() {
        match conn.stat(remote)? {
            Some(existing) if existing.is_dir => {}
            Some(_) => return Err(SftpError::Exists(remote.to_string())),
            None => conn.mkdir(remote)?,
        }
        let mut children: Vec<_> = fs::read_dir(local)?.filter_map(Result::ok).collect();
        children.sort_by_key(|c| c.file_name());
        for child in children {
            let name = child.file_name().to_string_lossy().into_owned();
            // Symlinks to folders are not followed (no loops, nothing outside the tree).
            if fs::symlink_metadata(child.path())
                .is_ok_and(|m| m.is_symlink() && child.path().is_dir())
            {
                continue;
            }
            upload_tree(
                conn,
                &child.path(),
                &remote_join(remote, &name),
                progress,
                host,
                report,
            )?;
        }
        return Ok(());
    }
    progress.set_current(
        &local
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    let mut target = remote.to_string();
    let mut overwrite = false;
    if let Some(existing) = conn.stat(remote)? {
        let conflict = Conflict {
            src: local.to_path_buf(),
            dst: PathBuf::from(remote),
            src_size: meta.len(),
            dst_size: existing.size,
            src_modified: meta.modified().ok(),
            dst_modified: existing.modified,
        };
        match host.conflict(&conflict).map_err(op_error)? {
            Resolution::Overwrite => overwrite = true,
            Resolution::OverwriteOlder => {
                if newer(meta.modified().ok(), existing.modified) {
                    overwrite = true;
                } else {
                    skip(progress, meta.len(), report);
                    return Ok(());
                }
            }
            Resolution::Skip => {
                skip(progress, meta.len(), report);
                return Ok(());
            }
            Resolution::KeepBoth => target = free_remote_name(conn, remote)?,
        }
    }
    conn.upload(
        local,
        &target,
        progress,
        TransferOptions {
            resume: true,
            overwrite,
        },
    )?;
    progress.finish_item();
    report.files += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_paths() {
        assert_eq!(remote_join("/a", "b"), "/a/b");
        assert_eq!(remote_join("/", "b"), "/b");
        assert_eq!(remote_parent("/a/b"), "/a");
        assert_eq!(remote_parent("/a"), "/");
        assert_eq!(remote_parent("/"), "/");
        assert_eq!(remote_parent("/a/b/"), "/a");
        assert_eq!(remote_name("/a/b.txt"), "b.txt");
        assert_eq!(remote_name("/a/b/"), "b");
    }

    #[test]
    fn copy_names() {
        assert_eq!(kopia_name("a.txt", 1), "a (kopia).txt");
        assert_eq!(kopia_name("a.txt", 2), "a (kopia 2).txt");
        assert_eq!(kopia_name(".rc", 1), ".rc (kopia)");
        assert_eq!(kopia_name("noext", 1), "noext (kopia)");
    }

    #[test]
    fn newer_means_strictly_newer_in_seconds() {
        use std::time::Duration;
        let t = |s| Some(UNIX_EPOCH + Duration::from_secs(s));
        assert!(newer(t(11), t(10)));
        assert!(!newer(t(10), t(10)));
        assert!(!newer(None, t(10)));
        assert!(!newer(t(10), None));
    }
}
