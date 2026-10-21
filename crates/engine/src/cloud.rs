//! Clouds through `rclone` (`cloud`): Google Drive, OneDrive, S3, WebDAV and the rest of what
//! rclone is configured for show up as a folder in a panel, like a server. bluepane keeps no
//! cloud logins of its own: they live in rclone (`rclone config`). `cloud` lists the clouds
//! rclone knows, `cloud <name>` opens one (and saves it as a site named `<name> (rclone)`),
//! and `go rclone://<name>/<folder>` does the same from the path bar.

use bluepane_remote::{list_remotes, Protocol, SecretStore, Site};

use crate::session::Session;
use crate::EngineError;

/// What `cloud` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudAction {
    /// Say which clouds rclone knows.
    List,
    /// Open this one in the active panel.
    Open(String),
}

/// The site that stands for a cloud.
fn site_name(cloud: &str) -> String {
    format!("{cloud} (rclone)")
}

impl Session {
    pub(crate) fn cloud_action(&mut self, action: CloudAction) -> Result<(), EngineError> {
        match action {
            CloudAction::List => {
                let remotes =
                    list_remotes(&*self.rclone).map_err(|e| EngineError(e.to_string()))?;
                self.status = if remotes.is_empty() {
                    "rclone nie ma skonfigurowanych chmur: dodaj je poleceniem `rclone config`"
                        .to_string()
                } else {
                    let names: Vec<String> = remotes
                        .iter()
                        .map(|r| format!("{} ({})", r.name, r.kind))
                        .collect();
                    format!(
                        "Chmury z rclone: {} (cloud <nazwa> otwiera)",
                        names.join(", ")
                    )
                };
                Ok(())
            }
            CloudAction::Open(name) => self.cloud_connect(&name, None),
        }
    }

    /// Opens the cloud `name`, in `dir` when one is given, saving the site on first use.
    pub(crate) fn cloud_connect(
        &mut self,
        name: &str,
        dir: Option<String>,
    ) -> Result<(), EngineError> {
        self.load_config();
        let site_name = site_name(name);
        if self.sites.get(&site_name).is_none() {
            // Only a cloud rclone really has becomes a site.
            let remotes = list_remotes(&*self.rclone).map_err(|e| EngineError(e.to_string()))?;
            if !remotes.iter().any(|r| r.name == name) {
                let known: Vec<&str> = remotes.iter().map(|r| r.name.as_str()).collect();
                return Err(EngineError(format!(
                    "rclone nie zna chmury „{name}” ({}); dodaj ją poleceniem `rclone config`",
                    if known.is_empty() {
                        "brak skonfigurowanych".to_string()
                    } else {
                        format!("są: {}", known.join(", "))
                    }
                )));
            }
            let mut site = Site::new(site_name.clone(), name, "");
            site.protocol = Protocol::Rclone;
            site.port = Protocol::Rclone.default_port();
            site.secret = SecretStore::Ask;
            if let Some(problem) = site.problem() {
                return Err(EngineError(format!("chmura „{name}”: {problem}")));
            }
            self.sites
                .upsert(site)
                .map_err(|e| EngineError(format!("nie udało się zapisać witryny: {e}")))?;
        }
        self.after_connect = dir.map(|dir| (site_name.clone(), dir));
        self.connect_site(&site_name, None)
    }
}

