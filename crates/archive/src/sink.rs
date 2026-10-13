//! One way to write an archive, whatever its format.
//!
//! Creating an archive and modifying one both end up feeding entries into a [`Sink`]; the
//! sink knows the format's details (headers, compression, encryption).

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use bluepane_ops::Progress;
use flate2::write::GzEncoder;
use flate2::Compression;
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

use crate::create::Level;
use crate::format::Format;
use crate::time::to_zip;
use crate::ArchiveError;

/// What the archive remembers about an entry besides its name and content.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Meta {
    pub modified: Option<SystemTime>,
    /// Unix permission bits.
    pub mode: Option<u32>,
}

/// An entry of an existing archive, handed out while it is being rewritten.
pub(crate) struct Existing {
    /// The cleaned-up path, or `None` for an entry with an unsafe name.
    pub path: Option<String>,
    /// The name as stored.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub meta: Meta,
    /// A symlink, device or the like, which a rewrite cannot carry over.
    pub special: bool,
}

/// Settings every sink needs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SinkSettings<'a> {
    pub format: Format,
    pub level: Level,
    pub password: Option<&'a str>,
    /// 7z only: hide the file names too.
    pub encrypt_names: bool,
}

pub(crate) trait Sink {
    fn add_dir(&mut self, name: &str, meta: &Meta) -> Result<(), ArchiveError>;
    /// `data` yields exactly `size` bytes.
    fn add_file(
        &mut self,
        name: &str,
        size: u64,
        meta: &Meta,
        data: &mut dyn Read,
    ) -> Result<(), ArchiveError>;
    /// Writes whatever closes the archive (central directory, trailer).
    fn finish(self: Box<Self>) -> Result<(), ArchiveError>;
}

pub(crate) fn io_failure(progress: &Progress, error: io::Error) -> ArchiveError {
    if progress.is_cancelled() {
        ArchiveError::Cancelled
    } else {
        ArchiveError::Io(error)
    }
}

pub(crate) fn zip_failure(progress: &Progress, error: zip::result::ZipError) -> ArchiveError {
    match error {
        zip::result::ZipError::Io(e) => io_failure(progress, e),
        other => ArchiveError::Op(other.to_string()),
    }
}

/// Opens a sink that writes the archive to `temp`.
pub(crate) fn open_sink<'a>(
    temp: &Path,
    settings: &SinkSettings<'_>,
    progress: &'a Progress,
) -> Result<Box<dyn Sink + 'a>, ArchiveError> {
    Ok(match settings.format {
        Format::Zip => Box::new(ZipSink::open(temp, settings, progress)?),
        Format::Tar | Format::TarGz => Box::new(TarSink::open(temp, settings, progress)?),
        Format::SevenZ => crate::seven::open_sink(temp, settings, progress)?,
    })
}

pub(crate) struct ZipSink<'a> {
    pub zip: ZipWriter<File>,
    method: CompressionMethod,
    level: Level,
    password: Option<String>,
    progress: &'a Progress,
}

impl<'a> ZipSink<'a> {
    pub fn open(
        temp: &Path,
        settings: &SinkSettings<'_>,
        progress: &'a Progress,
    ) -> Result<Self, ArchiveError> {
        Ok(ZipSink {
            zip: ZipWriter::new(File::create(temp)?),
            method: if settings.level == Level::Store {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            },
            level: settings.level,
            password: settings.password.map(str::to_string),
            progress,
        })
    }

    fn options(&self, meta: &Meta) -> SimpleFileOptions {
        let mut options = SimpleFileOptions::default()
            .compression_method(self.method)
            .compression_level(match self.level {
                Level::Store => None,
                level => Some(level.deflate_level()),
            });
        if let Some(stamp) = meta.modified.and_then(to_zip) {
            options = options.last_modified_time(stamp);
        }
        if let Some(mode) = meta.mode {
            options = options.unix_permissions(mode);
        }
        options
    }

    /// Copies an entry of another ZIP as it is (compressed, encrypted), under `name`.
    pub fn raw_copy<R: Read + io::Seek>(
        &mut self,
        file: zip::read::ZipFile<'_, R>,
        name: &str,
    ) -> Result<(), ArchiveError> {
        self.zip
            .raw_copy_file_rename(file, name)
            .map_err(|e| zip_failure(self.progress, e))
    }

