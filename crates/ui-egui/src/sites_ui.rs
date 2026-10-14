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
    /// 0 = ask every time, 1 = store here, 2 = Bitwarden.
    secret_kind: u8,
    secret: String,
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
        let (secret_kind, secret) = match &site.secret {
            SecretStore::Ask => (0, String::new()),
            SecretStore::Local(secret) => (1, secret.clone()),
            SecretStore::Bitwarden(item) => (2, item.clone()),
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
            secret_kind,
            secret,
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
            2 => format!("bitwarden:{}", self.secret.trim()),
            _ => "ask".to_string(),
        };
        vec![
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
            set("secret", &secret),
        ]
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
                            ] {
                                ui.selectable_value(&mut form.protocol, protocol, protocol.label());
                            }
                        });
                    if form.protocol != previous && form.port.trim() == previous.default_port().to_string() {
                        form.port = form.protocol.default_port().to_string();
                    }
                    ui.end_row();
                    field(ui, "Serwer:", &mut form.host, "adres lub nazwa, np. 192.168.1.10");
                    field(ui, "Port:", &mut form.port, "22");
                    field(ui, "Użytkownik:", &mut form.user, "np. root");
                    if form.protocol == Protocol::Sftp {
                        field(ui, "Plik klucza:", &mut form.key, "puste = logowanie hasłem");
                    }
                    field(ui, "Katalog startowy:", &mut form.start, "puste = katalog domowy");
                });
                ui.add_space(6.0);
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
                ui.label(if form.protocol != Protocol::Sftp || form.key.trim().is_empty() {
                    "Hasło:"
                } else {
                    "Hasło klucza (frazy):"
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut form.secret_kind, 0, "pytaj przy łączeniu");
                    ui.selectable_value(&mut form.secret_kind, 1, "zapisz lokalnie");
                    ui.selectable_value(&mut form.secret_kind, 2, "Bitwarden");
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
                            "Zapisane jawnym tekstem w pliku sites.conf (tylko dla Ciebie). Bezpieczniej: „pytaj” albo Bitwarden.",
                        );
                    }
                    2 => {
                        ui.add(
                            egui::TextEdit::singleline(&mut form.secret)
                                .hint_text("nazwa lub identyfikator elementu w Bitwarden")
                                .desired_width(320.0),
                        );
                        ui.label(
                            RichText::new(if bluepane_engine::bitwarden_available() {
                                "Bitwarden CLI znaleziony. Odblokuj sejf (`bw unlock`) i ustaw BW_SESSION przed uruchomieniem bluepane."
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
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = !form.name.trim().is_empty() && !form.host.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Zapisz"))
                        .clicked()
                    {
                        commands.extend(form.commands());
                        form.original = form.name.trim().to_string();
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Zapisz i połącz"))
                        .clicked()
                    {
                        commands.extend(form.commands());
                        form.original = form.name.trim().to_string();
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
            ui.colored_label(Color32::LIGHT_RED, text);
        }
        self.site_form = Some(load.unwrap_or(form));
        for command in commands {
            let _ = self.session.execute(command);
        }
    }
}
