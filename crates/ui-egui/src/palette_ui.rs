//! The command palette (F1) and "jump to" (Ctrl+P): a search box over a list. The engine ranks
//! and runs; this file draws the rows and turns the keys into `palette ...` / `jump ...`
//! command lines.

use egui::{Align2, RichText};

use crate::BluepaneUi;

/// Rows shown at once; the list scrolls to the highlighted one.
const VISIBLE_ROWS: f32 = 12.0;

/// One row of a pick list.
struct PickRow {
    text: String,
    /// What kind of thing it is (shown muted in front), if anything.
    tag: Option<&'static str>,
    strong: bool,
}

impl BluepaneUi {
    pub(crate) fn palette_dialog(&mut self, ctx: &egui::Context) {
        let Some(palette) = self.session.palette.clone() else {
            return;
        };
        let rows: Vec<PickRow> = self
            .session
            .palette_entries()
            .iter()
            .map(|entry| PickRow {
                text: format!("{}  {}", entry.help.usage, short(entry.help.description)),
                tag: None,
                strong: entry.recent,
            })
            .collect();
        let hint = "Wpisz komendę lub jej opis…  (↑↓ wybór, Enter uruchamia, Esc zamyka)";
        let lines = self.pick_window(
            ctx,
            ("Paleta komend", "palette", hint, "Nie ma takiej komendy"),
            &palette.query,
            palette.selected,
            &rows,
        );
        for line in lines {
            let _ = self.session.run_line(&line);
        }
    }

    pub(crate) fn jump_dialog(&mut self, ctx: &egui::Context) {
        let Some(jump) = self.session.jump.clone() else {
            return;
        };
        let rows: Vec<PickRow> = self
            .session
            .jump_entries()
            .into_iter()
            .map(|entry| PickRow {
                text: entry.label,
                tag: Some(entry.kind),
                strong: false,
            })
            .collect();
        let hint =
            "Skocz do katalogu, zakładki albo serwera…  (↑↓ wybór, Enter przechodzi, Esc zamyka)";
        let lines = self.pick_window(
            ctx,
            ("Skocz do", "jump", hint, "Nie ma takiego miejsca"),
            &jump.query,
            jump.selected,
            &rows,
        );
        for line in lines {
            let _ = self.session.run_line(&line);
        }
    }

    /// Draws the search field and the rows; returns the command lines the input asks for.
    /// `names` is (window title, command, hint, text when nothing matches).
    fn pick_window(
        &self,
        ctx: &egui::Context,
        names: (&str, &str, &str, &str),
        query: &str,
        selected: usize,
        rows: &[PickRow],
    ) -> Vec<String> {
        let (title, command, hint, empty) = names;
        let mut text = query.to_string();
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
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(Align2::CENTER_TOP, [0.0, 56.0])
            .fixed_size([620.0, 0.0])
            .show(ctx, |ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY),
                );
                // Modal in spirit: the cursor stays in the field.
                field.request_focus();
                ui.add_space(4.0);
                if rows.is_empty() {
                    ui.label(RichText::new(empty).color(muted));
                }
                egui::ScrollArea::vertical()
                    .max_height(VISIBLE_ROWS * 24.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (index, row) in rows.iter().enumerate() {
                            let mut label = match row.tag {
                                Some(tag) => RichText::new(format!("{tag}  {}", row.text)),
                                None => RichText::new(row.text.clone()),
                            };
                            if row.strong {
                                label = label.strong();
                            }
                            let response = ui.selectable_label(index == selected, label);
                            if index == selected && (down || up) {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                lines.push(format!(
                                    "{command} move {}",
                                    index as i64 - selected as i64
                                ));
                                lines.push(format!("{command} run"));
                            }
                        }
                    });
            });
        if text != query {
            lines.insert(0, format!("{command} query {text}"));
        }
        if down {
            lines.push(format!("{command} move 1"));
        }
        if up {
            lines.push(format!("{command} move -1"));
        }
        if enter {
            lines.push(format!("{command} run"));
        }
        if escape {
            lines.push(format!("{command} close"));
        }
        lines
    }
}

/// The description up to the first `;`, so one row stays on one line.
fn short(description: &str) -> &str {
    description.split(';').next().unwrap_or(description)
}
