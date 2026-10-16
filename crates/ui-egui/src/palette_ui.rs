//! The command palette (F1): a search box over all commands. The engine ranks and runs; this
//! file draws the rows and turns the keys into `palette ...` command lines.

use egui::{Align2, RichText};

use crate::BluepaneUi;

/// Rows shown at once; the list scrolls to the highlighted one.
const VISIBLE_ROWS: f32 = 12.0;

impl BluepaneUi {
    pub(crate) fn palette_dialog(&mut self, ctx: &egui::Context) {
        let Some(palette) = self.session.palette.clone() else {
            return;
        };
        let entries = self.session.palette_entries();
        let mut query = palette.query.clone();
        let mut lines: Vec<String> = Vec::new();
        let (down, up, enter, escape) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let muted = self.theme.muted;
        egui::Window::new("Paleta komend")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(Align2::CENTER_TOP, [0.0, 56.0])
            .fixed_size([620.0, 0.0])
            .show(ctx, |ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text(
                            "Wpisz komendę lub jej opis…  (↑↓ wybór, Enter uruchamia, Esc zamyka)",
                        )
                        .desired_width(f32::INFINITY),
                );
                // Modal in spirit: the cursor stays in the field.
                field.request_focus();
                ui.add_space(4.0);
                if entries.is_empty() {
                    ui.label(RichText::new("Nie ma takiej komendy").color(muted));
                }
                egui::ScrollArea::vertical()
                    .max_height(VISIBLE_ROWS * 24.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (index, entry) in entries.iter().enumerate() {
                            let selected = index == palette.selected;
                            let text =
                                format!("{}  {}", entry.help.usage, short(entry.help.description));
                            let mut label = RichText::new(text);
                            if entry.recent {
                                label = label.strong();
                            }
                            let response = ui.selectable_label(selected, label);
                            if selected && (down || up) {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                lines.push(format!(
                                    "palette move {}",
                                    index as i64 - palette.selected as i64
                                ));
                                lines.push("palette run".into());
                            }
                        }
                    });
            });
        if query != palette.query {
            lines.insert(0, format!("palette query {query}"));
        }
        if down {
            lines.push("palette move 1".into());
        }
        if up {
            lines.push("palette move -1".into());
        }
        if enter {
            lines.push("palette run".into());
        }
        if escape {
            lines.push("palette close".into());
        }
        for line in lines {
            let _ = self.session.run_line(&line);
        }
    }
}

/// The description up to the first `;`, so one row stays on one line.
fn short(description: &str) -> &str {
    description.split(';').next().unwrap_or(description)
}
