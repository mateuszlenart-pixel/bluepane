//! Data read from inside a file for the masks `[EXIF:...]` and `[TAG:...]`: when a photo was
//! taken and with what camera (EXIF), the artist, title, album and so on of an audio file.
//! Reading happens only for files of a rename whose mask asks for such a field, and a file
//! that has no such data (or is not that kind of file) simply has none.

use std::io::BufReader;
use std::path::Path;

use crate::Stamp;

/// What was found in one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    /// When the photo was taken (`DateTimeOriginal`), as written by the camera.
    pub taken: Option<Stamp>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub track: Option<u32>,
    pub year: Option<u32>,
}

/// Reads whatever the file offers. Never fails: an unreadable or unsupported file has no data.
pub fn read_meta(path: &Path) -> Meta {
    let mut meta = Meta::default();
    read_exif(path, &mut meta);
    read_tags(path, &mut meta);
    meta
}

/// Everything readable about the inside of a media file, as (label, value) lines for a
/// reader: the EXIF fields of a photo, the properties and tags of an audio file. A file that
/// is neither gives nothing. Never fails.
pub fn describe(path: &Path) -> Vec<(String, String)> {
    let mut lines = Vec::new();
    describe_exif(path, &mut lines);
    describe_audio(path, &mut lines);
    lines
}

/// A long value (a maker note, say) is cut.
fn short(text: String) -> String {
    let text = text.trim().to_string();
    if text.chars().count() > 160 {
        let cut: String = text.chars().take(160).collect();
        format!("{cut}…")
    } else {
        text
    }
}

