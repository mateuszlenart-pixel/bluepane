//! Windows "verbatim" paths (`\\?\C:\...`), which lift the 260 character limit.
//!
//! The string logic is plain and platform independent so it can be tested everywhere;
//! only `windows.rs` calls it.

/// Paths shorter than this are left alone: they work without the prefix, and the prefix
/// makes paths look unusual in messages.
#[cfg_attr(not(windows), allow(dead_code))]
pub const LONG_PATH_THRESHOLD: usize = 240;

/// Converts an absolute, normalised Windows path to its verbatim form. Returns `None` when
/// the path is already verbatim or is not an absolute drive or UNC path.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn to_verbatim(absolute: &str) -> Option<String> {
    if absolute.starts_with(r"\\?\") || absolute.starts_with(r"\\.\") {
        return None;
    }
    if let Some(unc) = absolute.strip_prefix(r"\\") {
        return Some(format!(r"\\?\UNC\{unc}"));
    }
    let mut chars = absolute.chars();
    let drive = chars.next()?;
    if drive.is_ascii_alphabetic() && chars.next() == Some(':') && chars.next() == Some('\\') {
        return Some(format!(r"\\?\{absolute}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_paths_get_the_prefix() {
        assert_eq!(
            to_verbatim(r"C:\Users\a\file.txt").as_deref(),
            Some(r"\\?\C:\Users\a\file.txt")
        );
    }

    #[test]
    fn unc_paths_use_the_unc_form() {
        assert_eq!(
            to_verbatim(r"\\server\share\dir").as_deref(),
            Some(r"\\?\UNC\server\share\dir")
        );
    }

    #[test]
    fn already_verbatim_or_relative_paths_are_left_alone() {
        assert_eq!(to_verbatim(r"\\?\C:\x"), None);
        assert_eq!(to_verbatim(r"\\.\COM1"), None);
        assert_eq!(to_verbatim(r"relative\path"), None);
        assert_eq!(to_verbatim("C:relative"), None);
        assert_eq!(to_verbatim(""), None);
    }
}
