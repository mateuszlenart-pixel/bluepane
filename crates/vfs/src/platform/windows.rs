use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;

use crate::Volume;

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// Hidden or system attribute set.
pub fn is_hidden(_name: &str, meta: &Metadata) -> bool {
    meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

/// Drive letters that currently exist.
pub fn volumes() -> Vec<Volume> {
    ('A'..='Z')
        .filter_map(|letter| {
            let path = PathBuf::from(format!("{letter}:\\"));
            path.exists().then(|| Volume {
                name: format!("{letter}:"),
                path,
            })
        })
        .collect()
}
