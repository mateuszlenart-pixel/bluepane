//! The site manager (Ctrl+S): saved SFTP sites on the left, the selected one's settings on
//! the right. It only sends `siteset`, `sitedel` and `connect` commands.

use bluepane_engine::{Command, Protocol};
use egui::{Color32, RichText};

use crate::BluepaneUi;

/// What the user is editing; sent to the engine only by "Zapisz" and "Połącz".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SiteForm {
    /// The saved name this form started from (empty for a new site).
    original: String,
    protocol: Protocol,
    name: String,
    host: String,
    port: String,
    user: String,
    key: String,
    start: String,
    /// FTP / FTPS only: active instead of passive data connections.
    active: bool,
    /// 0 = ask every time, 1 = store here, 2 = Bitwarden.
    /// 0 = ask every time, 1 = the sites file, 2 = the system password store, 3 = Bitwarden.
    secret_kind: u8,
    /// The password typed now (empty = leave what is saved as it is), or for kind 1 the saved text.
    secret: String,
    /// The Bitwarden item name.
    bw_item: String,
}

impl SiteForm {
    fn blank() -> SiteForm {
        SiteForm {
            port: "22".into(),
            ..SiteForm::default()
        }
    }

    fn from_site(site: &bluepane_engine::Site) -> SiteForm {
        use bluepane_engine::SecretStore;
        let (secret_kind, secret, bw_item) = match &site.secret {
            SecretStore::Ask => (0, String::new(), String::new()),
            SecretStore::Local(secret) => (1, secret.clone(), String::new()),
            SecretStore::Keyring => (2, String::new(), String::new()),
            SecretStore::Bitwarden(item) => (3, String::new(), item.clone()),
        };
        SiteForm {
            original: site.name.clone(),
            protocol: site.protocol,
            name: site.name.clone(),
            host: site.host.clone(),
            port: site.port.to_string(),
            user: site.user.clone(),
            key: site
                .key
                .as_ref()
                .map(|k| k.display().to_string())
                .unwrap_or_default(),
            start: site.start_dir.clone(),
            active: site.active,
            secret_kind,
            secret,
            bw_item,
        }
    }

    fn commands(&self) -> Vec<Command> {
        let set = |field: &str, value: &str| {
            Command::SiteSet(
                self.name.trim().to_string(),
                field.into(),
                value.to_string(),
            )
        };
        let secret = match self.secret_kind {
            1 => format!("local:{}", self.secret),
            2 => "keyring".to_string(),
            3 => format!("bitwarden:{}", self.bitwarden_item()),
            _ => "ask".to_string(),
        };
        let mut commands = vec![
            // The protocol first: changing it moves a default port, which is set again below.
            set("protocol", self.protocol.word()),
            set("host", &self.host),
            set("port", self.port.trim()),
            set("user", &self.user),
            set(
                "key",
                if self.protocol == Protocol::Sftp {
                    &self.key
                } else {
                    ""
                },
            ),
            set("start", &self.start),
            set(
                "mode",
                if self.protocol != Protocol::Sftp && self.active {
                    "active"
                } else {
                    "passive"
                },
            ),
            set("secret", &secret),
        ];
        // A password typed for the system store or Bitwarden is saved there (in the
        // background); the field stays empty afterwards.
        if matches!(self.secret_kind, 2 | 3) && !self.secret.is_empty() {
            commands.push(Command::SitePassword(
                self.name.trim().to_string(),
                self.secret.clone(),
            ));
        }
        commands
    }

    /// The Bitwarden item: what was typed, or `bluepane: <site name>`.
    fn bitwarden_item(&self) -> String {
        let typed = self.bw_item.trim();
        if typed.is_empty() {
            format!("bluepane: {}", self.name.trim())
        } else {
            typed.to_string()
        }
    }
}

