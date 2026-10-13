//! egui front end.
//!
//! The UI only translates input into command lines and draws the engine's state.
//! It contains no file system logic.

use bluepane_engine::{format_modified, human_size, name_parts, Command, Panel, Session, SortKey};
use egui::{Align2, Color32, FontId, Rect, RichText};

const CMD_LINE_ID: &str = "bluepane-command-line";

/// F-key bar: (key, label, command line). Same bindings as the default key map.
const FKEYS: &[(&str, &str, &str)] = &[
    ("F5", "Kopiuj", "copy"),
    ("F6", "Przenieś", "move"),
    ("F7", "Nowy katalog", "mkdir "),
    ("F8", "Do kosza", "delete"),
    ("F9", "Odśwież", "refresh"),
];

pub struct BluepaneUi {
    session: Session,
    command_line: String,
    focus_command_line: bool,
    last_error: bool,
    /// Editable text of each panel's path bar (resynced from the panel while not focused).
    path_text: [String; 2],
}

impl BluepaneUi {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            command_line: String::new(),
            focus_command_line: false,
            last_error: false,
            path_text: [String::new(), String::new()],
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Draws one frame. Call this from `eframe::App::ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let cmd_id = egui::Id::new(CMD_LINE_ID);
        // While any text field (command line, path bar) has focus, keys belong to it.
        if !ctx.egui_wants_keyboard_input() {
            self.handle_keys(&ctx);
        }

        egui::Panel::bottom("bottom").show(ui, |ui| {
            self.bottom_bar(ui, cmd_id);
        });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            ui.columns(2, |columns| {
                for (index, ui) in columns.iter_mut().enumerate() {
                    self.panel_ui(ui, index);
                }
            });
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let (pressed, typed) = ctx.input(|i| {
            let mut pressed = Vec::new();
            let mut typed = String::new();
            // Alt/Ctrl shortcuts can also arrive as text ("Alt+2" -> "2"); the modifier state
            // of the frame may already be released, so remember it from the key events.
            let mut shortcut = i.modifiers.alt || i.modifiers.ctrl;
            for event in &i.events {
                match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        shortcut |= modifiers.alt || modifiers.ctrl;
                        pressed.push(key_name(*key, *modifiers));
                    }
                    egui::Event::Text(text) => {
                        typed.extend(
                            text.chars()
                                .filter(|c| !c.is_whitespace() && !c.is_control()),
                        );
                    }
                    _ => {}
                }
            }
            if shortcut {
                typed.clear();
            }
            (pressed, typed)
        });
        if self.session.pending.is_some() {
            // A question is open: only the answer keys count.
            for key in pressed {
                match key.as_str() {
                    "Enter" => self.dispatch("confirm"),
                    "Escape" => self.dispatch("cancel"),
                    _ => {}
                }
            }
            return;
        }
        for key in pressed {
            if let Some(line) = self.session.keymap.lookup(&key).map(str::to_owned) {
                self.dispatch(&line);
            }
        }
        if !typed.is_empty() {
            let filter = format!("{}{typed}", self.session.active_panel().filter);
            self.last_error = self.session.execute(Command::Filter(filter)).is_err();
        }
    }

    /// Runs a command line, or puts it in the command line if it needs an argument.
    fn dispatch(&mut self, line: &str) {
        if line.ends_with(' ') {
            self.command_line = line.to_string();
            self.focus_command_line = true;
        } else {
            self.last_error = self.session.run_line(line).is_err();
        }
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, cmd_id: egui::Id) {
        let status_color = if self.last_error {
            Color32::LIGHT_RED
        } else {
            ui.visuals().weak_text_color()
        };
        if self.session.pending.is_some() {
            ui.label(RichText::new(&self.session.status).color(Color32::YELLOW));
            ui.horizontal(|ui| {
                if ui.button("Tak (Enter)").clicked() {
                    self.dispatch("confirm");
                }
                if ui.button("Nie (Esc)").clicked() {
                    self.dispatch("cancel");
                }
            });
        } else {
            ui.label(RichText::new(&self.session.status).color(status_color));
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(">").monospace());
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.command_line)
                    .id(cmd_id)
                    .desired_width(f32::INFINITY)
                    .hint_text("komenda, np. cd /tmp, mkdir Raporty, copy"),
            );
            if self.focus_command_line {
                response.request_focus();
                self.focus_command_line = false;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let line = std::mem::take(&mut self.command_line);
                if !line.trim().is_empty() {
                    self.last_error = self.session.run_line(&line).is_err();
                }
            }
        });

        ui.horizontal(|ui| {
            for (key, label, line) in FKEYS {
                if ui.button(format!("{key} {label}")).clicked() {
                    self.dispatch(line);
                }
            }
        });
    }

    /// Volume picker and editable path bar of one panel.
    fn header_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let is_active = self.session.active == index;
        let current_volume = self.session.volume_of(index);
        let mut chosen = None;
        let path_id = egui::Id::new(("path-bar", index));
        let editing = ui.ctx().memory(|m| m.has_focus(path_id));
        if !editing {
            self.path_text[index] = self.session.panels[index].cwd.display().to_string();
        }

        ui.horizontal(|ui| {
            let selected_text = current_volume
                .map(|i| self.session.volumes[i].name.clone())
                .unwrap_or_else(|| "?".to_string());
            egui::ComboBox::from_id_salt(("volume", index))
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (i, volume) in self.session.volumes.iter().enumerate() {
                        let label = format!("{} {}", i + 1, volume.name);
                        if ui
                            .selectable_label(current_volume == Some(i), label)
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                    }
                });

            let mut edit = egui::TextEdit::singleline(&mut self.path_text[index])
                .id(path_id)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY);
            if is_active {
                edit = edit.text_color(Color32::from_rgb(120, 200, 255));
            }
            let response = ui.add(edit);
            if response.gained_focus() {
                self.session.active = index;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.session.active = index;
                let line = format!("cd {}", self.path_text[index]);
                self.last_error = self.session.run_line(&line).is_err();
            }
        });

        if let Some(i) = chosen {
            self.session.active = index;
            self.last_error = self
                .session
                .execute(Command::Volume((i + 1).to_string()))
                .is_err();
        }
    }

    fn panel_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let is_active = self.session.active == index;
        self.header_ui(ui, index);
        let panel: &Panel = &self.session.panels[index];

        if let Some(err) = &panel.error {
            ui.colored_label(Color32::LIGHT_RED, err);
        }
        if !panel.filter.is_empty() {
            ui.label(
                RichText::new(format!("Filtr: {}  (Esc czyści)", panel.filter))
                    .color(Color32::YELLOW),
            );
        }
        ui.separator();

        let row_height = ui.spacing().interact_size.y;
        let stride = row_height + ui.spacing().item_spacing.y;
        let cursor = panel.cursor;
        let sort = panel.sort;
        let mut clicked = None;
        let mut entered = None;
        let mut sort_by = None;

        // Column header: click to sort by that column, click again to flip the direction.
        let (header_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_height),
            egui::Sense::hover(),
        );
        let columns = Columns::split(header_rect);
        let font = FontId::monospace(13.0);
        for (rect, key, label) in [
            (columns.name, SortKey::Name, "Nazwa"),
            (columns.ext, SortKey::Ext, "Typ"),
            (columns.size, SortKey::Size, "Rozmiar"),
            (columns.date, SortKey::Date, "Data"),
        ] {
            let response = ui.interact(
                rect,
                ui.id().with(("sort", index, label)),
                egui::Sense::click(),
            );
            let arrow = match (sort.key == key, sort.descending) {
                (false, _) => "",
                (true, false) => " ▲",
                (true, true) => " ▼",
            };
            let color = if response.hovered() {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            let (pos, align) = if key == SortKey::Size {
                (
                    egui::pos2(rect.right() - COLUMN_PAD, rect.center().y),
                    Align2::RIGHT_CENTER,
                )
            } else {
                (
                    egui::pos2(rect.left() + COLUMN_PAD, rect.center().y),
                    Align2::LEFT_CENTER,
                )
            };
            ui.painter()
                .text(pos, align, format!("{label}{arrow}"), font.clone(), color);
            if response.clicked() {
                sort_by = Some(key);
            }
        }

        egui::ScrollArea::vertical()
            .id_salt(("panel", index))
            .auto_shrink([false, false])
            .show_rows(ui, row_height, panel.entries.len(), |ui, range| {
                if is_active {
                    let y =
                        ui.min_rect().min.y + cursor.saturating_sub(range.start) as f32 * stride;
                    if cursor < range.start || cursor >= range.end {
                        let rect = Rect::from_min_size(
                            egui::pos2(ui.min_rect().min.x, y),
                            egui::vec2(1.0, row_height),
                        );
                        ui.scroll_to_rect(rect, None);
                    }
                }
                for row in range {
                    let entry = &panel.entries[row];
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_height),
                        egui::Sense::click(),
                    );
                    let is_selected = panel.selected.contains(&entry.path);
                    if row == cursor {
                        let fill = if is_active {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().widgets.inactive.weak_bg_fill
                        };
                        ui.painter().rect_filled(rect, 2.0, fill);
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            ui.visuals().widgets.hovered.weak_bg_fill,
                        );
                    }
                    let color = if is_selected {
                        Color32::YELLOW
                    } else if entry.hidden {
                        ui.visuals().weak_text_color()
                    } else {
                        ui.visuals().text_color()
                    };
                    let cols = Columns::split(rect);
                    let (stem, ext) = name_parts(&entry.name, entry.is_dir());
                    let name = if entry.is_dir() {
                        format!("[{stem}]")
                    } else {
                        stem.to_string()
                    };
                    let size = if entry.is_dir() {
                        "<DIR>".to_string()
                    } else {
                        human_size(entry.size)
                    };
                    let painter = ui.painter();
                    let y = rect.center().y;
                    // Each text is clipped to its own column so long names do not spill over.
                    let cell = |col: Rect, align: Align2, text: String| {
                        let x = if align == Align2::RIGHT_CENTER {
                            col.right() - COLUMN_PAD
                        } else {
                            col.left() + COLUMN_PAD
                        };
                        painter.with_clip_rect(col).text(
                            egui::pos2(x, y),
                            align,
                            text,
                            font.clone(),
                            color,
                        );
                    };
                    cell(cols.name, Align2::LEFT_CENTER, name);
                    cell(cols.ext, Align2::LEFT_CENTER, ext.to_string());
                    cell(cols.size, Align2::RIGHT_CENTER, size);
                    cell(
                        cols.date,
                        Align2::LEFT_CENTER,
                        format_modified(entry.modified),
                    );
                    if response.clicked() {
                        clicked = Some(row);
                    }
                    if response.double_clicked() {
                        entered = Some(row);
                    }
                }
            });

        if let Some(row) = clicked.or(entered) {
            self.session.active = index;
            self.session.panels[index].cursor = row;
        }
        if entered.is_some() {
            self.session.panels[index].enter();
        }
        if let Some(key) = sort_by {
            self.session.active = index;
            self.last_error = self.session.execute(Command::Sort(key, None)).is_err();
        }
    }
}

const COLUMN_PAD: f32 = 6.0;

/// Horizontal layout of a panel row: a flexible name column and fixed columns on the right.
struct Columns {
    name: Rect,
    ext: Rect,
    size: Rect,
    date: Rect,
}

impl Columns {
    const EXT: f32 = 56.0;
    const SIZE: f32 = 80.0;
    const DATE: f32 = 128.0;

    fn split(row: Rect) -> Columns {
        let date_left = row.right() - Self::DATE;
        let size_left = date_left - Self::SIZE;
        let ext_left = size_left - Self::EXT;
        let cut = |left: f32, right: f32| {
            Rect::from_min_max(egui::pos2(left, row.top()), egui::pos2(right, row.bottom()))
        };
        Columns {
            name: cut(row.left(), ext_left.max(row.left())),
            ext: cut(ext_left.max(row.left()), size_left.max(row.left())),
            size: cut(size_left.max(row.left()), date_left.max(row.left())),
            date: cut(date_left.max(row.left()), row.right()),
        }
    }
}

/// Key name as the engine's `Keymap` knows it: `F5`, `Alt+ArrowLeft`, `Ctrl+L`.
fn key_name(key: egui::Key, modifiers: egui::Modifiers) -> String {
    let mut name = String::new();
    if modifiers.ctrl {
        name.push_str("Ctrl+");
    }
    if modifiers.alt {
        name.push_str("Alt+");
    }
    if modifiers.shift {
        name.push_str("Shift+");
    }
    name.push_str(&format!("{key:?}"));
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_carry_modifiers() {
        let none = egui::Modifiers::NONE;
        assert_eq!(key_name(egui::Key::F5, none), "F5");
        assert_eq!(
            key_name(egui::Key::ArrowLeft, egui::Modifiers::ALT),
            "Alt+ArrowLeft"
        );
        assert_eq!(key_name(egui::Key::L, egui::Modifiers::CTRL), "Ctrl+L");
    }

    #[test]
    fn columns_tile_the_row_without_overlap() {
        let row = Rect::from_min_max(egui::pos2(10.0, 0.0), egui::pos2(500.0, 20.0));
        let c = Columns::split(row);
        assert_eq!(c.name.left(), 10.0);
        assert_eq!(c.name.right(), c.ext.left());
        assert_eq!(c.ext.right(), c.size.left());
        assert_eq!(c.size.right(), c.date.left());
        assert_eq!(c.date.right(), 500.0);
    }

    #[test]
    fn narrow_rows_do_not_produce_inverted_columns() {
        let row = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 20.0));
        let c = Columns::split(row);
        for rect in [c.name, c.ext, c.size, c.date] {
            assert!(rect.width() >= 0.0);
        }
    }
}
