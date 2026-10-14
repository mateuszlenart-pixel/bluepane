//! Windows / Samba shares of a host, and opening a remote desktop. Both use the programs the
//! system already has (`smbclient` on Linux, `net view` and `mstsc` on Windows), because
//! speaking SMB ourselves is far more than a file manager needs.

use std::io;
use std::process::{Command, Stdio};

/// What a share is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareKind {
    Disk,
    Printer,
    /// `IPC$` and other service shares.
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub name: String,
    pub kind: ShareKind,
    pub comment: String,
}

impl Share {
    /// Administrative shares (`C$`, `ADMIN$`, `IPC$`) are not what people look for.
    pub fn is_hidden(&self) -> bool {
        self.name.ends_with('$')
    }
}

/// `smbclient -g -L` output: `Disk|name|comment`, `Printer|...`, `IPC|...`.
pub(crate) fn parse_smbclient(text: &str) -> Vec<Share> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let kind = match parts.next()?.trim() {
                "Disk" => ShareKind::Disk,
                "Printer" => ShareKind::Printer,
                "IPC" => ShareKind::Service,
                _ => return None,
            };
            let name = parts.next()?.trim();
            (!name.is_empty()).then(|| Share {
                name: name.to_string(),
                kind,
                comment: parts.next().unwrap_or("").trim().to_string(),
            })
        })
        .collect()
}

/// `net view \\host` output: a header, a line of dashes, then rows whose columns are
/// separated by two or more spaces (`name  Type  Used as  Comment`), in any language.
pub(crate) fn parse_net_view(text: &str) -> Vec<Share> {
    let mut rows = text
        .lines()
        .skip_while(|line| !(line.trim().len() >= 10 && line.trim().chars().all(|c| c == '-')))
        .skip(1);
    let mut shares = Vec::new();
    for line in rows.by_ref() {
        let columns: Vec<&str> = line
            .split("  ")
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        // The closing "The command completed successfully." has no columns.
        if columns.len() < 2 {
            break;
        }
        let kind_word = columns[1].to_lowercase();
        let kind = if kind_word.starts_with("print") || kind_word.starts_with("drukark") {
            ShareKind::Printer
        } else if columns[0].ends_with('$') {
            ShareKind::Service
        } else {
            ShareKind::Disk
        };
        shares.push(Share {
            name: columns[0].to_string(),
            kind,
            comment: columns
                .get(3)
                .or(columns.get(2))
                .copied()
                .unwrap_or("")
                .to_string(),
        });
    }
    shares
}

/// A message for the user from what the share-listing program said.
pub(crate) fn explain_failure(output: &str) -> String {
    let lower = output.to_lowercase();
    let needs_login =
        "serwer wymaga logowania: udziałów nie da się wylistować anonimowo".to_string();
    // The status codes of smbclient are exact; check them before the looser texts.
    if lower.contains("access_denied") || lower.contains("logon_failure") {
        needs_login
    } else if lower.contains("connection_refused")
        || lower.contains("connection to")
        || lower.contains("system error 53")
        || lower.contains("błąd systemu 53")
        || lower.contains("nie znaleziono ścieżki sieciowej")
        || lower.contains("network path was not found")
    {
        "serwer nie odpowiada na SMB (udziały wyłączone?)".into()
    } else if lower.contains("system error 5.")
        || lower.contains("system error 5 ")
        || lower.contains("błąd systemu 5.")
        || lower.contains("błąd systemu 5 ")
        || lower.contains("access is denied")
        || lower.contains("odmowa dostępu")
    {
        needs_login
    } else {
        let first = output
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("nieznany błąd");
        format!("nie udało się wylistować udziałów: {first}")
    }
}

/// The shares of `host` (an address or a name), as far as it lets us see them.
pub fn list_shares(host: &str) -> Result<Vec<Share>, String> {
    if host.is_empty() || host.contains(|c: char| c.is_whitespace() || "\\/\"'`;&|<>$".contains(c))
    {
        return Err("niepoprawny adres serwera".into());
    }
    let (output, parse): (_, fn(&str) -> Vec<Share>) = if cfg!(windows) {
        (
            run("net", &["view", &format!("\\\\{host}")]),
            parse_net_view,
        )
    } else {
        (
            run("smbclient", &["-L", &format!("//{host}"), "-N", "-g"]),
            parse_smbclient,
        )
    };
    match output {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(if cfg!(windows) {
            "brak polecenia `net`".into()
        } else {
            "zainstaluj `smbclient` (pakiet smbclient), żeby listować udziały".into()
        }),
        Err(e) => Err(format!("nie udało się uruchomić programu: {e}")),
        Ok((success, text)) => {
            let shares = parse(&text);
            if shares.is_empty() && !success {
                Err(explain_failure(&text))
            } else {
                Ok(shares)
            }
        }
    }
}

