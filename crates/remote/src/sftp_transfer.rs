//! Downloading and uploading one file, resumable.

use std::fs;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bluepane_ops::Progress;
use russh_sftp::protocol::OpenFlags;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::fs::TransferOptions;
use crate::sftp::SftpConnection;
use crate::RemoteError;

/// Suffix of a file that is still being transferred; it is renamed when complete.
const PART: &str = ".bpart";
const CHUNK: usize = 256 * 1024;

/// Whole seconds since the epoch (SFTP times have no finer resolution).
fn secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn part_of_local(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(PART);
    path.with_file_name(name)
}

impl SftpConnection {
    /// Downloads `remote` to `local`. The data goes to `local.bpart` first, so a cancelled
    /// or broken transfer never leaves a half file under the real name and can be resumed.
    pub fn download(
        &self,
        remote: &str,
        local: &Path,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        if !options.overwrite && fs::symlink_metadata(local).is_ok() {
            return Err(RemoteError::Exists(local.display().to_string()));
        }
        let part = part_of_local(local);
        self.run(async {
            let meta = self.sftp.metadata(remote).await?;
            let size = meta.len();
            let mut offset = 0;
            if options.resume {
                // Only a partial file written after the source last changed can be continued:
                // an older one may be the start of a different version of the file.
                if let Ok(existing) = fs::metadata(&part) {
                    let fresh = match (existing.modified(), meta.modified()) {
                        (Ok(part_time), Ok(source_time)) => secs(part_time) >= secs(source_time),
                        _ => false,
                    };
                    if existing.len() < size && fresh {
                        offset = existing.len();
                    }
                }
            }
            let mut source = self.sftp.open(remote).await?;
            let mut target = if offset > 0 {
                source.seek(SeekFrom::Start(offset)).await?;
                fs::OpenOptions::new().append(true).open(&part)?
            } else {
                fs::File::create(&part)?
            };
            progress.add(offset);
            let mut buffer = vec![0u8; CHUNK];
            loop {
                if progress.is_cancelled() {
                    return Err(RemoteError::Cancelled);
                }
                let n = source.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                std::io::Write::write_all(&mut target, &buffer[..n])?;
                progress.add(n as u64);
            }
            drop(target);
            let written = fs::metadata(&part)?.len();
            if written != size {
                return Err(RemoteError::Remote(format!(
                    "pobrano {written} z {size} bajtów (połączenie przerwane?)"
                )));
            }
            if options.overwrite && fs::symlink_metadata(local).is_ok() {
                fs::remove_file(local)?;
            }
            fs::rename(&part, local)?;
            if let Ok(time) = meta.modified() {
                if let Ok(file) = fs::OpenOptions::new().write(true).open(local) {
                    let _ = file.set_modified(time);
                }
            }
            Ok(())
        })
    }

    /// Uploads `local` to `remote`, through `remote.bpart` (see [`SftpConnection::download`]).
    pub fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        let meta = fs::metadata(local)?;
        let size = meta.len();
        if !options.overwrite && self.stat(remote)?.is_some() {
            return Err(RemoteError::Exists(remote.to_string()));
        }
        let part = format!("{remote}{PART}");
        self.run(async {
            let mut offset = 0;
            if options.resume {
                if let Some(existing) = self.stat_async(&part).await? {
                    let fresh = match (existing.modified, meta.modified()) {
                        (Some(part_time), Ok(source_time)) => secs(part_time) >= secs(source_time),
                        _ => false,
                    };
                    if existing.size < size && fresh {
                        offset = existing.size;
                    }
                }
            }
            let mut source = fs::File::open(local)?;
            let mut target = if offset > 0 {
                let mut file = self
                    .sftp
                    .open_with_flags(part.as_str(), OpenFlags::WRITE)
                    .await?;
                file.seek(SeekFrom::Start(offset)).await?;
                std::io::Seek::seek(&mut source, SeekFrom::Start(offset))?;
                file
            } else {
                self.sftp
                    .open_with_flags(
                        part.as_str(),
                        OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
                    )
                    .await?
            };
            progress.add(offset);
            let mut buffer = vec![0u8; CHUNK];
            loop {
                if progress.is_cancelled() {
                    let _ = target.shutdown().await;
                    return Err(RemoteError::Cancelled);
                }
                let n = std::io::Read::read(&mut source, &mut buffer)?;
                if n == 0 {
                    break;
                }
                target.write_all(&buffer[..n]).await?;
                progress.add(n as u64);
            }
            target.flush().await?;
            target.shutdown().await?;
            let written = self.stat_async(&part).await?.map_or(0, |e| e.size);
            if written != size {
                return Err(RemoteError::Remote(format!(
                    "wysłano {written} z {size} bajtów"
                )));
            }
            if options.overwrite && self.stat_async(remote).await?.is_some() {
                self.sftp.remove_file(remote).await?;
            }
            self.sftp.rename(part.as_str(), remote).await?;
            if let Ok(time) = meta.modified() {
                if let Ok(secs) = time.duration_since(UNIX_EPOCH) {
                    let mut attrs = russh_sftp::protocol::FileAttributes::empty();
                    attrs.mtime = Some(secs.as_secs() as u32);
                    attrs.atime = attrs.mtime;
                    let _ = self.sftp.set_metadata(remote, attrs).await;
                }
            }
            Ok(())
        })
    }
}
