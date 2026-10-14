//! The network scanner window (Ctrl+N), drawn in place of the two panels.
//!
//! It edits the engine's form with `scanset`, starts with `scanrun` and shows the hosts the
//! scanning thread has found so far (it redraws while the scan runs).

use bluepane_engine::{Command, ScanHost};
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
            ui.colored_label(Color32::LIGHT_RED, error);
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
        ui.separator();
        self.scan_rows(ui, &hosts);

        if run && !running {
            commands.push(Command::ScanRun);
        }
        for command in commands {
            let _ = self.session.execute(command);
        }
    }

    fn scan_rows(&self, ui: &mut egui::Ui, hosts: &[ScanHost]) {
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
                        egui::Sense::hover(),
                    );
                    if response.hovered() {
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
                    let cells: [(f32, f32, String, Color32); 6] = [
                        (
                            0.02,
                            0.23,
                            format!("● {}", host.name.clone().unwrap_or_default()),
                            normal,
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
}