/// Runs a program and returns whether it succeeded and everything it printed.
fn run(program: &str, args: &[&str]) -> io::Result<(bool, String)> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    let out = command.output()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), text))
}

/// Opens a remote desktop to `host`: `mstsc` on Windows; `remmina` or `xfreerdp` on Linux.
pub fn open_remote_desktop(host: &str) -> io::Result<()> {
    if host.is_empty() || host.contains(|c: char| c.is_whitespace() || "\\/\"'`;&|<>$".contains(c))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "niepoprawny adres",
        ));
    }
    let attempts: Vec<(&str, Vec<String>)> = if cfg!(windows) {
        vec![("mstsc", vec![format!("/v:{host}")])]
    } else {
        vec![
            ("remmina", vec!["-c".into(), format!("rdp://{host}")]),
            ("xfreerdp", vec![format!("/v:{host}")]),
            ("xdg-open", vec![format!("rdp://{host}")]),
        ]
    };
    let mut last = io::Error::new(io::ErrorKind::NotFound, "brak programu do pulpitu zdalnego");
    for (program, args) in attempts {
        match Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(e) => last = e,
        }
    }
    Err(io::Error::new(
        last.kind(),
        "zainstaluj program do pulpitu zdalnego (Linux: remmina albo freerdp, Windows: mstsc jest w systemie)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smbclient_output_is_parsed() {
        let text = "\
Disk|soft|Shared documents
Disk|Public|
Printer|HP Color LaserJet|HP Color LaserJet 9500
IPC|IPC$|IPC Service (Samba 4.19)
Reconnecting with SMB1 for workgroup listing.
SMB1 disabled -- no workgroup available
";
        let shares = parse_smbclient(text);
        assert_eq!(shares.len(), 4);
        assert_eq!(
            shares[0],
            Share {
                name: "soft".into(),
                kind: ShareKind::Disk,
                comment: "Shared documents".into()
            }
        );
        assert_eq!(shares[1].comment, "");
        assert_eq!(shares[2].kind, ShareKind::Printer);
        assert!(shares[3].is_hidden() && shares[3].kind == ShareKind::Service);
        assert!(!shares[0].is_hidden());
    }

    #[test]
    fn net_view_output_is_parsed_in_english_and_polish() {
        let english = "\
Shared resources at \\\\192.168.0.5

Share name  Type  Used as  Comment

-------------------------------------------------------------------------------
soft        Disk           Shared docs
HP Laser    Print          Office printer
C$          Disk           Default share
The command completed successfully.
";
        let shares = parse_net_view(english);
        assert_eq!(shares.len(), 3);
        assert_eq!(shares[0].name, "soft");
        assert_eq!(shares[0].comment, "Shared docs");
        assert_eq!(
            shares[1],
            Share {
                name: "HP Laser".into(),
                kind: ShareKind::Printer,
                comment: "Office printer".into()
            }
        );
        assert_eq!(shares[2].kind, ShareKind::Service);

        let polish = "\
Udziały na \\\\192.168.0.5

Nazwa udziału  Typ        Użyte jako  Komentarz

-------------------------------------------------------------------------------
dane           Dysk                   Moje dane
Drukarka1      Drukarka               Biuro
Polecenie zostało wykonane pomyślnie.
";
        let shares = parse_net_view(polish);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "dane");
        assert_eq!(shares[1].kind, ShareKind::Printer);
        assert!(parse_net_view("nothing useful here").is_empty());
    }

    #[test]
    fn failures_are_explained() {
        assert!(
            explain_failure("session setup failed: NT_STATUS_ACCESS_DENIED").contains("logowania")
        );
        assert!(
            explain_failure("System error 5 has occurred.\nAccess is denied.")
                .contains("logowania")
        );
        assert!(explain_failure(
            "Connection to 10.0.0.9 failed (Error NT_STATUS_CONNECTION_REFUSED)"
        )
        .contains("nie odpowiada"));
        assert!(explain_failure("Błąd systemu 53.").contains("nie odpowiada"));
        assert!(explain_failure("\nsomething odd\n").contains("something odd"));
        // A login failure that mentions the connection is still a login problem.
        assert!(
            explain_failure("Connection to host failed (Error NT_STATUS_LOGON_FAILURE)")
                .contains("logowania")
        );
    }

    #[test]
    fn dangerous_host_names_are_refused() {
        for bad in ["", "a b", "x;rm", "a|b", "$(id)", "a/b", "\\\\x"] {
            assert!(list_shares(bad).is_err(), "{bad}");
            assert!(open_remote_desktop(bad).is_err(), "{bad}");
        }
    }
}