impl BluepaneUi {
    pub(crate) fn sites_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            if key == "Escape" {
                self.dispatch("siteclose");
            }
        }
    }

    pub(crate) fn sites_ui(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = self
            .session
            .sites
            .sites
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let mut form = self.site_form.take().unwrap_or_else(SiteForm::blank);
        let mut commands: Vec<Command> = Vec::new();
        let mut load: Option<SiteForm> = None;
        let mut error: Option<String> = None;

        ui.label(RichText::new("Serwery: SFTP, FTP, FTPS").strong());
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.label("Zapisane połączenia:");
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for name in &names {
                        if ui
                            .selectable_label(form.original == *name, name)
                            .clicked()
                        {
                            if let Some(site) = self.session.sites.get(name) {
                                load = Some(SiteForm::from_site(site));
                            }
                        }
                    }
                });
                if ui.button("Nowa witryna").clicked() {
                    load = Some(SiteForm::blank());
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                egui::Grid::new("site_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    let field = |ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str| {
                        ui.label(label);
                        ui.add(
                            egui::TextEdit::singleline(text)
                                .hint_text(hint)
                                .desired_width(320.0),
                        );
                        ui.end_row();
                    };
                    field(ui, "Nazwa:", &mut form.name, "np. Proxmox");
                    ui.label("Protokół:");
                    let previous = form.protocol;
                    egui::ComboBox::from_id_salt("site_protocol")
                        .selected_text(form.protocol.label())
                        .show_ui(ui, |ui| {
                            for protocol in [
                                Protocol::Sftp,
                                Protocol::FtpsExplicit,
                                Protocol::FtpsImplicit,
                                Protocol::Ftp,
                                Protocol::Rclone,
                                Protocol::Smb,
                            ] {
                                ui.selectable_value(&mut form.protocol, protocol, protocol.label());
                            }
                        });
                    if form.protocol != previous && form.port.trim() == previous.default_port().to_string() {
                        form.port = form.protocol.default_port().to_string();
                    }
                    ui.end_row();
                    if form.protocol == Protocol::Rclone {
                        field(ui, "Chmura (nazwa z rclone):", &mut form.host, "np. gdrive; lista: rclone listremotes");
                    } else {
                        field(ui, "Serwer:", &mut form.host, "adres lub nazwa, np. 192.168.1.10");
                        field(ui, "Port:", &mut form.port, "22");
                        if form.protocol == Protocol::Smb {
                            field(ui, "Użytkownik:", &mut form.user, "puste = gość");
                        } else {
                            field(ui, "Użytkownik:", &mut form.user, "np. root");
                        }
                    }
                    if form.protocol == Protocol::Sftp {
                        field(ui, "Plik klucza:", &mut form.key, "puste = logowanie hasłem");
                    }
                    field(ui, "Katalog startowy:", &mut form.start, "puste = katalog domowy");
                });
                ui.add_space(6.0);
                if form.protocol == Protocol::Rclone {
                    ui.label(
                        RichText::new(
                            "Chmura działa przez program rclone: loginy i klucze trzyma rclone (rclone config), bluepane nie pyta o hasło. Wymaga zainstalowanego rclone (rclone.org).",
                        )
                        .weak(),
                    );
                }
                if form.protocol == Protocol::Smb {
                    ui.label(
                        RichText::new(
                            "Udziały SMB działają przez program smbclient (Linux: pakiet smbclient), bez montowania. Katalog główny serwera to lista udziałów; katalog startowy np. /dane/zdjęcia.",
                        )
                        .weak(),
                    );
                }
                if !matches!(form.protocol, Protocol::Sftp | Protocol::Rclone | Protocol::Smb) {
                    ui.checkbox(
                        &mut form.active,
                        "Tryb aktywny (serwer łączy się do Ciebie; domyślnie pasywny)",
                    )
                    .on_hover_text(
                        "Aktywny wymaga, żeby serwer mógł nawiązać połączenie z tym komputerem (bez NAT-a i zapory). Zwykle zostaw pasywny.",
                    );
                }
                if form.protocol == Protocol::Ftp {
                    ui.colored_label(
                        Color32::YELLOW,
                        "FTP bez szyfrowania: hasło i pliki idą otwartym tekstem. Używaj tylko w zaufanej sieci albo wybierz FTPS.",
                    );
                }
                if matches!(form.protocol, Protocol::FtpsExplicit | Protocol::FtpsImplicit) {
                    ui.label(
                        RichText::new("FTPS: certyfikat serwera jest przypinany (odcisk SHA-256 pokaże się przy pierwszym połączeniu).")
                            .weak(),
                    );
                }
                // A cloud keeps its login in rclone: no password here.
                if form.protocol == Protocol::Rclone {
                    form.secret_kind = 0;
                }
                let cloud = form.protocol == Protocol::Rclone;
                if !cloud {
                    ui.label(if form.protocol != Protocol::Sftp || form.key.trim().is_empty() {
                        "Hasło:"
                    } else {
                        "Hasło klucza (frazy):"
                    });
                }
                if !cloud {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut form.secret_kind, 0, "pytaj przy łączeniu");
                    ui.selectable_value(&mut form.secret_kind, 1, "w pliku witryn");
                    ui.selectable_value(&mut form.secret_kind, 2, "magazyn haseł systemu");
                    ui.selectable_value(&mut form.secret_kind, 3, "Bitwarden");
                });
                match form.secret_kind {
                    1 => {
                        ui.add(
                            egui::TextEdit::singleline(&mut form.secret)
                                .password(true)
                                .desired_width(320.0),
                        );
                        ui.colored_label(
                            Color32::YELLOW,
                            "Zapisane jawnym tekstem w pliku sites.conf (tylko dla Ciebie). Bezpieczniej: magazyn systemu, Bitwarden albo „pytaj”.",
                        );
                    }
                    2 => {
                        ui.add(
                            egui::TextEdit::singleline(&mut form.secret)
                                .password(true)
                                .hint_text("nowe hasło (puste = bez zmiany)")
                                .desired_width(320.0),
                        );
                        ui.label(
                            RichText::new(
                                "Hasło trafia do magazynu haseł systemu (Gnome Keyring / KWallet / Menedżer poświadczeń Windows), nie do pliku witryn.",
                            )
                            .weak(),
                        );
                    }
                    3 => {
                        ui.add(
                            egui::TextEdit::singleline(&mut form.bw_item)
                                .hint_text("nazwa elementu w Bitwarden (puste = bluepane: <nazwa>)")
                                .desired_width(320.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut form.secret)
                                .password(true)
                                .hint_text("nowe hasło do zapisania w Bitwarden (puste = element już istnieje)")
                                .desired_width(320.0),
                        );
                        ui.label(
                            RichText::new(if bluepane_engine::bitwarden_available() {
                                "Bitwarden CLI znaleziony. Odblokuj sejf (`bw unlock`) i ustaw BW_SESSION przed uruchomieniem bluepane. Nowy element powstaje tylko, jeśli o tej nazwie nie ma jeszcze żadnego."
                            } else {
                                "Nie znaleziono Bitwarden CLI (`bw`): zainstaluj go, żeby korzystać z tej opcji."
                            })
                            .weak(),
                        );
                    }
                    _ => {
                        ui.label(RichText::new("Hasło nie jest nigdzie zapisywane.").weak());
                    }
                }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = !form.name.trim().is_empty() && !form.host.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Zapisz"))
                        .clicked()
                    {
                        commands.extend(form.commands());
                        form.original = form.name.trim().to_string();
                        if matches!(form.secret_kind, 2 | 3) {
                            form.secret.clear();
                        }
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Zapisz i połącz"))
                        .clicked()
                    {
                        commands.extend(form.commands());
                        form.original = form.name.trim().to_string();
                        if matches!(form.secret_kind, 2 | 3) {
                            form.secret.clear();
                        }
                        commands.push(Command::SiteClose);
                        commands.push(Command::Connect(form.name.trim().to_string(), None));
                    }
                    if ui
                        .add_enabled(!form.original.is_empty(), egui::Button::new("Usuń"))
                        .clicked()
                    {
                        commands.push(Command::SiteDelete(form.original.clone()));
                        load = Some(SiteForm::blank());
                    }
                    if ui.button("Zamknij (Esc)").clicked() {
                        commands.push(Command::SiteClose);
                    }
                });
                if form.original.is_empty() {
                    ui.label(RichText::new("Nowa witryna: wpisz dane i kliknij „Zapisz”.").weak());
                }
                if !form.port.trim().is_empty() && form.port.trim().parse::<u16>().is_err() {
                    error = Some("Port to liczba od 1 do 65535".into());
                }
            });
        });
        if let Some(text) = error {
            ui.colored_label(ui.visuals().error_fg_color, text);
        }
        self.site_form = Some(load.unwrap_or(form));
        for command in commands {
            let _ = self.session.execute(command);
        }
    }
}
