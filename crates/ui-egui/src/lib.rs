//! egui front end.
//!
//! The UI only translates input into command lines and draws the engine's state.
//! It contains no file system logic.

use std::time::{Duration, SystemTime};

use bluepane_engine::{Command, Panel, Session};
use egui::{Color32, RichText};

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
        ui.label(RichText::new(&self.session.status).color(status_color));

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
        let mut clicked = None;
        let mut entered = None;

        egui::ScrollArea::vertical()
            .id_salt(("panel", index))
            .auto_shrink([false, false])
            .show_rows(ui, row_height, panel.entries.len(), |ui, range| {
                if is_active {
                    let y =
                        ui.min_rect().min.y + cursor.saturating_sub(range.start) as f32 * stride;
                    if cursor < range.start || cursor >= range.end {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(ui.min_rect().min.x, y),
                            egui::vec2(1.0, row_height),
                        );
                        ui.scroll_to_rect(rect, None);
                    }
                }
                for row in range {
                    let entry = &panel.entries[row];
                    let marker = if panel.selected.contains(&entry.path) {
                        "*"
                    } else {
                        " "
                    };
                    let name = if entry.is_dir() {
                        format!("[{}]", entry.name)
                    } else {
                        entry.name.clone()
                    };
                    let size = if entry.is_dir() {
                        "<DIR>".to_string()
                    } else {
                        human_size(entry.size)
                    };
                    let text = format!(
                        "{marker}{name:<42} {size:>10}  {}",
                        format_age(entry.modified)
                    );
                    let mut rich = RichText::new(text).monospace();
                    if panel.selected.contains(&entry.path) {
                        rich = rich.color(Color32::YELLOW);
                    } else if entry.hidden {
                        rich = rich.color(ui.visuals().weak_text_color());
                    }
                    let response = ui.selectable_label(is_active && row == cursor, rich);
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

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Coarse "how long ago" text; real date formatting arrives with the columns work (stage 3).
fn format_age(modified: Option<SystemTime>) -> String {
    let Some(modified) = modified else {
        return String::new();
    };
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    match age {
        0..=59 => "teraz".to_string(),
        60..=3599 => format!("{} min temu", age / 60),
        3600..=86_399 => format!("{} godz. temu", age / 3600),
        _ => format!("{} dni temu", age / 86_400),
    }
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
    fn sizes_are_human_readable() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn age_buckets() {
        assert_eq!(format_age(None), "");
        assert_eq!(format_age(Some(SystemTime::now())), "teraz");
    }
}