/// `cloud [list|<nazwa>]`
pub(crate) fn parse_cloud(rest: &str) -> Result<CloudAction, EngineError> {
    let rest = rest.trim();
    match rest.to_lowercase().as_str() {
        "" | "list" => Ok(CloudAction::List),
        _ if rest.contains([':', '/', '\\']) => Err(EngineError(
            "cloud: podaj samą nazwę chmury z rclone (np. cloud gdrive), bez : i /".into(),
        )),
        _ => Ok(CloudAction::Open(rest.trim_matches('"').to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bluepane_remote::testing::FakeRclone;
    use bluepane_remote::{RcloneOutput, RcloneRunner, RemoteError};
    use std::sync::Arc;

    fn session(with: &[(&str, &str)]) -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = Session::new(&left, &right);
        s.use_config_dir(&tmp.path().join("config"));
        let cloud = tmp.path().join("cloud");
        std::fs::create_dir_all(cloud.join("gdrive/Zdjęcia")).unwrap();
        std::fs::write(cloud.join("gdrive/zażółć.txt"), "treść").unwrap();
        std::fs::write(cloud.join("gdrive/Zdjęcia/a.jpg"), "jpg").unwrap();
        s.rclone = Arc::new(FakeRclone::new(&cloud, with));
        (tmp, s)
    }

    #[test]
    fn the_clouds_rclone_knows_are_listed() {
        let (_tmp, mut s) = session(&[("gdrive", "drive"), ("nas", "webdav")]);
        s.run_line("cloud").unwrap();
        assert!(
            s.status.contains("gdrive (drive)") && s.status.contains("nas (webdav)"),
            "{}",
            s.status
        );
        let (_tmp, mut none) = session(&[]);
        none.run_line("cloud").unwrap();
        assert!(none.status.contains("rclone config"), "{}", none.status);
    }

    #[test]
    fn a_cloud_opens_as_a_folder_without_asking_for_a_password() {
        let (_tmp, mut s) = session(&[("gdrive", "drive")]);
        s.run_line("cloud gdrive").unwrap();
        assert!(s.pending.is_none(), "no password question for a cloud");
        s.wait_idle();
        let panel = s.active_panel();
        assert!(panel.remote.is_some(), "{}", s.status);
        let names: Vec<&str> = panel.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"zażółć.txt") && names.contains(&"Zdjęcia"),
            "{names:?}"
        );
        let site = s.sites.get("gdrive (rclone)").unwrap();
        assert_eq!(
            (site.protocol, site.host.as_str(), site.port),
            (Protocol::Rclone, "gdrive", 0)
        );
        assert!(site.problem().is_none());
        // The same cloud again reuses the site.
        s.run_line("disconnect").unwrap();
        s.run_line("cloud gdrive").unwrap();
        assert_eq!(s.sites.sites.len(), 1);
    }

    #[test]
    fn the_path_bar_opens_a_cloud_in_a_folder() {
        let (_tmp, mut s) = session(&[("gdrive", "drive")]);
        s.run_line("go rclone://gdrive/Zdjęcia").unwrap();
        s.wait_idle();
        s.wait_idle();
        let panel = s.active_panel();
        assert!(panel.remote.is_some(), "{}", s.status);
        assert!(
            panel.entries.iter().any(|e| e.name == "a.jpg"),
            "{:?}",
            panel.entries.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_unknown_cloud_and_a_missing_rclone_are_reported_without_saving_anything() {
        let (_tmp, mut s) = session(&[("gdrive", "drive")]);
        let err = s.run_line("cloud inna").unwrap_err();
        assert!(
            err.0.contains("nie zna chmury") && err.0.contains("gdrive"),
            "{err}"
        );
        assert!(s.sites.sites.is_empty());
        assert!(s.run_line("cloud a:b").is_err());

        #[derive(Debug)]
        struct Missing;
        impl RcloneRunner for Missing {
            fn run(
                &self,
                _: &[String],
                _: &dyn Fn() -> bool,
            ) -> Result<Option<RcloneOutput>, RemoteError> {
                Err(RemoteError::Connect(bluepane_remote::NOT_INSTALLED.into()))
            }
        }
        s.rclone = Arc::new(Missing);
        let err = s.run_line("cloud").unwrap_err();
        assert!(err.0.contains("zainstaluj"), "{err}");
        assert!(s.run_line("cloud gdrive").is_err());
        assert!(s.sites.sites.is_empty());
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_cloud("").unwrap(), CloudAction::List);
        assert_eq!(parse_cloud("LIST").unwrap(), CloudAction::List);
        assert_eq!(
            parse_cloud(" moja chmura ").unwrap(),
            CloudAction::Open("moja chmura".into())
        );
        assert!(parse_cloud("gdrive:/x").is_err());
    }
}
