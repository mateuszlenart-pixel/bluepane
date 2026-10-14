//! Remembered server keys, one per line: `host:port algorithm fingerprint`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    Known,
    Unknown,
    /// A different key is remembered for this host.
    Changed {
        expected: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownHosts {
    path: Option<PathBuf>,
    /// (host:port, algorithm, fingerprint)
    entries: Vec<(String, String, String)>,
}

fn id(host: &str, port: u16) -> String {
    format!("{}:{port}", host.to_lowercase())
}

impl KnownHosts {
    /// An empty list that is not stored anywhere (tests, one-off connections).
    pub fn in_memory() -> KnownHosts {
        KnownHosts::default()
    }

    /// Reads the list; a missing file is an empty list.
    pub fn load(path: &Path) -> io::Result<KnownHosts> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let entries = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                Some((
                    parts.next()?.to_string(),
                    parts.next()?.to_string(),
                    parts.next()?.to_string(),
                ))
            })
            .collect();
        Ok(KnownHosts {
            path: Some(path.to_path_buf()),
            entries,
        })
    }

    pub fn status(
        &self,
        host: &str,
        port: u16,
        algorithm: &str,
        fingerprint: &str,
    ) -> HostKeyStatus {
        let wanted = id(host, port);
        let mut same_algorithm = None;
        for (entry, algo, fp) in &self.entries {
            if *entry != wanted {
                continue;
            }
            if algo == algorithm {
                if fp == fingerprint {
                    return HostKeyStatus::Known;
                }
                same_algorithm = Some(fp.clone());
            }
        }
        match same_algorithm {
            Some(expected) => HostKeyStatus::Changed { expected },
            None => HostKeyStatus::Unknown,
        }
    }

    /// Remembers a key (and writes the list, when it came from a file).
    pub fn add(
        &mut self,
        host: &str,
        port: u16,
        algorithm: &str,
        fingerprint: &str,
    ) -> io::Result<()> {
        let entry = (
            id(host, port),
            algorithm.to_string(),
            fingerprint.to_string(),
        );
        if !self.entries.contains(&entry) {
            self.entries.push(entry);
        }
        self.save()
    }

    /// Forgets every key of a host (the user decided to trust a changed key).
    pub fn forget(&mut self, host: &str, port: u16) -> io::Result<()> {
        let wanted = id(host, port);
        self.entries.retain(|(entry, _, _)| *entry != wanted);
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text: String = self
            .entries
            .iter()
            .map(|(host, algo, fp)| format!("{host} {algo} {fp}\n"))
            .collect();
        fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_known_and_changed() {
        let mut hosts = KnownHosts::in_memory();
        assert_eq!(
            hosts.status("Example.com", 22, "ssh-ed25519", "SHA256:aaa"),
            HostKeyStatus::Unknown
        );
        hosts
            .add("example.com", 22, "ssh-ed25519", "SHA256:aaa")
            .unwrap();
        assert_eq!(
            hosts.status("EXAMPLE.com", 22, "ssh-ed25519", "SHA256:aaa"),
            HostKeyStatus::Known
        );
        assert_eq!(
            hosts.status("example.com", 22, "ssh-ed25519", "SHA256:bbb"),
            HostKeyStatus::Changed {
                expected: "SHA256:aaa".into()
            }
        );
        // Another port or another algorithm is a different case.
        assert_eq!(
            hosts.status("example.com", 2222, "ssh-ed25519", "SHA256:bbb"),
            HostKeyStatus::Unknown
        );
        assert_eq!(
            hosts.status("example.com", 22, "ssh-rsa", "SHA256:bbb"),
            HostKeyStatus::Unknown
        );
        hosts.forget("example.com", 22).unwrap();
        assert_eq!(
            hosts.status("example.com", 22, "ssh-ed25519", "SHA256:bbb"),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn the_list_is_stored_in_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("conf/known_hosts");
        let mut hosts = KnownHosts::load(&path).unwrap();
        hosts.add("h", 22, "ssh-ed25519", "SHA256:x").unwrap();
        let again = KnownHosts::load(&path).unwrap();
        assert_eq!(
            again.status("h", 22, "ssh-ed25519", "SHA256:x"),
            HostKeyStatus::Known
        );
    }
}
