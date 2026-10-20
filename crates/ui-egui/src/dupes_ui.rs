//! The duplicate finder: groups of identical files with a checkbox per copy. The engine finds,
//! marks and trashes; this draws the list and sends `dupes ...` lines.

use bluepane_engine::human_size;
use egui::RichText;

use crate::BluepaneUi;

/// One line of the flattened list: a group header or one copy of it.
enum Row {
    Header(String),
    Copy(usize, usize, String, bool),
}

impl BluepaneUi {
    pub(crate) fn dupes_keys(&mut self, pressed: &[String]) {
        if pressed.iter().any(|k| k == "Escape") {
            self.dispatch("dupes close");
        }
    }

    pub(crate) fn dupes_ui(&mut self, ui: &mut egui::Ui) {
        let Some(summary) = self.session.dup_summary() else {
            return;
        };
        let mut line: Option<String> = None;

        ui.label(RichText::new("Duplikaty").strong());
        ui.horizontal_wrapped(|ui| {
            for (text, command) in [
                ("Zostaw najnowszą", "dupes keep newest"),
                ("Zostaw najstarszą", "dupes keep oldest"),
                ("Zostaw najkrótszą ścieżkę", "dupes keep shortest"),
                ("Odznacz", "dupes clear"),
            ] {
                if ui.button(text).clicked() {
                    line = Some(command.into());
                }
            }
            let trash = ui.add_enabled(summary.marked > 0, egui::Button::new("Do kosza"));
            if trash.clicked() {
                line = Some("dupes trash".into());
            }
            if summary.running && ui.button("Przerwij").clicked() {
                line = Some("dupes stop".into());
            }
            if ui.button("Zamknij (Esc)").clicked() {
                line = Some("dupes close".into());
            }
        });
        let status = if summary.running && summary.listing {
            format!("Przeglądam katalogi: {} plików…", summary.progress)
        } else if summary.running {
            format!(
                "Liczę skróty, zostało {} plików. Grup: {}, do odzyskania {}",
                summary.progress,
                summary.groups,
                human_size(summary.wasted)
            )
        } else {
            format!(
                "Grup: {}, do odzyskania {}. Zaznaczono {} ({})",
                summary.groups,
                human_size(summary.wasted),
                summary.marked,
                human_size(summary.marked_bytes)
            )
        };
        ui.label(RichText::new(status).color(self.theme.muted));

        let mut rows = Vec::new();
        if let Some(found) = self.session.duplicates.as_ref() {
            for (g, group) in self.session.dup_groups().iter().enumerate() {
                rows.push(Row::Header(format!(
                    "Grupa {}: {} × {}",
                    g + 1,
                    group.files.len(),
                    human_size(group.size)
                )));
                for (f, file) in group.files.iter().enumerate() {
                    rows.push(Row::Copy(
                        g + 1,
                        f + 1,
                        file.path.display().to_string(),
                        found.is_marked(&file.path),
                    ));
                }
            }
        }
        if rows.is_empty() && !summary.running {
            ui.label("Nie znaleziono duplikatów.");
        }
        let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            height,
            rows.len(),
            |ui, range| {
                for row in &rows[range] {
                    match row {
                        Row::Header(text) => {
                            ui.label(RichText::new(text).strong());
                        }
                        Row::Copy(g, f, path, marked) => {
                            let mut checked = *marked;
                            if ui.checkbox(&mut checked, path).changed() {
                                line = Some(format!("dupes mark {g} {f}"));
                            }
                        }
                    }
                }
            },
        );
        if summary.running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(line) = line {
            self.dispatch(&line);
        }
    }
}
