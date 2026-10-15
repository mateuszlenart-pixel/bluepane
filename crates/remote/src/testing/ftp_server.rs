//! A small FTP / FTPS server over a local directory, for tests.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use crate::tls::fingerprint_of;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tls {
    None,
    /// `AUTH TLS` on the normal port.
    Explicit,
    /// TLS from the first byte.
    Implicit,
}

#[derive(Clone)]
pub struct FtpSettings {
    pub root: PathBuf,
    pub user: String,
    pub password: String,
    pub tls: Tls,
    /// Answer `MLSD` with an error so clients fall back to `LIST`.
    pub no_mlsd: bool,
}

pub struct FtpServer {
    pub port: u16,
    pub root: PathBuf,
    /// The certificate's fingerprint (FTPS servers).
    pub fingerprint: Option<String>,
}

/// Starts a server on a free local port; it lives until the test process ends.
pub fn start_ftp(settings: FtpSettings) -> FtpServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tls, fingerprint) = if settings.tls == Tls::None {
        (None, None)
    } else {
        let certified = rcgen::generate_simple_self_signed(vec![
            "127.0.0.1".to_string(),
            "localhost".to_string(),
        ])
        .unwrap();
        let der = certified.cert.der().clone();
        let fingerprint = fingerprint_of(der.as_ref());
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
            certified.signing_key.serialize_der(),
        ));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(der.to_vec())], key)
            .unwrap();
        // Unread session tickets make a client that closes right away reset the connection.
        config.send_tls13_tickets = 0;
        (Some(Arc::new(config)), Some(fingerprint))
    };
    let root = settings.root.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (settings, tls) = (settings.clone(), tls.clone());
            std::thread::spawn(move || {
                let _ = serve(stream, settings, tls);
            });
        }
    });
    FtpServer {
        port,
        root,
        fingerprint,
    }
}

/// A control or data connection, plain or inside TLS.
enum Wire {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ServerConnection, TcpStream>>),
}

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Wire::Plain(s) => s.read(buf),
            Wire::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Wire::Plain(s) => s.write(buf),
            Wire::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Wire::Plain(s) => s.flush(),
            Wire::Tls(s) => s.flush(),
        }
    }
}

impl Wire {
    fn secure(stream: TcpStream, config: &Arc<ServerConfig>) -> Wire {
        let connection = ServerConnection::new(config.clone()).unwrap();
        Wire::Tls(Box::new(StreamOwned::new(connection, stream)))
    }

    fn finish(mut self) {
        let _ = self.flush();
        if let Wire::Tls(tls) = &mut self {
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    }
}

fn read_line(wire: &mut Wire) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match wire.read(&mut byte) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
        }
    }
    Some(
        String::from_utf8_lossy(&line)
            .trim_end_matches('\r')
            .to_string(),
    )
}

fn reply(wire: &mut Wire, text: &str) {
    let _ = wire.write_all(format!("{text}\r\n").as_bytes());
    let _ = wire.flush();
}

struct State {
    root: PathBuf,
    cwd: String,
    rest: u64,
    rename_from: Option<String>,
    pasv: Option<TcpListener>,
    /// Where the client asked the server to connect for data (`PORT` / `EPRT`).
    active: Option<std::net::SocketAddr>,
    protected: bool,
}

impl State {
    fn remote(&self, path: &str) -> String {
        let joined = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{path}", self.cwd.trim_end_matches('/'))
        };
        let mut parts: Vec<&str> = Vec::new();
        for part in joined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        format!("/{}", parts.join("/"))
    }

    fn local(&self, path: &str) -> PathBuf {
        let mut local = self.root.clone();
        for part in self.remote(path).split('/').filter(|p| !p.is_empty()) {
            local.push(part);
        }
        local
    }
}

