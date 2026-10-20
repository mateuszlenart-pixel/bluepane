//! The network scanner window (Ctrl+N), drawn in place of the two panels.
//!
//! It edits the engine's form with `scanset`, starts with `scanrun` and shows the hosts the
//! scanning thread has found so far (it redraws while the scan runs).

use std::net::Ipv4Addr;

use bluepane_engine::{format_modified, Command, ScanHost, ShareKind, ShareList};
use egui::{Align2, Color32, FontId, RichText};

use crate::BluepaneUi;

const ROW_HEIGHT: f32 = 22.0;

impl BluepaneUi {
    pub(crate) fn scan_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            if key == "Escape" {
                let running = self.session.scan.as_ref().is_some_and(|v| v.is_running());
                self.dispatch(if running { "scanstop" } else { "scanclose" });
            }
        }
    }

    pub(crate) fn scan_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.session.scan.as_ref() else {
            return;
        };
        let form = view.form.clone();
        let running = view.is_running();
        let progress = view.progress();
        let hosts = view.visible_hosts();
        let selected = view.selected;
        let show_favorites = view.show_favorites;
        let show_history = view.show_history;
        let favorites = view.favorites.clone();
        let (total, summary, error) = {
            let data = view.data();
            (data.hosts.len(), data.summary.clone(), data.error.clone())
        };
        let mut commands: Vec<Command> = Vec::new();
        let mut run = false;
        let set = |field: &str, value: String| Command::ScanSet(field.into(), value);

        ui.label(RichText::new("Skaner sieci").strong());
        ui.add_space(2.0);
        egui::Grid::new("scan_form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Zakres adresów IP:");
            ui.horizontal(|ui| {
                let mut text = form.range.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text("np. 192.168.0.1-254, 10.0.0.0/24, 192.168.0.90-110")
                        .desired_width(380.0),
                );
                if response.changed() {
                    commands.push(set("range", text));
                }
                run |= response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui
                    .button("Moja sieć")
                    .on_hover_text("zakres sieci tego komputera (/24)")
                    .clicked()
                {
                    if let Some(range) = bluepane_engine::local_range() {
                        commands.push(set("range", range));
                    }
                }
            });
            ui.end_row();

            ui.label("Porty:");
            ui.horizontal(|ui| {
                let mut text = form.ports.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("auto albo np. 22,80,8000-8010")
                            .desired_width(240.0),
                    )
                    .changed()
                {
                    commands.push(set("ports", text));
                }
                let mut names = form.names;
                if ui
                    .checkbox(&mut names, "nazwy (DNS)")
                    .on_hover_text("odpytaj DNS o nazwę każdego urządzenia (może chwilę trwać)")
                    .changed()
                {
                    commands.push(set("names", if names { "on" } else { "off" }.into()));
                }
                let mut arp = form.arp;
                if ui
                    .checkbox(&mut arp, "tablica ARP")
                    .on_hover_text(
                        "urządzenia z zablokowanymi portami też odpowiadają na ARP w sieci lokalnej",
                    )
                    .changed()
                {
                    commands.push(set("arp", if arp { "on" } else { "off" }.into()));
                }
            });
            ui.end_row();

            ui.label("Filtr wyników:");
            let mut filter = form.filter.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("nazwa, adres, producent, usługa…")
                        .desired_width(380.0),
                )
                .changed()
            {
                commands.push(set("filter", filter));
            }
            ui.end_row();
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("Skanuj"))
                .clicked()
            {
                run = true;
            }
            if ui
                .add_enabled(running, egui::Button::new("Zatrzymaj (Esc)"))
                .clicked()
            {
                commands.push(Command::ScanStop);
            }
            if ui.button("Zamknij").clicked() {
                commands.push(Command::ScanClose);
            }
            if running {
                if let Some((done, all)) = progress {
                    if all > 0 {
                        ui.add(
                            egui::ProgressBar::new(done as f32 / all as f32)
                                .desired_width(180.0)
                                .show_percentage(),
                        );
                    }
                }
                ui.spinner();
            }
        });
        if let Some(error) = error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        } else if running {
            ui.label(format!("Skanowanie… znaleziono urządzeń: {total}"));
        } else if let Some(summary) = summary {
            let mut text = format!(
                "Przeskanowano {} adresów, znaleziono urządzeń: {}",
                summary.addresses, summary.hosts
            );
            if summary.cancelled {
                text = format!("Zatrzymano. {text}");
            }
            ui.label(RichText::new(text).strong());
        }
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!show_favorites && !show_history, "Wyniki")
                .clicked()
            {
                commands.push(Command::ScanFavorites(false));
                commands.push(Command::ScanHistory(false));
            }
            if ui
                .selectable_label(show_favorites, format!("Ulubione ({})", favorites.len()))
                .clicked()
            {
                commands.push(Command::ScanFavorites(true));
            }
            if ui.selectable_label(show_history, "Historia").clicked() {
                commands.push(Command::ScanHistory(true));
            }
        });
        ui.separator();
        if let Some(ip) = selected {
            self.scan_details(ui, ip, &mut commands);
            ui.separator();
        }
        if show_history {
            self.scan_history(ui, selected, &mut commands);
        } else if show_favorites {
            self.scan_favorites(ui, &favorites, selected, &mut commands);
        } else {
            self.scan_rows(ui, &hosts, selected, &mut commands);
        }

        if run && !running {
            commands.push(Command::ScanRun);
        }
        for command in commands {
            let _ = self.session.execute(command);
        }
    }

    fn scan_rows(
        &self,
        ui: &mut egui::Ui,
        hosts: &[ScanHost],
        selected: Option<Ipv4Addr>,
        commands: &mut Vec<Command>,
    ) {
        let font = FontId::monospace(13.0);
        // Header.
        let (header, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), ROW_HEIGHT),
            egui::Sense::hover(),
        );
        let columns = [
            ("Nazwa", 0.02),
            ("Adres IP", 0.24),
            ("Producent", 0.36),
            ("Adres MAC", 0.54),
            ("Usługi", 0.68),
            ("ms", 0.96),
        ];
        for (title, at) in columns {
            ui.painter().text(
                egui::pos2(header.left() + header.width() * at, header.center().y),
                Align2::LEFT_CENTER,
                title,
                font.clone(),
                ui.visuals().strong_text_color(),
            );
        }
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            ROW_HEIGHT,
            hosts.len(),
            |ui, range| {
                for host in &hosts[range] {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        egui::Sense::click(),
                    );
                    if response.clicked() {
                        commands.push(Command::ScanSelect(host.ip));
                    }
                    if selected == Some(host.ip) {
                        ui.painter()
                            .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            ui.visuals().widgets.hovered.weak_bg_fill,
                        );
                    }
                    let y = rect.center().y;
                    let at = |f: f32| rect.left() + rect.width() * f;
                    let normal = ui.visuals().text_color();
                    let weak = ui.visuals().weak_text_color();
                    let painter = ui.painter();
                    let services = host
                        .services
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let is_new = self.session.scan.as_ref().is_some_and(|v| v.is_new(host));
                    let cells: [(f32, f32, String, Color32); 6] = [
                        (
                            0.02,
                            0.23,
                            format!(
                                "{} {}",
                                if is_new { "NOWE ●" } else { "●" },
                                host.name.clone().unwrap_or_default()
                            ),
                            if is_new { self.theme.accent } else { normal },
                        ),
                        (0.24, 0.35, host.ip.to_string(), normal),
                        (0.36, 0.53, host.vendor.clone().unwrap_or_default(), normal),
                        (0.54, 0.67, host.mac.clone().unwrap_or_default(), weak),
                        (0.68, 0.95, services, normal),
                        (
                            0.96,
                            1.0,
                            host.latency
                                .map(|d| d.as_millis().to_string())
                                .unwrap_or_default(),
                            weak,
                        ),
                    ];
                    for (from, to, text, color) in cells {
                        let clip = egui::Rect::from_min_max(
                            egui::pos2(at(from), rect.top()),
                            egui::pos2(at(to), rect.bottom()),
                        );
                        painter.with_clip_rect(clip).text(
                            egui::pos2(clip.left(), y),
                            Align2::LEFT_CENTER,
                            text,
                            font.clone(),
                            color,
                        );
                    }
                }
            },
        );
    }

    /// What can be done with the selected host: its services, shares, address, favorite.
    fn scan_details(&mut self, ui: &mut egui::Ui, ip: Ipv4Addr, commands: &mut Vec<Command>) {
        let Some(view) = self.session.scan.as_ref() else {
            return;
        };
        let host = view.host(ip);
        let is_favorite = view.is_favorite(ip);
        let shares = view.shares_of(ip);
        ui.horizontal_wrapped(|ui| {
            let title = match host.as_ref().and_then(|h| h.name.clone()) {
                Some(name) => format!("{ip}  {name}"),
                None => ip.to_string(),
            };
            ui.label(RichText::new(title).strong());
            if let Some(vendor) = host.as_ref().and_then(|h| h.vendor.clone()) {
                ui.label(RichText::new(vendor).weak());
            }
            if ui.button("Kopiuj adres").clicked() {
                commands.push(Command::ScanCopy(ip));
            }
            if ui
                .button(if is_favorite {
                    "★ Usuń z ulubionych"
                } else {
                    "☆ Dodaj do ulubionych"
                })
                .clicked()
            {
                commands.push(Command::ScanFavorite(ip));
            }
            if ui
                .button("Obudź (Wake-on-LAN)")
                .on_hover_text(
                    "wysyła pakiet budzący na adres MAC urządzenia (znany z skanu albo historii); komputer musi mieć włączone budzenie przez sieć",
                )
                .clicked()
            {
                commands.push(Command::ScanWake(ip.to_string()));
            }
            for (label, protocol) in [
                ("Witryna SFTP", bluepane_engine::Protocol::Sftp),
                ("Witryna FTP", bluepane_engine::Protocol::Ftp),
            ] {
                if ui
                    .button(label)
                    .on_hover_text(
                        "zapisuje urządzenie jako serwer w menedżerze serwerów (potem podasz użytkownika i hasło)",
                    )
                    .clicked()
                {
                    commands.push(Command::ScanSite(ip, protocol));
                }
            }
        });
        if let Some(host) = &host {
            ui.horizontal_wrapped(|ui| {
                if host.services.is_empty() {
                    ui.label(RichText::new("Brak otwartych portów z listy.").weak());
                }
                for service in &host.services {
                    let hint = match service.port {
                        80 | 3000 | 5000 | 8000 | 8080 | 8123 | 8888 | 9000 => {
                            "otwórz w przeglądarce"
                        }
                        443 | 5001 | 8006 | 8443 => "otwórz w przeglądarce (HTTPS)",
                        3389 => "uruchom pulpit zdalny",
                        22 => "zapisz jako serwer SFTP (potem podasz użytkownika i hasło)",
                        21 => "zapisz jako serwer FTP (potem podasz użytkownika i hasło)",
                        139 | 445 => "pokaż udziały sieciowe",
                        _ => "brak domyślnej akcji (skopiuj adres)",
                    };
                    if ui
                        .button(format!("{} :{}", service.name, service.port))
                        .on_hover_text(hint)
                        .clicked()
                    {
                        commands.push(Command::ScanOpen(ip, service.port));
                    }
                }
            });
        }
        match shares {
            None => {}
            Some(ShareList::Loading) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Szukanie udziałów…");
                });
            }
            Some(ShareList::Failed(why)) => {
                ui.colored_label(ui.visuals().error_fg_color, why);
            }
            Some(ShareList::Ready(list)) => {
                let visible: Vec<_> = list.iter().filter(|s| !s.is_hidden()).collect();
                ui.label(RichText::new(format!("Udziały: {}", visible.len())).strong());
                for share in visible {
                    ui.horizontal(|ui| {
                        let icon = if share.kind == ShareKind::Printer {
                            "🖨"
                        } else {
                            "📁"
                        };
                        ui.label(format!("{icon} {}", share.name));
                        if !share.comment.is_empty() {
                            ui.label(RichText::new(&share.comment).weak());
                        }
                        if share.kind != ShareKind::Printer {
                            if ui
                                .small_button("W panelu")
                                .on_hover_text("pokazuje udział w aktywnym panelu")
                                .clicked()
                            {
                                commands.push(Command::ScanPanel(ip, share.name.clone()));
                            }
                            if ui
                                .small_button("Otwórz")
                                .on_hover_text("otwiera udział w menedżerze plików systemu")
                                .clicked()
                            {
                                commands.push(Command::ScanShare(ip, share.name.clone()));
                            }
                        }
                    });
                }
            }
        }
    }

    /// The devices the scanner has seen (latest first) and the log of the last scans.
    fn scan_history(
        &self,
        ui: &mut egui::Ui,
        selected: Option<Ipv4Addr>,
        commands: &mut Vec<Command>,
    ) {
        let Some(view) = self.session.scan.as_ref() else {
            return;
        };
        let history = view.history();
        if history.devices.is_empty() {
            ui.label(
                RichText::new(
                    "Historia jest pusta: powstaje po pierwszym zakończonym skanie (zatrzymany skan nie jest zapamiętywany).",
                )
                .weak(),
            );
            return;
        }
        let mut devices = history.devices.clone();
        devices.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then(a.ip.cmp(&b.ip)));
        ui.label(RichText::new(format!("Znane urządzenia: {}", devices.len())).strong());
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for device in &devices {
                    let label = format!(
                        "{}  {}  {}  {}  ostatnio {}, pierwszy raz {}",
                        device.ip,
                        device.name,
                        device.mac.as_deref().unwrap_or("brak MAC"),
                        device.vendor,
                        format_modified(Some(device.last_seen_at())),
                        format_modified(Some(device.first_seen_at())),
                    );
                    if ui
                        .selectable_label(selected == Some(device.ip), label)
                        .clicked()
                    {
                        commands.push(Command::ScanSelect(device.ip));
                    }
                }
                ui.separator();
                ui.label(RichText::new("Ostatnie skany").strong());
                for entry in history.log.iter().rev().take(20) {
                    ui.label(format!(
                        "{}  {}  urządzeń {}, nowych {}",
                        format_modified(Some(entry.at())),
                        entry.range,
                        entry.hosts,
                        entry.new
                    ));
                }
            });
    }

    fn scan_favorites(
        &self,
        ui: &mut egui::Ui,
        favorites: &[bluepane_engine::Favorite],
        selected: Option<Ipv4Addr>,
        commands: &mut Vec<Command>,
    ) {
        if favorites.is_empty() {
            ui.label(
                RichText::new(
                    "Brak ulubionych: zaznacz urządzenie w wynikach i dodaj je gwiazdką.",
                )
                .weak(),
            );
        }
        let view = self.session.scan.as_ref();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for favorite in favorites {
                    let seen = view.is_some_and(|v| v.host(favorite.ip).is_some());
                    let label = format!(
                        "★ {}  {}   {}",
                        favorite.ip,
                        favorite.name,
                        if seen {
                            "(w ostatnim skanie)"
                        } else {
                            "(nie widać w skanie)"
                        }
                    );
                    if ui
                        .selectable_label(selected == Some(favorite.ip), label)
                        .clicked()
                    {
                        commands.push(Command::ScanSelect(favorite.ip));
                    }
                }
            });
    }
}
