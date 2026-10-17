//! The right-click menu (and Shift+F10): the engine says which actions fit what is under
//! the pointer, this file draws them where the click was and runs the chosen command line.

use egui::Align2;

use crate::{BluepaneUi, ContextMenu};

impl BluepaneUi {
    /// Opens the menu at `pos` for the row under the cursor (`on_row`) or the empty list.
    pub(crate) fn open_context_menu(&mut self, pos: egui::Pos2, on_row: bool) {
        let items = self.session.context_menu(on_row);
        self.context_menu = Some(ContextMenu {
            pos,
            items,
            opened_in: self.frame,
        });
    }

    pub(crate) fn context_menu_popup(&mut self, ctx: &egui::Context) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        let menu_opened_in = menu.opened_in;
        let mut chosen: Option<String> = None;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let area = egui::Area::new(egui::Id::new("context-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.pos)
            .pivot(Align2::LEFT_TOP)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    for item in &menu.items {
                        if item.separator {
                            ui.separator();
                        }
                        let mut button = egui::Button::new(&item.label).frame(false);
                        if let Some(key) = &item.shortcut {
                            button = button.shortcut_text(key);
                        }
                        if ui.add_sized([ui.available_width(), 22.0], button).clicked() {
                            chosen = Some(item.line.clone());
                        }
                    }
                });
            });
        if area.response.clicked_elsewhere() && menu_opened_in != self.frame {
            close = true;
        }
        if chosen.is_some() || close {
            self.context_menu = None;
        }
        if let Some(line) = chosen {
            self.dispatch(&line);
        }
    }
}