fn stamp(time: SystemTime) -> String {
    // YYYYMMDDHHMMSS in UTC, from days since the epoch.
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (z, era_day) = (days + 719_468, 0);
    let _ = era_day;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn serve(
    stream: TcpStream,
    settings: FtpSettings,
    tls: Option<Arc<ServerConfig>>,
) -> std::io::Result<()> {
    let mut control = match (&tls, settings.tls) {
        (Some(config), Tls::Implicit) => Wire::secure(stream, config),
        _ => Wire::Plain(stream),
    };
    let mut state = State {
        root: settings.root.clone(),
        cwd: "/".into(),
        rest: 0,
        rename_from: None,
        pasv: None,
        active: None,
        protected: settings.tls == Tls::Implicit,
    };
    let mut user_ok = false;
    let mut logged_in = false;
    reply(&mut control, "220 bluepane test server");
    while let Some(line) = read_line(&mut control) {
        let (command, argument) = match line.split_once(' ') {
            Some((c, a)) => (c.to_ascii_uppercase(), a.to_string()),
            None => (line.to_ascii_uppercase(), String::new()),
        };
        match command.as_str() {
            "USER" => {
                user_ok = argument == settings.user;
                reply(&mut control, "331 password please");
            }
            "PASS" => {
                if user_ok && argument == settings.password {
                    logged_in = true;
                    reply(&mut control, "230 logged in");
                } else {
                    reply(&mut control, "530 login incorrect");
                }
            }
            "AUTH" if settings.tls == Tls::Explicit => {
                reply(&mut control, "234 go ahead");
                let Wire::Plain(tcp) = control else { return Ok(()) };
                control = Wire::secure(tcp, tls.as_ref().unwrap());
            }
            "PBSZ" => reply(&mut control, "200 ok"),
            "PROT" => {
                state.protected = argument.eq_ignore_ascii_case("P");
                reply(&mut control, "200 ok");
            }
            "SYST" => reply(&mut control, "215 UNIX Type: L8"),
            "FEAT" => reply(
                &mut control,
                "211-Features:\r\n MLST type*;size*;modify*;\r\n SIZE\r\n MDTM\r\n REST STREAM\r\n UTF8\r\n AUTH TLS\r\n PBSZ\r\n PROT\r\n211 End",
            ),
            "OPTS" => reply(&mut control, "200 ok"),
            "NOOP" => reply(&mut control, "200 ok"),
            "QUIT" => {
                reply(&mut control, "221 bye");
                break;
            }
            _ if !logged_in => reply(&mut control, "530 log in first"),
            "TYPE" => reply(&mut control, "200 type set"),
            "PWD" => reply(&mut control, &format!("257 \"{}\" is the current directory", state.cwd)),
            "CWD" => {
                let target = state.remote(&argument);
                if state.local(&argument).is_dir() {
                    state.cwd = target;
                    reply(&mut control, "250 ok");
                } else {
                    reply(&mut control, "550 no such directory");
                }
            }
            "CDUP" => {
                state.cwd = state.remote("..");
                reply(&mut control, "250 ok");
            }
            "SITE" => match argument.split_once(' ') {
                Some((word, rest)) if word.eq_ignore_ascii_case("CHMOD") => {
                    match rest.split_once(' ').and_then(|(mode, path)| {
                        u32::from_str_radix(mode, 8).ok().map(|m| (m, path))
                    }) {
                        Some((mode, path)) if state.local(path).exists() => {
                            super::record_chmod(state.local(path), mode);
                            reply(&mut control, "200 SITE CHMOD ok");
                        }
                        _ => reply(&mut control, "550 chmod failed"),
                    }
                }
                _ => reply(&mut control, "500 unknown SITE command"),
            },
            "MKD" => match fs::create_dir(state.local(&argument)) {
                Ok(()) => reply(&mut control, &format!("257 \"{}\" created", state.remote(&argument))),
                Err(_) => reply(&mut control, "550 cannot create"),
            },
            "RMD" => match fs::remove_dir(state.local(&argument)) {
                Ok(()) => reply(&mut control, "250 removed"),
                Err(_) => reply(&mut control, "550 cannot remove"),
            },
            "DELE" => match fs::remove_file(state.local(&argument)) {
                Ok(()) => reply(&mut control, "250 deleted"),
                Err(_) => reply(&mut control, "550 cannot delete"),
            },
            "RNFR" => {
                if state.local(&argument).exists() {
                    state.rename_from = Some(argument);
                    reply(&mut control, "350 ready for RNTO");
                } else {
                    reply(&mut control, "550 no such file");
                }
            }
            "RNTO" => {
                let from = state.rename_from.take();
                match from.map(|f| fs::rename(state.local(&f), state.local(&argument))) {
                    Some(Ok(())) => reply(&mut control, "250 renamed"),
                    _ => reply(&mut control, "550 cannot rename"),
                }
            }
            "SIZE" => match fs::metadata(state.local(&argument)) {
                Ok(meta) if meta.is_file() => reply(&mut control, &format!("213 {}", meta.len())),
                _ => reply(&mut control, "550 not a file"),
            },
            "MDTM" => match fs::metadata(state.local(&argument)).and_then(|m| m.modified()) {
                Ok(time) => reply(&mut control, &format!("213 {}", stamp(time))),
                Err(_) => reply(&mut control, "550 no such file"),
            },
            "REST" => {
                state.rest = argument.parse().unwrap_or(0);
                reply(&mut control, "350 restarting");
            }
            "PORT" => {
                let n: Vec<u16> = argument.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if n.len() == 6 && n.iter().all(|v| *v < 256) {
                    let ip = std::net::Ipv4Addr::new(n[0] as u8, n[1] as u8, n[2] as u8, n[3] as u8);
                    state.active = Some(std::net::SocketAddr::from((ip, n[4] * 256 + n[5])));
                    state.pasv = None;
                    reply(&mut control, "200 PORT ok");
                } else {
                    reply(&mut control, "501 bad PORT");
                }
            }
            "EPRT" => {
                // |1|127.0.0.1|34567|
                let parts: Vec<&str> = argument.split('|').collect();
                match (parts.get(2).and_then(|a| a.parse().ok()), parts.get(3).and_then(|p| p.parse().ok())) {
                    (Some(ip), Some(port)) => {
                        let ip: std::net::IpAddr = ip;
                        state.active = Some(std::net::SocketAddr::new(ip, port));
                        state.pasv = None;
                        reply(&mut control, "200 EPRT ok");
                    }
                    _ => reply(&mut control, "501 bad EPRT"),
                }
            }
            "PASV" | "EPSV" => {
                state.active = None;
                let listener = TcpListener::bind("127.0.0.1:0")?;
                let port = listener.local_addr()?.port();
                state.pasv = Some(listener);
                if command == "PASV" {
                    reply(
                        &mut control,
                        &format!("227 Entering Passive Mode (127,0,0,1,{},{})", port / 256, port % 256),
                    );
                } else {
                    reply(&mut control, &format!("229 Entering Extended Passive Mode (|||{port}|)"));
                }
            }
            "LIST" | "MLSD" | "RETR" | "STOR" | "APPE" => {
                let rest = std::mem::take(&mut state.rest);
                data_command(&mut control, &mut state, &command, &argument, rest, &settings, &tls);
            }
            _ => reply(&mut control, "502 not implemented"),
        }
    }
    Ok(())
}

fn data_command(
    control: &mut Wire,
    state: &mut State,
    command: &str,
    argument: &str,
    rest: u64,
    settings: &FtpSettings,
    tls: &Option<Arc<ServerConfig>>,
) {
    let path = state.local(argument);
    // Check what can be answered before the data connection is opened.
    match command {
        "MLSD" if settings.no_mlsd => return reply(control, "500 MLSD not supported"),
        "LIST" | "MLSD" if !path.is_dir() => return reply(control, "550 no such directory"),
        "RETR" if !path.is_file() => return reply(control, "550 no such file"),
        _ => {}
    }
    let tcp = if let Some(address) = state.active.take() {
        reply(control, "150 opening data connection");
        match TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
            Ok(tcp) => {
                super::record_active_transfer(state.root.clone());
                tcp
            }
            Err(_) => return reply(control, "425 cannot connect to client"),
        }
    } else {
        let Some(listener) = state.pasv.take() else {
            return reply(control, "425 use PASV or PORT first");
        };
        reply(control, "150 opening data connection");
        let _ = listener.set_nonblocking(true);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((tcp, _)) => break tcp,
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => return reply(control, "425 no data connection"),
            }
        }
    };
    let _ = tcp.set_nonblocking(false);
    let mut data = match tls {
        Some(config) if state.protected => Wire::secure(tcp, config),
        _ => Wire::Plain(tcp),
    };
    let ok = match command {
        "LIST" | "MLSD" => {
            let mut text = String::new();
            let mut entries: Vec<_> = fs::read_dir(&path).unwrap().flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let meta = entry.metadata().unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                let kind = if meta.is_dir() { "dir" } else { "file" };
                let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                if command == "MLSD" {
                    text.push_str(&format!(
                        "type={kind};size={};modify={}; {name}\r\n",
                        meta.len(),
                        stamp(modified)
                    ));
                } else {
                    let stamp = stamp(modified);
                    let months = [
                        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
                        "Nov", "Dec",
                    ];
                    let month = months[stamp[4..6].parse::<usize>().unwrap() - 1];
                    text.push_str(&format!(
                        "{}rw-r--r-- 1 user group {} {month} {} {}:{} {name}\r\n",
                        if meta.is_dir() { "d" } else { "-" },
                        meta.len(),
                        stamp[6..8].trim_start_matches('0'),
                        &stamp[8..10],
                        &stamp[10..12],
                    ));
                }
            }
            data.write_all(text.as_bytes()).is_ok()
        }
        "RETR" => {
            let mut file = fs::File::open(&path).unwrap();
            let _ = file.seek(SeekFrom::Start(rest));
            std::io::copy(&mut file, &mut data).is_ok()
        }
        _ => {
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true);
            if command == "APPE" {
                options.append(true);
            } else {
                options.truncate(rest == 0);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    if command == "STOR" && rest > 0 {
                        let _ = file.seek(SeekFrom::Start(rest));
                    }
                    // Many clients close a TLS data connection without close_notify.
                    match std::io::copy(&mut data, &mut file) {
                        Ok(_) => true,
                        Err(e) => e.kind() == std::io::ErrorKind::UnexpectedEof,
                    }
                }
                Err(_) => false,
            }
        }
    };
    data.finish();
    reply(
        control,
        if ok {
            "226 transfer complete"
        } else {
            "451 transfer failed"
        },
    );
}
