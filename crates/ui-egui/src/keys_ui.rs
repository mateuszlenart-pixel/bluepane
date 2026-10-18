//! The key cheat sheet: `keys` / `?` opens it with every key, holding Ctrl or Alt alone for a
//! moment shows the keys with that modifier until it is let go. The list comes from the
//! engine (the current key map with the user's own bindings).

use egui::{Align2, RichText};

use crate::BluepaneUi;

/// Seconds Ctrl or Alt must be held alone before its keys are shown.
const HOLD: f64 = 0.7;

impl BluepaneUi {
    pub(crate) fn key_sheet_ui(&mut self, ctx: &egui::Context) {
        // Ctrl or Alt held on its own (no other key, no text field) shows its keys.
        let (now, modifiers, keys_down, typing) =
            ctx.input(|i| (i.time, i.modifiers, i.keys_down.len(), false));
        let typing = typing || ctx.egui_wants_keyboard_input();
        let held = match (modifiers.ctrl, modifiers.alt, modifiers.shift) {
            (true, false, false) => Some("Ctrl+"),
            (false, true, false) => Some("Alt+"),
            _ => None,
        }
        .filter(|_| keys_down == 0 && !typing);
        self.modifier_hold = match (held, self.modifier_hold) {
            (Some(prefix), Some((was, since))) if prefix == was => Some((was, since)),
            (Some(prefix), _) => Some((prefix, now)),
            (None, _) => None,
        };
        let held_long = self
            .modifier_hold
            .filter(|(_, since)| now - since >= HOLD)
            .map(|(prefix, _)| prefix);
        if self.modifier_hold.is_some() && held_long.is_none() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        let (prefix, from_command) = match (&self.session.key_sheet_open, held_long) {
            (Some(prefix), _) => (prefix.clone(), true),
            (None, Some(prefix)) => (prefix.to_string(), false),
            (None, None) => return,
        };
        let lines = self.session.key_sheet(&prefix);
        let title = match prefix.as_str() {
            "" => "Klawisze".to_string(),
            other => format!("Klawisze z {}", other.trim_end_matches('+')),
        };
        let (accent, muted) = (self.theme.accent, self.theme.muted);
        let mut close = false;
        egui::Window::new(&title)
            .id(egui::Id::new("key-sheet"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(480.0)
                    .show(ui, |ui| {
                        let mut group = "";
                        egui::Grid::new("key-sheet-grid")
                            .num_columns(2)
                            .spacing([18.0, 3.0])
                            .show(ui, |ui| {
                                for line in &lines {
                                    if line.group != group {
                                        group = line.group;
                                        ui.label(
                                            RichText::new(group).color(muted).small().strong(),
                                        );
                                        ui.end_row();
                                    }
                                    ui.label(RichText::new(&line.key).monospace().color(accent));
                                    ui.label(&line.description);
                                    ui.end_row();
                                }
                            });
                        if lines.is_empty() {
                            ui.label(RichText::new("Brak takich klawiszy").color(muted));
                        }
                    });
                ui.separator();
                let hint = if from_command {
                    "Esc albo ? zamyka; klawisze zmienisz komendą bind"
                } else {
                    "Puść klawisz, żeby zamknąć"
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(hint).color(muted).small());
                    if from_command && ui.button("Zamknij").clicked() {
                        close = true;
                    }
                });
            });
        if from_command && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if close {
            self.session.key_sheet_open = None;
        }
    }
}
