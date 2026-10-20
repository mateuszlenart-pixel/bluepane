//! The comparison tab: two files side by side with the differences marked, the first
//! difference of two binary files, or the result of a checksum check. The engine compares and
//! hashes; this draws what it found.

use std::sync::Arc;

use bluepane_engine::{human_size, CompareOutcome, DiffRow, RowKind, SumStatus};
use egui::{Color32, RichText};

use crate::BluepaneUi;

fn tint(dark: bool, kind: RowKind) -> Option<Color32> {
    let (r, g, b) = match kind {
        RowKind::Same => return None,
        RowKind::Changed => (200, 160, 40),
        RowKind::OnlyLeft => (200, 70, 70),
        RowKind::OnlyRight => (60, 170, 90),
    };
    Some(Color32::from_rgba_unmultiplied(
        r,
        g,
        b,
        if dark { 70 } else { 90 },
    ))
}

fn cell(side: &Option<(usize, String)>) -> String {
    side.as_ref()
        .map_or_else(String::new, |(n, text)| format!("{n:>5}  {text}"))
}

fn hex_line(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x} ")).collect()
}

impl BluepaneUi {
    pub(crate) fn compare_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            match key.as_str() {
                "Escape" => self.dispatch("compare close"),
                "F8" => self.dispatch("compare next"),
                "F7" => self.dispatch("compare prev"),
                _ => {}
            }
        }
    }

    pub(crate) fn compare_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.session.compare.as_ref() else {
            return;
        };
        let (title, left, right, current) = (
            view.title.clone(),
            view.left.clone(),
            view.right.clone(),
            view.current,
        );
        let outcome = view.outcome();
        let mut line: Option<String> = None;

        ui.label(RichText::new(title).strong());
        ui.horizontal(|ui| {
            if matches!(outcome.as_deref(), Some(CompareOutcome::Text { .. })) {
                if ui.button("Poprzednia różnica (F7)").clicked() {
                    line = Some("compare prev".into());
                }
                if ui.button("Następna różnica (F8)").clicked() {
                    line = Some("compare next".into());
                }
            }
            if ui.button("Zamknij (Esc)").clicked() {
                line = Some("compare close".into());
            }
        });

        match outcome {
            None => {
                ui.label(RichText::new("Czytam pliki…").color(self.theme.muted));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
            Some(outcome) => self.compare_result(ui, &outcome, &left, &right, current),
        }
        if let Some(line) = line {
            self.dispatch(&line);
        }
    }

    fn compare_result(
        &mut self,
        ui: &mut egui::Ui,
        outcome: &Arc<CompareOutcome>,
        left: &str,
        right: &str,
        current: usize,
    ) {
        let muted = self.theme.muted;
        match &**outcome {
            CompareOutcome::Identical { size } => {
                ui.label(format!("Pliki są identyczne ({}).", human_size(*size)));
            }
            CompareOutcome::Failed(text) => {
                ui.label(RichText::new(text).color(self.theme.error));
            }
            CompareOutcome::Binary {
                left_size,
                right_size,
                first,
                left_bytes,
                right_bytes,
            } => {
                ui.label(format!(
                    "Pliki binarne: {} i {}.",
                    human_size(*left_size),
                    human_size(*right_size)
                ));
                match first {
                    Some(offset) => {
                        ui.label(format!(
                            "Pierwsza różnica na bajcie {offset} (0x{offset:x})."
                        ));
                        ui.monospace(format!("{left}\n  {}", hex_line(left_bytes)));
                        ui.monospace(format!("{right}\n  {}", hex_line(right_bytes)));
                    }
                    None => {
                        ui.label("Krótszy plik jest początkiem dłuższego.");
                    }
                }
            }
            CompareOutcome::Sums { file, lines } => {
                let bad = lines
                    .iter()
                    .filter(|l| !matches!(l.status, SumStatus::Ok | SumStatus::Created))
                    .count();
                ui.label(
                    RichText::new(format!(
                        "{file}: plików {}, do sprawdzenia {bad}",
                        lines.len()
                    ))
                    .color(muted),
                );
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for l in lines {
                            let (mark, color) = match l.status {
                                SumStatus::Ok => ("zgodny", self.theme.ok),
                                SumStatus::Created => ("zapisano", self.theme.ok),
                                SumStatus::Mismatch => ("ZMIENIONY", self.theme.error),
                                SumStatus::Missing => ("brak pliku", self.theme.error),
                                SumStatus::Unreadable => ("nie do odczytu", self.theme.error),
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(mark).color(color).strong());
                                ui.label(&l.name);
                            });
                        }
                    });
            }
            CompareOutcome::Text { rows, starts } => {
                ui.label(
                    RichText::new(format!(
                        "Różnice: {} (bieżąca {}). Żółte: zmieniona linia, czerwone: tylko po lewej, zielone: tylko po prawej.",
                        starts.len(),
                        current + 1
                    ))
                    .color(muted),
                );
                self.compare_rows(ui, rows, left, right);
            }
        }
    }

    fn compare_rows(&mut self, ui: &mut egui::Ui, rows: &[DiffRow], left: &str, right: &str) {
        let dark = ui.visuals().dark_mode;
        let height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
        let scroll = self
            .session
            .compare
            .as_mut()
            .and_then(bluepane_engine::CompareView::take_scroll);
        let mut area = egui::ScrollArea::both().auto_shrink(false);
        if let Some(row) = scroll {
            area = area.vertical_scroll_offset(row.saturating_sub(2) as f32 * height);
        }
        ui.columns(2, |cols| {
            cols[0].label(RichText::new(left).color(self.theme.muted));
            cols[1].label(RichText::new(right).color(self.theme.muted));
        });
        area.show_rows(ui, height, rows.len(), |ui, range| {
            for row in &rows[range] {
                ui.columns(2, |cols| {
                    for (col, side) in cols.iter_mut().zip([&row.left, &row.right]) {
                        let mut text = RichText::new(cell(side)).monospace();
                        if let Some(color) = tint(dark, row.kind) {
                            text = text.background_color(color);
                        }
                        col.add(egui::Label::new(text).truncate());
                    }
                });
            }
        });
    }
}