fn describe_exif(path: &Path, lines: &mut Vec<(String, String)>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else {
        return;
    };
    for field in exif
        .fields()
        .filter(|f| f.ifd_num == exif::In::PRIMARY)
        .take(80)
    {
        let is_date = matches!(
            field.tag,
            exif::Tag::DateTime | exif::Tag::DateTimeOriginal | exif::Tag::DateTimeDigitized
        );
        let value = match &field.value {
            // Text such as the camera's name is UTF-8 more often than not: read it as such
            // (the library's own display would escape every non-ASCII byte).
            exif::Value::Ascii(parts) if !is_date => parts
                .iter()
                .map(|p| {
                    String::from_utf8_lossy(p)
                        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(", "),
            _ => field.display_value().with_unit(&exif).to_string(),
        };
        let value = short(value);
        if !value.is_empty() && !value.starts_with("[") {
            lines.push((format!("EXIF {}", field.tag), value));
        }
    }
}

fn describe_audio(path: &Path, lines: &mut Vec<(String, String)>) {
    use lofty::prelude::*;
    let Ok(file) = lofty::read_from_path(path) else {
        return;
    };
    let props = file.properties();
    let secs = props.duration().as_secs();
    if secs > 0 {
        lines.push((
            "Czas trwania".into(),
            format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
        ));
    }
    let fact = |label: &str, value: Option<u32>, unit: &str, lines: &mut Vec<(String, String)>| {
        if let Some(v) = value.filter(|v| *v > 0) {
            lines.push((label.to_string(), format!("{v} {unit}")));
        }
    };
    fact("Przepływność", props.audio_bitrate(), "kb/s", lines);
    fact("Częstotliwość", props.sample_rate(), "Hz", lines);
    fact("Kanały", props.channels().map(u32::from), "", lines);
    fact("Głębia", props.bit_depth().map(u32::from), "bitów", lines);
    lines.push(("Format".into(), format!("{:?}", file.file_type())));
    if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
        let text = |label: &str,
                    value: Option<std::borrow::Cow<'_, str>>,
                    lines: &mut Vec<(String, String)>| {
            if let Some(v) = value
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
            {
                lines.push((label.to_string(), short(v)));
            }
        };
        text("Wykonawca", tag.artist(), lines);
        text("Tytuł", tag.title(), lines);
        text("Album", tag.album(), lines);
        text("Gatunek", tag.genre(), lines);
        text("Komentarz", tag.comment(), lines);
        if let Some(track) = tag.track() {
            lines.push(("Ścieżka".into(), track.to_string()));
        }
        if let Some(year) = tag.year() {
            lines.push(("Rok".into(), year.to_string()));
        }
    }
}

fn ascii_field(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let field = exif.get_field(tag, exif::In::PRIMARY)?;
    match &field.value {
        exif::Value::Ascii(parts) => {
            let text = String::from_utf8_lossy(parts.first()?)
                .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_string();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn read_exif(path: &Path, meta: &mut Meta) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else {
        return;
    };
    // The moment of shooting first; the others are when the file was digitized or saved.
    for tag in [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ] {
        let Some(text) = ascii_field(&exif, tag) else {
            continue;
        };
        if let Ok(date) = exif::DateTime::from_ascii(text.as_bytes()) {
            meta.taken = Some(Stamp {
                year: i32::from(date.year),
                month: u32::from(date.month),
                day: u32::from(date.day),
                hour: u32::from(date.hour),
                minute: u32::from(date.minute),
                second: u32::from(date.second),
            });
            break;
        }
    }
    meta.make = ascii_field(&exif, exif::Tag::Make);
    meta.model = ascii_field(&exif, exif::Tag::Model);
}

fn read_tags(path: &Path, meta: &mut Meta) {
    use lofty::prelude::*;
    let Ok(file) = lofty::read_from_path(path) else {
        return;
    };
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return;
    };
    let text = |value: Option<std::borrow::Cow<'_, str>>| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    meta.artist = text(tag.artist());
    meta.title = text(tag.title());
    meta.album = text(tag.album());
    meta.genre = text(tag.genre());
    meta.track = tag.track();
    meta.year = tag.year();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A JPEG with nothing but an EXIF block: Make, and DateTimeOriginal in the Exif IFD.
    fn jpeg_with_exif(make: &str, taken: &str) -> Vec<u8> {
        let mut make_bytes = make.as_bytes().to_vec();
        make_bytes.push(0);
        let mut taken_bytes = taken.as_bytes().to_vec();
        taken_bytes.push(0);
        // Big-endian TIFF: header (8), IFD0 with two entries (30), Make data, Exif IFD with one
        // entry (18), DateTimeOriginal data.
        let ifd0 = 8u32;
        let make_at = ifd0 + 30;
        let exif_ifd = make_at + make_bytes.len() as u32;
        let taken_at = exif_ifd + 18;
        let mut tiff = vec![b'M', b'M', 0, 42, 0, 0, 0, 8];
        tiff.extend(2u16.to_be_bytes());
        // Make: tag 0x010F, ASCII, count, offset.
        tiff.extend(0x010Fu16.to_be_bytes());
        tiff.extend(2u16.to_be_bytes());
        tiff.extend((make_bytes.len() as u32).to_be_bytes());
        tiff.extend(make_at.to_be_bytes());
        // Pointer to the Exif IFD: tag 0x8769, LONG.
        tiff.extend(0x8769u16.to_be_bytes());
        tiff.extend(4u16.to_be_bytes());
        tiff.extend(1u32.to_be_bytes());
        tiff.extend(exif_ifd.to_be_bytes());
        tiff.extend(0u32.to_be_bytes());
        tiff.extend(&make_bytes);
        tiff.extend(1u16.to_be_bytes());
        tiff.extend(0x9003u16.to_be_bytes());
        tiff.extend(2u16.to_be_bytes());
        tiff.extend((taken_bytes.len() as u32).to_be_bytes());
        tiff.extend(taken_at.to_be_bytes());
        tiff.extend(0u32.to_be_bytes());
        tiff.extend(&taken_bytes);
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(tiff);
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend(((app1.len() + 2) as u16).to_be_bytes());
        jpeg.extend(app1);
        jpeg.extend([0xFF, 0xD9]);
        jpeg
    }

    #[test]
    fn exif_gives_the_date_taken_and_the_camera() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zdjęcie.jpg");
        std::fs::write(&path, jpeg_with_exif("Canon", "2026:10:04 07:05:09")).unwrap();
        let meta = read_meta(&path);
        assert_eq!(
            meta.taken,
            Some(Stamp {
                year: 2026,
                month: 10,
                day: 4,
                hour: 7,
                minute: 5,
                second: 9
            })
        );
        assert_eq!(meta.make.as_deref(), Some("Canon"));
        assert_eq!(meta.model, None);
        assert_eq!(meta.artist, None);
    }

    #[test]
    fn files_without_data_and_missing_files_have_none() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notatki.txt");
        std::fs::write(&text, "to nie zdjęcie ani utwór").unwrap();
        assert_eq!(read_meta(&text), Meta::default());
        assert_eq!(read_meta(&dir.path().join("brak.jpg")), Meta::default());
        // Broken: starts like a JPEG but ends at once.
        let broken = dir.path().join("zepsute.jpg");
        std::fs::write(&broken, [0xFF, 0xD8, 0xFF]).unwrap();
        assert_eq!(read_meta(&broken), Meta::default());
    }

    /// A WAV file with a little silence, enough for the tags to be written into it.
    fn silent_wav() -> Vec<u8> {
        let mut wav = b"RIFF".to_vec();
        wav.extend((36u32 + 160).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes()); // PCM
        wav.extend(1u16.to_le_bytes()); // mono
        wav.extend(8000u32.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(160u32.to_le_bytes());
        wav.extend([0u8; 160]);
        wav
    }

    #[test]
    fn audio_tags_are_read_back_after_they_were_written() {
        use lofty::config::WriteOptions;
        use lofty::prelude::*;
        use lofty::tag::{Tag, TagType};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("utwór.wav");
        std::fs::write(&path, silent_wav()).unwrap();
        let mut tag = Tag::new(TagType::RiffInfo);
        tag.set_artist("Zespół Żółć".into());
        tag.set_title("Gęślą jaźń".into());
        tag.set_album("Płyta".into());
        tag.set_track(7);
        tag.set_genre("Rock".into());
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        let meta = read_meta(&path);
        assert_eq!(meta.artist.as_deref(), Some("Zespół Żółć"));
        assert_eq!(meta.title.as_deref(), Some("Gęślą jaźń"));
        assert_eq!(meta.album.as_deref(), Some("Płyta"));
        assert_eq!(meta.genre.as_deref(), Some("Rock"));
        assert_eq!(meta.track, Some(7));
        assert_eq!(meta.taken, None);
    }

    #[test]
    fn describe_lists_exif_fields_and_audio_properties_with_tags() {
        use lofty::config::WriteOptions;
        use lofty::prelude::*;
        use lofty::tag::{Tag, TagType};
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("zdjęcie.jpg");
        std::fs::write(&photo, jpeg_with_exif("Aparat Żółć", "2026:10:04 07:05:09")).unwrap();
        let lines = describe(&photo);
        assert!(
            lines
                .iter()
                .any(|(label, value)| label.starts_with("EXIF") && value.contains("Aparat Żółć")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|(_, v)| v.contains("2026-10-04")),
            "{lines:?}"
        );

        let audio = dir.path().join("utwór.wav");
        std::fs::write(&audio, silent_wav()).unwrap();
        let mut tag = Tag::new(TagType::RiffInfo);
        tag.set_artist("Zespół".into());
        tag.set_title("Gęślą jaźń".into());
        tag.set_year(1999);
        tag.save_to_path(&audio, WriteOptions::default()).unwrap();
        let lines = describe(&audio);
        let get = |label: &str| {
            lines
                .iter()
                .find(|(l, _)| l == label)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("Częstotliwość"), Some("8000 Hz"), "{lines:?}");
        assert_eq!(get("Wykonawca"), Some("Zespół"));
        assert_eq!(get("Tytuł"), Some("Gęślą jaźń"));
        assert!(get("Format").is_some());

        // Not a media file, missing, broken: nothing, and no panic.
        let text = dir.path().join("a.txt");
        std::fs::write(&text, "zwykły tekst").unwrap();
        assert!(describe(&text).is_empty());
        assert!(describe(&dir.path().join("brak.mp3")).is_empty());
        let broken = dir.path().join("zepsute.jpg");
        std::fs::write(&broken, [0xFF, 0xD8, 0xFF]).unwrap();
        assert!(describe(&broken).is_empty());
    }

    /// The files in `testdata/media` are what `docs/TESTOWANIE.md` tells the tester to expect.
    #[test]
    fn the_shipped_media_test_data_has_the_exif_and_tags_the_manual_tests_expect() {
        let media = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/media");
        let photo = read_meta(&media.join("zdjecie-exif.jpg"));
        assert_eq!(photo.make.as_deref(), Some("Aparat Żółć"));
        assert_eq!(photo.model.as_deref(), Some("Model gęślą 5"));
        assert_eq!(
            photo.taken,
            Some(Stamp {
                year: 2025,
                month: 7,
                day: 14,
                hour: 18,
                minute: 30,
                second: 5
            })
        );
        let second = read_meta(&media.join("zdjecie-exif-2.jpg"));
        assert_eq!(second.make.as_deref(), Some("Inny Aparat"));
        assert_eq!(
            second.taken,
            Some(Stamp {
                year: 2024,
                month: 12,
                day: 24,
                hour: 22,
                minute: 10,
                second: 0
            })
        );
        assert_eq!(
            read_meta(&media.join("zdjecie-bez-exif.jpg")),
            Meta::default()
        );

        let first = read_meta(&media.join("utwor-tagi.wav"));
        assert_eq!(first.artist.as_deref(), Some("Zespół Żółć"));
        assert_eq!(first.title.as_deref(), Some("Gęślą jaźń"));
        assert_eq!(first.album.as_deref(), Some("Płyta testowa"));
        assert_eq!(first.genre.as_deref(), Some("Rock"));
        let other = read_meta(&media.join("utwor-2.wav"));
        assert_eq!(other.artist.as_deref(), Some("Beta Kwartet"));
        assert_eq!(other.title.as_deref(), Some("Łódź nocą"));
        assert_eq!((first.track, first.year), (Some(7), Some(1999)));
        assert_eq!((other.track, other.year), (Some(2), Some(2012)));
        let lines = describe(&media.join("utwor-tagi.wav"));
        assert!(
            lines
                .iter()
                .any(|(l, v)| l == "Czas trwania" && v == "0:00:01"),
            "{lines:?}"
        );
        assert!(lines.iter().any(|(_, v)| v == "8000 Hz"), "{lines:?}");
        assert!(describe(&media.join("zdjecie-exif.jpg"))
            .iter()
            .any(|(_, v)| v == "Aparat Żółć"));
    }
}