    pub fn finish_zip(self) -> Result<(), ArchiveError> {
        let progress = self.progress;
        self.zip.finish().map_err(|e| zip_failure(progress, e))?;
        Ok(())
    }
}

impl Sink for ZipSink<'_> {
    fn add_dir(&mut self, name: &str, meta: &Meta) -> Result<(), ArchiveError> {
        let options = self.options(meta);
        self.zip
            .add_directory(format!("{name}/"), options)
            .map_err(|e| zip_failure(self.progress, e))
    }

    fn add_file(
        &mut self,
        name: &str,
        size: u64,
        meta: &Meta,
        data: &mut dyn Read,
    ) -> Result<(), ArchiveError> {
        let mut options = self.options(meta).large_file(size >= u64::from(u32::MAX));
        if let Some(password) = &self.password {
            options = options.with_aes_encryption(AesMode::Aes256, password);
        }
        self.zip
            .start_file(name, options)
            .map_err(|e| zip_failure(self.progress, e))?;
        io::copy(data, &mut self.zip).map_err(|e| io_failure(self.progress, e))?;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), ArchiveError> {
        (*self).finish_zip()
    }
}

/// Where a tar stream goes: plain, or through gzip.
enum TarOut {
    Plain(File),
    Gz(Box<GzEncoder<File>>),
}

impl Write for TarOut {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            TarOut::Plain(file) => file.write(buf),
            TarOut::Gz(encoder) => encoder.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            TarOut::Plain(file) => file.flush(),
            TarOut::Gz(encoder) => encoder.flush(),
        }
    }
}

pub(crate) struct TarSink<'a> {
    builder: tar::Builder<TarOut>,
    progress: &'a Progress,
}

impl<'a> TarSink<'a> {
    pub fn open(
        temp: &Path,
        settings: &SinkSettings<'_>,
        progress: &'a Progress,
    ) -> Result<Self, ArchiveError> {
        let file = File::create(temp)?;
        let out = if settings.format == Format::TarGz {
            let level = match settings.level {
                Level::Store => Compression::none(),
                Level::Fast => Compression::fast(),
                Level::Normal => Compression::default(),
                Level::Maximum => Compression::best(),
            };
            TarOut::Gz(Box::new(GzEncoder::new(file, level)))
        } else {
            TarOut::Plain(file)
        };
        Ok(TarSink {
            builder: tar::Builder::new(out),
            progress,
        })
    }

    fn header(meta: &Meta, size: u64, entry_type: tar::EntryType) -> tar::Header {
        let mut header = tar::Header::new_gnu();
        header.set_size(size);
        header.set_entry_type(entry_type);
        header.set_mode(meta.mode.unwrap_or(match entry_type {
            tar::EntryType::Directory => 0o755,
            _ => 0o644,
        }));
        let secs = meta
            .modified
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        header.set_mtime(secs);
        header
    }
}

impl Sink for TarSink<'_> {
    fn add_dir(&mut self, name: &str, meta: &Meta) -> Result<(), ArchiveError> {
        let mut header = Self::header(meta, 0, tar::EntryType::Directory);
        self.builder
            .append_data(&mut header, format!("{name}/"), io::empty())
            .map_err(|e| io_failure(self.progress, e))
    }

    fn add_file(
        &mut self,
        name: &str,
        size: u64,
        meta: &Meta,
        data: &mut dyn Read,
    ) -> Result<(), ArchiveError> {
        let mut header = Self::header(meta, size, tar::EntryType::Regular);
        self.builder
            .append_data(&mut header, name, data)
            .map_err(|e| io_failure(self.progress, e))
    }

    fn finish(mut self: Box<Self>) -> Result<(), ArchiveError> {
        let progress = self.progress;
        self.builder.finish().map_err(|e| io_failure(progress, e))?;
        match self
            .builder
            .into_inner()
            .map_err(|e| io_failure(progress, e))?
        {
            TarOut::Plain(mut file) => file.flush().map_err(|e| io_failure(progress, e)),
            TarOut::Gz(encoder) => encoder
                .finish()
                .map(|_| ())
                .map_err(|e| io_failure(progress, e)),
        }
    }
}
