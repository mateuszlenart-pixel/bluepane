//! The Places bar (Ctrl+B): drives, favorites, servers and recent folders as a list on the
//! left. Every row is a command line the engine runs (`volume 2`, `cd ...`, `connect ...`).

use bluepane_engine::human_size;
use egui::RichText;

use crate::BluepaneUi;

impl BluepaneUi {
    pub(crate) fn places_bar(&mut self, ui: &mut egui::Ui) {
        let sections = self.session.places();
        let (muted, accent, warn, error) = (
            self.theme.muted,
            self.theme.accent,
            self.theme.warn,
            self.theme.error,
        );
        let mut line: Option<String> = None;
        egui::Panel::left("places")
            .resizable(true)
            .default_size(210.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(4.0);
                    for section in &sections {
                        ui.label(RichText::new(section.title).color(muted).small().strong());
                        for place in &section.places {
                            ui.horizontal(|ui| {
                                let text = RichText::new(&place.label);
                                let text = if place.current {
                                    text.color(accent).strong()
                                } else {
                                    text
                                };
                                if ui.selectable_label(place.current, text).clicked() {
                                    line = Some(place.line.clone());
                                }
                                if let Some(remove) = &place.remove {
                                    let close =
                                        ui.small_button("✕").on_hover_text("Usuń z ulubionych");
                                    if close.clicked() {
                                        line = Some(remove.clone());
                                    }
                                }
                            });
                            if let Some(space) = place.space {
                                let used = space.used_fraction();
                                let fill = if used >= 0.95 {
                                    error
                                } else if used >= 0.85 {
                                    warn
                                } else {
                                    accent
                                };
                                ui.add(egui::ProgressBar::new(used).desired_height(6.0).fill(fill));
                                ui.label(
                                    RichText::new(format!(
                                        "{} wolne z {}",
                                        human_size(space.free),
                                        human_size(space.total)
                                    ))
                                    .color(muted)
                                    .small(),
                                );
                            }
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        if let Some(line) = line {
            self.dispatch(&line);
        }
    }
}
