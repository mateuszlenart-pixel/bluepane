//! The completions under the path bar (the omnibox): the engine offers them, a click takes one.

use crate::BluepaneUi;

impl BluepaneUi {
    /// Completions under the path bar while it is being edited; a click puts one in the field.
    pub(crate) fn omnibox_hints_popup(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        field: &egui::Response,
    ) {
        let hints = self.session.omnibox_hints(&self.path_text[index]);
        if hints.is_empty() {
            return;
        }
        let mut chosen: Option<String> = None;
        egui::Area::new(egui::Id::new(("omnibox-hints", index)))
            .order(egui::Order::Foreground)
            .fixed_pos(field.rect.left_bottom())
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(field.rect.width().min(520.0));
                    for hint in &hints {
                        if ui.selectable_label(false, &hint.label).clicked() {
                            chosen = Some(hint.completion.clone());
                        }
                    }
                });
            });
        if let Some(text) = chosen {
            self.path_text[index] = text;
            self.path_edit[index] = true;
        }
    }
}
