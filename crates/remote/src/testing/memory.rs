//! A password store and a Bitwarden CLI that live in memory, for tests.

use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

use crate::secrets::{BitwardenCli, CliOutput, KeyStore, SecretServices};

#[derive(Debug, Default)]
pub struct MemoryKeys {
    map: Mutex<HashMap<(String, String), String>>,
    /// When set, every call fails with this text (a locked or missing store).
    pub broken: Mutex<Option<String>>,
}

impl MemoryKeys {
    fn check(&self) -> Result<(), String> {
        match &*self.broken.lock().unwrap() {
            Some(why) => Err(why.clone()),
            None => Ok(()),
        }
    }
}

impl KeyStore for MemoryKeys {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String> {
        self.check()?;
        Ok(self
            .map
            .lock()
            .unwrap()
            .get(&(service.into(), user.into()))
            .cloned())
    }

    fn set(&self, service: &str, user: &str, password: &str) -> Result<(), String> {
        self.check()?;
        self.map
            .lock()
            .unwrap()
            .insert((service.into(), user.into()), password.into());
        Ok(())
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), String> {
        self.check()?;
        self.map
            .lock()
            .unwrap()
            .remove(&(service.into(), user.into()));
        Ok(())
    }
}

/// `bw` with a vault of `name -> password`.
#[derive(Debug)]
pub struct FakeBitwarden {
    pub installed: bool,
    pub locked: bool,
    pub items: Mutex<HashMap<String, String>>,
    /// The JSON of every item created, in order (decoded from the base64 argument).
    pub created: Mutex<Vec<String>>,
}

impl Default for FakeBitwarden {
    fn default() -> Self {
        FakeBitwarden {
            installed: true,
            locked: false,
            items: Mutex::default(),
            created: Mutex::default(),
        }
    }
}

impl FakeBitwarden {
    pub fn with_item(self, name: &str, password: &str) -> Self {
        self.items
            .lock()
            .unwrap()
            .insert(name.into(), password.into());
        self
    }

    pub fn password_of(&self, name: &str) -> Option<String> {
        self.items.lock().unwrap().get(name).cloned()
    }
}

fn ok(stdout: &str) -> io::Result<CliOutput> {
    Ok(CliOutput {
        success: true,
        stdout: stdout.into(),
        stderr: String::new(),
    })
}

fn failed(stderr: &str) -> io::Result<CliOutput> {
    Ok(CliOutput {
        success: false,
        stdout: String::new(),
        stderr: stderr.into(),
    })
}

/// The first string value of `"key":"..."` in a JSON text (enough for the item bluepane writes).
fn json_value(json: &str, key: &str) -> Option<String> {
    let start = json.find(&format!("\"{key}\":\""))? + key.len() + 4;
    let mut out = String::new();
    let mut chars = json[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn unbase64(text: &str) -> String {
    let value =
        |c: char| "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".find(c);
    let mut bytes = Vec::new();
    let digits: Vec<u32> = text
        .chars()
        .filter_map(|c| value(c).map(|v| v as u32))
        .collect();
    for chunk in digits.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, d)| n | d << (18 - 6 * i));
        for i in 0..chunk.len().saturating_sub(1) {
            bytes.push((n >> (16 - 8 * i)) as u8);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

impl BitwardenCli for FakeBitwarden {
    fn run(&self, args: &[&str]) -> io::Result<CliOutput> {
        if !self.installed {
            return Err(io::Error::new(io::ErrorKind::NotFound, "bw: not found"));
        }
        if args == ["--version"] {
            return ok("2025.1.0");
        }
        if self.locked {
            return failed("Vault is locked.");
        }
        match args {
            ["get", "password", name, ..] => match self.items.lock().unwrap().get(*name) {
                Some(password) => ok(&format!("{password}\n")),
                None => failed("Not found."),
            },
            ["get", "item", name, ..] => match self.items.lock().unwrap().get(*name) {
                Some(_) => ok("{}"),
                None => failed("Not found."),
            },
            ["create", "item", encoded, ..] => {
                let json = unbase64(encoded);
                let (Some(name), Some(password)) =
                    (json_value(&json, "name"), json_value(&json, "password"))
                else {
                    return failed("Invalid item.");
                };
                self.items.lock().unwrap().insert(name, password);
                self.created.lock().unwrap().push(json);
                ok("{}")
            }
            _ => failed("unknown command"),
        }
    }
}

#[derive(Debug, Default)]
pub struct MemoryServices {
    pub keys: MemoryKeys,
    pub bitwarden: FakeBitwarden,
}

impl SecretServices for MemoryServices {
    fn keys(&self) -> &dyn KeyStore {
        &self.keys
    }

    fn bitwarden(&self) -> &dyn BitwardenCli {
        &self.bitwarden
    }
}
