//! Text formatting shared by the front ends.

use std::fmt::Display;
use std::time::SystemTime;

use chrono::{DateTime, Local, TimeZone, Utc};

/// Permission bits the way `ls -l` writes them: `0o755` is `rwxr-xr-x`, and setuid, setgid and
/// sticky show as `s`/`S`/`t`/`T` in the execute place. Empty when the system has no bits.
pub fn format_mode(mode: Option<u32>) -> String {
    let Some(mode) = mode else {
        return String::new();
    };
    let mut out = String::with_capacity(9);
    for (shift, special, letter) in [(6, 0o4000, 's'), (3, 0o2000, 's'), (0, 0o1000, 't')] {
        let bits = (mode >> shift) & 0o7;
        out.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        out.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        let execute = bits & 0o1 != 0;
        out.push(match (mode & special != 0, execute) {
            (true, true) => letter,
            (true, false) => letter.to_ascii_uppercase(),
            (false, true) => 'x',
            (false, false) => '-',
        });
    }
    out
}

/// `1536` becomes `1.5 KB`; plain bytes below 1 KB.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Splits a file name into (stem, extension without the dot). Directories, dot-files and
/// names without a dot have no extension.
pub fn name_parts(name: &str, is_dir: bool) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && !is_dir => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// Modification time in the user's local time zone, `2026-10-13 06:28` (empty if unknown).
pub fn format_modified(modified: Option<SystemTime>) -> String {
    modified.map_or_else(String::new, |time| format_in(time, &Local))
}

fn format_in<Tz>(time: SystemTime, zone: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: Display,
{
    DateTime::<Utc>::from(time)
        .with_timezone(zone)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn permission_bits_are_written_like_ls() {
        assert_eq!(format_mode(Some(0o755)), "rwxr-xr-x");
        assert_eq!(format_mode(Some(0o640)), "rw-r-----");
        assert_eq!(format_mode(Some(0o4755)), "rwsr-xr-x");
        assert_eq!(format_mode(Some(0o4644)), "rwSr--r--");
        assert_eq!(format_mode(Some(0o1777)), "rwxrwxrwt");
        assert_eq!(format_mode(Some(0o2750)), "rwxr-s---");
        assert_eq!(format_mode(None), "");
    }

    #[test]
    fn names_split_into_stem_and_extension() {
        assert_eq!(
            name_parts("report.final.txt", false),
            ("report.final", "txt")
        );
        assert_eq!(name_parts("Makefile", false), ("Makefile", ""));
        assert_eq!(name_parts(".bashrc", false), (".bashrc", ""));
        assert_eq!(name_parts("v1.2", true), ("v1.2", ""));
        assert_eq!(name_parts("trailing.", false), ("trailing", ""));
    }

    #[test]
    fn dates_use_the_given_zone() {
        let t = UNIX_EPOCH + Duration::from_secs(1_760_336_880); // 2025-10-13 06:28:00 UTC
        assert_eq!(format_in(t, &Utc), "2025-10-13 06:28");
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(format_in(t, &plus_two), "2025-10-13 08:28");
    }

    #[test]
    fn local_dates_have_a_fixed_shape() {
        let text = format_modified(Some(SystemTime::now()));
        assert_eq!(text.len(), 16, "{text}");
        assert_eq!(format_modified(None), "");
    }
}
