//! Split volumes: reading several files as one stream and writing one stream as several files.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

struct Part {
    path: PathBuf,
    start: u64,
    len: u64,
}

/// Several files read as one: `Read + Seek` over their concatenation.
pub struct Joined {
    parts: Vec<Part>,
    total: u64,
    pos: u64,
    open: Option<(usize, File)>,
}

impl Joined {
    pub fn open(paths: &[PathBuf]) -> io::Result<Joined> {
        let mut parts = Vec::with_capacity(paths.len());
        let mut start = 0;
        for path in paths {
            let len = std::fs::metadata(path)?.len();
            parts.push(Part {
                path: path.clone(),
                start,
                len,
            });
            start += len;
        }
        Ok(Joined {
            parts,
            total: start,
            pos: 0,
            open: None,
        })
    }

    fn part_at(&self, pos: u64) -> Option<usize> {
        // The last part whose start is at or before `pos` (skipping empty parts).
        let index = self.parts.partition_point(|p| p.start + p.len <= pos);
        (index < self.parts.len()).then_some(index)
    }
}

impl Read for Joined {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.total {
            return Ok(0);
        }
        let Some(index) = self.part_at(self.pos) else {
            return Ok(0);
        };
        let (start, len) = (self.parts[index].start, self.parts[index].len);
        if !matches!(&self.open, Some((open, _)) if *open == index) {
            let mut file = File::open(&self.parts[index].path)?;
            file.seek(SeekFrom::Start(self.pos - start))?;
            self.open = Some((index, file));
        }
        let room = (start + len - self.pos).min(buf.len() as u64) as usize;
        let (_, file) = self.open.as_mut().expect("opened above");
        let n = file.read(&mut buf[..room])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Joined {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.total) + i128::from(n),
            SeekFrom::Current(n) => i128::from(self.pos) + i128::from(n),
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the start of the archive",
            ));
        }
        self.pos = target as u64;
        // The open file's cursor no longer matches: reopen on the next read.
        self.open = None;
        Ok(self.pos)
    }
}

/// Writes a stream as numbered volumes `base.001`, `base.002`, ... of at most `part_size`
/// bytes each.
pub struct SplitWriter {
    base: PathBuf,
    part_size: u64,
    current: Option<File>,
    in_current: u64,
    parts: Vec<PathBuf>,
}

impl SplitWriter {
    pub fn new(base: &Path, part_size: u64) -> SplitWriter {
        SplitWriter {
            base: base.to_path_buf(),
            part_size: part_size.max(1),
            current: None,
            in_current: 0,
            parts: Vec::new(),
        }
    }

    /// The volumes written so far.
    pub fn parts(&self) -> &[PathBuf] {
        &self.parts
    }

    fn next_part(&mut self) -> io::Result<()> {
        let name = self
            .base
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let path = self
            .base
            .with_file_name(format!("{name}.{:03}", self.parts.len() + 1));
        self.current = Some(File::create(&path)?);
        self.parts.push(path);
        self.in_current = 0;
        Ok(())
    }
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.current.is_none() || self.in_current >= self.part_size {
            self.next_part()?;
        }
        let room = (self.part_size - self.in_current).min(buf.len() as u64) as usize;
        let file = self.current.as_mut().expect("a part is open");
        let n = file.write(&buf[..room])?;
        self.in_current += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.current {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_parts(dir: &Path, sizes: &[usize]) -> Vec<PathBuf> {
        let mut next = 0u8;
        sizes
            .iter()
            .enumerate()
            .map(|(i, &size)| {
                let path = dir.join(format!("p.{:03}", i + 1));
                let bytes: Vec<u8> = (0..size)
                    .map(|_| {
                        next = next.wrapping_add(1);
                        next
                    })
                    .collect();
                std::fs::write(&path, bytes).unwrap();
                path
            })
            .collect()
    }

    #[test]
    fn joined_reads_across_part_boundaries() {
        let tmp = tempfile::tempdir().unwrap();
        let parts = write_parts(tmp.path(), &[10, 7, 0, 5]);
        let mut joined = Joined::open(&parts).unwrap();
        assert_eq!(joined.total, 22);
        let mut all = Vec::new();
        joined.read_to_end(&mut all).unwrap();
        assert_eq!(all, (1..=22u8).collect::<Vec<_>>());
    }

    #[test]
    fn joined_seeks_anywhere_and_from_the_end() {
        let tmp = tempfile::tempdir().unwrap();
        let parts = write_parts(tmp.path(), &[10, 7, 5]);
        let mut joined = Joined::open(&parts).unwrap();
        let mut buf = [0u8; 4];
        joined.seek(SeekFrom::Start(8)).unwrap();
        joined.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [9, 10, 11, 12]);
        joined.seek(SeekFrom::End(-3)).unwrap();
        let mut tail = Vec::new();
        joined.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, [20, 21, 22]);
        joined.seek(SeekFrom::Current(-5)).unwrap();
        joined.read_exact(&mut buf[..2]).unwrap();
        assert_eq!(&buf[..2], [18, 19]);
        assert!(joined.seek(SeekFrom::Current(-100)).is_err());
        joined.seek(SeekFrom::Start(500)).unwrap();
        assert_eq!(joined.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn a_missing_part_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(Joined::open(&[tmp.path().join("nope.001")]).is_err());
    }

    #[test]
    fn split_writer_cuts_volumes_at_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let mut writer = SplitWriter::new(&tmp.path().join("a.bin"), 10);
        writer.write_all(&(0..25u8).collect::<Vec<_>>()).unwrap();
        writer.flush().unwrap();
        let names: Vec<_> = writer
            .parts()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.bin.001", "a.bin.002", "a.bin.003"]);
        let sizes: Vec<_> = writer
            .parts()
            .iter()
            .map(|p| std::fs::metadata(p).unwrap().len())
            .collect();
        assert_eq!(sizes, [10, 10, 5]);
    }

    #[test]
    fn written_volumes_read_back_as_the_original_stream() {
        let tmp = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut writer = SplitWriter::new(&tmp.path().join("d"), 64);
        // Awkward write sizes, including ones that straddle a boundary.
        for chunk in data.chunks(37) {
            writer.write_all(chunk).unwrap();
        }
        let mut joined = Joined::open(writer.parts()).unwrap();
        let mut back = Vec::new();
        joined.read_to_end(&mut back).unwrap();
        assert_eq!(back, data);
        assert_eq!(writer.parts().len(), 16);
    }

    #[test]
    fn writing_nothing_creates_no_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let mut writer = SplitWriter::new(&tmp.path().join("e"), 10);
        writer.flush().unwrap();
        assert!(writer.parts().is_empty());
    }
}
