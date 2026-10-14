//! A small in-process SSH server with an SFTP subsystem over a local directory, so the
//! tests need no real server and no `sshd`.

#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use russh::keys::{Algorithm, HashAlg, PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Settings {
    pub root: PathBuf,
    pub user: String,
    pub password: String,
    pub authorized_key: Option<PublicKey>,
}

pub struct TestServer {
    pub port: u16,
    pub root: PathBuf,
    pub fingerprint: String,
    pub algorithm: String,
}

/// Starts a server on a free local port; it lives until the test process ends.
pub fn start(settings: Settings) -> TestServer {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
    let algorithm = key.public_key().algorithm().to_string();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let root = settings.root.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let config = russh::server::Config {
                auth_rejection_time: Duration::from_millis(10),
                auth_rejection_time_initial: Some(Duration::from_millis(0)),
                keys: vec![key],
                ..Default::default()
            };
            let mut server = Server { settings };
            let _ = server
                .run_on_address(Arc::new(config), ("127.0.0.1", port))
                .await;
        });
    });
    for _ in 0..200 {
        if TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(50),
        )
        .is_ok()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    TestServer {
        port,
        root,
        fingerprint,
        algorithm,
    }
}

#[derive(Clone)]
struct Server {
    settings: Settings,
}

impl russh::server::Server for Server {
    type Handler = SshSession;

    fn new_client(&mut self, _: Option<SocketAddr>) -> SshSession {
        SshSession {
            settings: self.settings.clone(),
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

struct SshSession {
    settings: Settings,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl russh::server::Handler for SshSession {
    type Error = anyhow_free::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == self.settings.user && password == self.settings.password {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        match &self.settings.authorized_key {
            Some(allowed) if user == self.settings.user && allowed == key => Ok(Auth::Accept),
            _ => Ok(Auth::reject()),
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().await.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            let channel = self.channels.lock().await.remove(&channel_id).unwrap();
            session.channel_success(channel_id)?;
            let handler = Files {
                root: self.settings.root.clone(),
                handles: HashMap::new(),
                next: 0,
            };
            russh_sftp::server::run(channel.into_stream(), handler).await;
        } else {
            session.channel_failure(channel_id)?;
        }
        Ok(())
    }
}

/// `russh`'s own error type is enough here.
mod anyhow_free {
    pub type Error = russh::Error;
}

enum Open {
    File(fs::File),
    Dir { entries: Vec<File>, sent: bool },
}

struct Files {
    root: PathBuf,
    handles: HashMap<String, Open>,
    next: u32,
}

fn status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".into(),
        language_tag: "en-US".into(),
    }
}

fn code(error: &std::io::Error) -> StatusCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

fn attrs_of(meta: &fs::Metadata) -> FileAttributes {
    let mut attrs = FileAttributes::default();
    attrs.size = Some(if meta.is_dir() { 0 } else { meta.len() });
    attrs.permissions = Some(if meta.is_dir() {
        0o755 | 0x4000
    } else {
        0o644 | 0x8000
    });
    attrs.mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as u32);
    attrs.atime = attrs.mtime;
    attrs
}

impl Files {
    /// `/a/b` on the server is `root/a/b` on disk; `..` cannot leave the root.
    fn local(&self, remote: &str) -> PathBuf {
        let mut path = self.root.clone();
        for part in remote.split('/') {
            match part {
                "" | "." | ".." => {}
                other => path.push(other),
            }
        }
        path
    }

    fn new_handle(&mut self, open: Open) -> String {
        self.next += 1;
        let handle = format!("h{}", self.next);
        self.handles.insert(handle.clone(), open);
        handle
    }
}

impl russh_sftp::server::Handler for Files {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, StatusCode> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        Ok(Name {
            id,
            files: vec![File::dummy(format!("/{}", parts.join("/")))],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let meta = fs::metadata(self.local(&path)).map_err(|e| code(&e))?;
        Ok(Attrs {
            id,
            attrs: attrs_of(&meta),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let meta = fs::symlink_metadata(self.local(&path)).map_err(|e| code(&e))?;
        Ok(Attrs {
            id,
            attrs: attrs_of(&meta),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, StatusCode> {
        match self.handles.get(&handle) {
            Some(Open::File(file)) => Ok(Attrs {
                id,
                attrs: attrs_of(&file.metadata().map_err(|e| code(&e))?),
            }),
            _ => Err(StatusCode::Failure),
        }
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        if let Some(mtime) = attrs.mtime {
            let file = fs::OpenOptions::new()
                .write(true)
                .open(self.local(&path))
                .map_err(|e| code(&e))?;
            let _ = file.set_modified(UNIX_EPOCH + Duration::from_secs(u64::from(mtime)));
        }
        Ok(status(id))
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let mut options = fs::OpenOptions::new();
        options
            .read(flags.contains(OpenFlags::READ))
            .write(flags.contains(OpenFlags::WRITE))
            .create(flags.contains(OpenFlags::CREATE))
            .truncate(flags.contains(OpenFlags::TRUNCATE));
        let file = options.open(self.local(&filename)).map_err(|e| code(&e))?;
        Ok(Handle {
            id,
            handle: self.new_handle(Open::File(file)),
        })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, StatusCode> {
        self.handles.remove(&handle);
        Ok(status(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset)).map_err(|e| code(&e))?;
        let mut buffer = vec![0u8; len as usize];
        let n = file.read(&mut buffer).map_err(|e| code(&e))?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        buffer.truncate(n);
        Ok(Data { id, data: buffer })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset)).map_err(|e| code(&e))?;
        file.write_all(&data).map_err(|e| code(&e))?;
        Ok(status(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, StatusCode> {
        let mut entries = Vec::new();
        for item in fs::read_dir(self.local(&path)).map_err(|e| code(&e))? {
            let item = item.map_err(|e| code(&e))?;
            let meta = fs::symlink_metadata(item.path()).map_err(|e| code(&e))?;
            entries.push(File {
                filename: item.file_name().to_string_lossy().into_owned(),
                longname: String::new(),
                attrs: attrs_of(&meta),
            });
        }
        Ok(Handle {
            id,
            handle: self.new_handle(Open::Dir {
                entries,
                sent: false,
            }),
        })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, StatusCode> {
        match self.handles.get_mut(&handle) {
            Some(Open::Dir { entries, sent }) => {
                if *sent {
                    return Err(StatusCode::Eof);
                }
                *sent = true;
                Ok(Name {
                    id,
                    files: std::mem::take(entries),
                })
            }
            _ => Err(StatusCode::Failure),
        }
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, StatusCode> {
        fs::remove_file(self.local(&filename)).map_err(|e| code(&e))?;
        Ok(status(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        fs::create_dir(self.local(&path)).map_err(|e| code(&e))?;
        Ok(status(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        fs::remove_dir(self.local(&path)).map_err(|e| code(&e))?;
        Ok(status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, StatusCode> {
        fs::rename(self.local(&oldpath), self.local(&newpath)).map_err(|e| code(&e))?;
        Ok(status(id))
    }
}

pub fn write_file(path: impl AsRef<Path>, content: impl AsRef<[u8]>) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
