//! Archive formats and split volumes.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    /// 7-Zip's own format (LZMA2, AES-256, optionally with encrypted file names).
    SevenZ,
}

impl Format {
    /// File name extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::SevenZ => "7z",
        }
    }

    /// Whether the format can be password protected.
    pub fn supports_password(self) -> bool {
        matches!(self, Format::Zip | Format::SevenZ)
    }

    /// Whether the file names can be hidden behind the password (7z only: a ZIP always shows
    /// its names).
    pub fn can_encrypt_names(self) -> bool {
        self == Format::SevenZ
    }

    /// The format a file name stands for. A split archive is recognised by its first
    /// volume (`name.zip.001`); later volumes (`.002`) are not archives on their own.
    pub fn from_name(name: &str) -> Option<Format> {
        let lower = name.to_lowercase();
        let base = match lower.strip_suffix(".001") {
            Some(base) => base,
            None if is_later_volume(&lower) => return None,
            None => &lower,
        };
        if base.ends_with(".zip") {
            Some(Format::Zip)
        } else if base.ends_with(".7z") {
            Some(Format::SevenZ)
        } else if base.ends_with(".tar.gz") || base.ends_with(".tgz") {
            Some(Format::TarGz)
        } else if base.ends_with(".tar") {
            Some(Format::Tar)
        } else {
            None
        }
    }
}

/// `.002`, `.003`, ... (volumes after the first).
fn is_later_volume(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((_, ext)) => {
            ext.len() == 3
                && ext.bytes().all(|b| b.is_ascii_digit())
                && ext != "001"
                && ext != "000"
        }
        None => false,
    }
}

/// The format of the archive at `path`, judged by its name.
pub fn detect(path: &Path) -> Option<Format> {
    Format::from_name(&path.file_name()?.to_string_lossy())
}

/// All volumes of the archive at `path`, in order. For `name.zip.001` that is every
/// consecutively numbered volume that exists; for any other path just the path itself.
pub fn volumes_of(path: &Path) -> Vec<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let Some(base) = name.as_deref().and_then(|n| n.strip_suffix(".001")) else {
        return vec![path.to_path_buf()];
    };
    let mut volumes = Vec::new();
    for n in 1.. {
        let candidate = path.with_file_name(format!("{base}.{n:03}"));
        if !candidate.is_file() {
            break;
        }
        volumes.push(candidate);
    }
    if volumes.is_empty() {
        vec![path.to_path_buf()]
    } else {
        volumes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_recognised_by_name_case_insensitively() {
        assert_eq!(Format::from_name("a.zip"), Some(Format::Zip));
        assert_eq!(Format::from_name("A.ZIP"), Some(Format::Zip));
        assert_eq!(Format::from_name("a.tar"), Some(Format::Tar));
        assert_eq!(Format::from_name("a.tar.gz"), Some(Format::TarGz));
        assert_eq!(Format::from_name("a.tgz"), Some(Format::TarGz));
        assert_eq!(Format::from_name("a.7z"), Some(Format::SevenZ));
        assert_eq!(Format::from_name("A.7Z"), Some(Format::SevenZ));
        assert_eq!(Format::from_name("a.txt"), None);
        assert_eq!(Format::from_name("zip"), None);
        assert_eq!(Format::from_name("a.gz"), None);
    }

    #[test]
    fn only_the_first_volume_of_a_split_archive_opens() {
        assert_eq!(Format::from_name("a.zip.001"), Some(Format::Zip));
        assert_eq!(Format::from_name("a.tar.gz.001"), Some(Format::TarGz));
        assert_eq!(Format::from_name("a.7z.001"), Some(Format::SevenZ));
        assert_eq!(Format::from_name("a.7z.002"), None);
        assert_eq!(Format::from_name("a.zip.002"), None);
        assert_eq!(Format::from_name("a.zip.123"), None);
        assert_eq!(Format::from_name("photo.001"), None);
    }

    #[test]
    fn volumes_are_collected_until_the_numbering_breaks() {
        let tmp = tempfile::tempdir().unwrap();
        for n in ["001", "002", "003", "005"] {
            std::fs::write(tmp.path().join(format!("a.zip.{n}")), "x").unwrap();
        }
        let volumes = volumes_of(&tmp.path().join("a.zip.001"));
        let names: Vec<_> = volumes
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.zip.001", "a.zip.002", "a.zip.003"]);
        let single = tmp.path().join("b.zip");
        assert_eq!(volumes_of(&single), [single]);
    }

    #[test]
    fn passwords_are_for_zip_and_7z_and_hidden_names_for_7z_only() {
        assert!(Format::Zip.supports_password());
        assert!(Format::SevenZ.supports_password());
        assert!(!Format::Tar.supports_password());
        assert!(!Format::TarGz.supports_password());
        assert!(Format::SevenZ.can_encrypt_names());
        assert!(!Format::Zip.can_encrypt_names());
    }
}
