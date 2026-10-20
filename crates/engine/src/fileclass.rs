//! What kind of thing a list entry is, for the icon and colour the front end gives it.
//! The decision lives here (by name only; nothing is opened) so that every front end and
//! the tests agree, and the drawing code only maps a class to a picture.

use bluepane_vfs::{Entry, EntryKind};

use crate::panel::is_parent_entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileClass {
    /// The `..` row.
    Parent,
    Dir,
    Symlink,
    /// Archives and packages (zip, 7z, tar, rar, deb, iso, ...).
    Archive,
    /// Documents the viewer knows (PDF, Word, Excel, OpenDocument, RTF, PowerPoint).
    Document,
    Image,
    Audio,
    Video,
    /// Source code and scripts.
    Code,
    /// Plain text, notes and configuration.
    Text,
    /// Programs and installers.
    Executable,
    Other,
}

const ARCHIVES: &[&str] = &[
    "rar", "xz", "bz2", "gz", "zst", "lz", "lzma", "cab", "iso", "deb", "rpm", "jar", "apk", "tar",
    "tgz", "zip", "7z",
];
const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "tif", "tiff", "heic", "avif",
];
const AUDIO: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "opus", "m4a", "aac", "wma", "mid",
];
const VIDEO: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v", "mpg",
];
const CODE: &[&str] = &[
    "rs", "c", "h", "cc", "cpp", "hpp", "cs", "java", "kt", "go", "py", "rb", "php", "js", "mjs",
    "ts", "tsx", "jsx", "swift", "lua", "sql", "sh", "bash", "zsh", "fish", "ps1", "html", "htm",
    "css", "scss", "vue",
];
const TEXT: &[&str] = &[
    "txt",
    "md",
    "log",
    "ini",
    "conf",
    "cfg",
    "toml",
    "yaml",
    "yml",
    "json",
    "xml",
    "csv",
    "tsv",
    "lock",
    "env",
    "gitignore",
    "forward",
    "profile",
    "rules",
];
const EXECUTABLES: &[&str] = &[
    "exe", "msi", "bat", "cmd", "com", "appimage", "run", "bin", "dll", "so", "scr",
];

/// The class of one list entry.
pub fn class_of(entry: &Entry) -> FileClass {
    if is_parent_entry(entry) {
        return FileClass::Parent;
    }
    match entry.kind {
        EntryKind::Dir => return FileClass::Dir,
        EntryKind::Symlink => return FileClass::Symlink,
        _ => {}
    }
    class_of_name(&entry.name)
}

/// The class of a file by its name.
pub fn class_of_name(name: &str) -> FileClass {
    let lower = name.to_lowercase();
    if bluepane_archive::Format::from_name(name).is_some() {
        return FileClass::Archive;
    }
    // Dot files such as `.gitignore` have no extension: the whole name is the word.
    let ext = match lower.rfind('.') {
        Some(0) => &lower[1..],
        Some(dot) => &lower[dot + 1..],
        None => "",
    };
    let is = |list: &[&str]| list.contains(&ext);
    if bluepane_docs::kind_of(std::path::Path::new(name)).is_some() {
        FileClass::Document
    } else if is(ARCHIVES) {
        FileClass::Archive
    } else if is(IMAGES) {
        FileClass::Image
    } else if is(AUDIO) {
        FileClass::Audio
    } else if is(VIDEO) {
        FileClass::Video
    } else if is(CODE) {
        FileClass::Code
    } else if is(TEXT) {
        FileClass::Text
    } else if is(EXECUTABLES) {
        FileClass::Executable
    } else {
        FileClass::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(name: &str, kind: EntryKind) -> Entry {
        Entry {
            name: name.into(),
            path: PathBuf::from("/x").join(name),
            kind,
            size: 1,
            modified: None,
            created: None,
            mode: None,
            owner: None,
            hidden: false,
        }
    }

    #[test]
    fn files_fall_into_classes_by_name_regardless_of_case() {
        for (name, class) in [
            ("a.ZIP", FileClass::Archive),
            ("backup.tar.gz", FileClass::Archive),
            ("movie.rar", FileClass::Archive),
            ("report.PDF", FileClass::Document),
            ("letter.docx", FileClass::Document),
            ("deck.pptx", FileClass::Document),
            ("note.rtf", FileClass::Document),
            ("photo.jpeg", FileClass::Image),
            ("song.flac", FileClass::Audio),
            ("clip.mkv", FileClass::Video),
            ("lib.rs", FileClass::Code),
            ("run.sh", FileClass::Code),
            ("Cargo.toml", FileClass::Text),
            (".gitignore", FileClass::Text),
            ("setup.exe", FileClass::Executable),
            ("noextension", FileClass::Other),
            ("weird.qqq", FileClass::Other),
        ] {
            assert_eq!(class_of_name(name), class, "{name}");
        }
    }

    #[test]
    fn directories_links_and_the_parent_row_come_first() {
        assert_eq!(class_of(&entry("src", EntryKind::Dir)), FileClass::Dir);
        // A folder that looks like a file is still a folder.
        assert_eq!(class_of(&entry("v1.zip", EntryKind::Dir)), FileClass::Dir);
        assert_eq!(
            class_of(&entry("latest", EntryKind::Symlink)),
            FileClass::Symlink
        );
        assert_eq!(class_of(&entry("a.rs", EntryKind::File)), FileClass::Code);
    }
}
