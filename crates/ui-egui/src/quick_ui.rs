//! Quick View (Ctrl+Q): the other panel's place shows the file under the cursor. It only
//! draws what the engine prepared and sends `quick scroll` / `quick page` commands.

use bluepane_engine::{human_size, QuickContent};
use egui::{Align2, Color32, FontId, RichText, Stroke};

use crate::pages_ui::paint_page;
use crate::BluepaneUi;

impl BluepaneUi {
    /// Draws the preview in the place of panel `index` (the one that is not active).
    pub(crate) fn quick_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let theme = self.theme.clone();
        let mut scroll: Option<i64> = None;
        let mut page: Option<i32> = None;
        egui::Frame::new()
            .fill(theme.panel_inactive)
            .stroke(Stroke::new(1.0, theme.accent.gamma_multiply(0.6)))
            .corner_radius(4)
            .inner_margin(6)
            .show(ui, |ui| {
                let Some(quick) = self.session.quick.as_mut() else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(egui_phosphor::regular::EYE).color(theme.accent));
                    ui.label(RichText::new(&quick.title).strong().color(theme.text));
                    if let QuickContent::Pages(viewer) = &quick.content {
                        let (n, total) = (viewer.page + 1, viewer.pages().max(1));
                        if ui
                            .add_enabled(
                                viewer.page > 0,
                                egui::Button::new(egui_phosphor::regular::CARET_LEFT),
                            )
                            .clicked()
                        {
                            page = Some(-1);
                        }
                        ui.label(RichText::new(format!("{n} / {total}")).color(theme.muted));
                        if ui
                            .add_enabled(
                                n < total,
                                egui::Button::new(egui_phosphor::regular::CARET_RIGHT),
                            )
                            .clicked()
                        {
                            page = Some(1);
                        }
                    }
                });
                ui.add(egui::Separator::default().spacing(4.0));
                let font = FontId::monospace(13.0);
                let glyph = ui
                    .painter()
                    .layout_no_wrap("M".to_string(), font.clone(), Color32::WHITE)
                    .size();
                let available = ui.available_size();
                let rows = ((available.y - 4.0) / glyph.y).floor().max(1.0) as usize;
                let cols = (available.x / glyph.x).floor().max(8.0) as usize;
                match &mut quick.content {
                    QuickContent::Note(text) => {
                        ui.add(
                            egui::Label::new(RichText::new(text.as_str()).color(theme.muted))
                                .wrap(),
                        );
                    }
                    QuickContent::Pages(viewer) => {
                        paint_page(
                            ui,
                            viewer,
                            bluepane_engine::PageZoom::Fit,
                            &mut self.page_texture,
                        );
                    }
                    QuickContent::Text(viewer) => {
                        viewer.set_viewport(rows, cols);
                        let head = format!("{}   {}%", human_size(viewer.size()), viewer.percent());
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(available.x, rows as f32 * glyph.y),
                            egui::Sense::hover(),
                        );
                        let painter = ui.painter_at(rect);
                        for (i, row) in viewer.visible_rows().iter().enumerate() {
                            painter.text(
                                egui::pos2(rect.left() + 2.0, rect.top() + i as f32 * glyph.y),
                                Align2::LEFT_TOP,
                                &row.text,
                                font.clone(),
                                theme.text,
                            );
                        }
                        painter.text(
                            rect.right_bottom() - egui::vec2(4.0, 2.0),
                            Align2::RIGHT_BOTTOM,
                            head,
                            FontId::proportional(11.0),
                            theme.muted,
                        );
                        scroll = wheel_rows(ui, &response, glyph.y, rows);
                    }
                    QuickContent::Lines { lines, top } => {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(available.x, rows as f32 * glyph.y),
                            egui::Sense::hover(),
                        );
                        let painter = ui.painter_at(rect);
                        for (i, line) in lines.iter().skip(*top).take(rows).enumerate() {
                            painter.text(
                                egui::pos2(rect.left() + 2.0, rect.top() + i as f32 * glyph.y),
                                Align2::LEFT_TOP,
                                line,
                                font.clone(),
                                theme.text,
                            );
                        }
                        scroll = wheel_rows(ui, &response, glyph.y, rows);
                    }
                }
            });
        let _ = index;
        if let Some(delta) = scroll {
            let _ = self.session.run_line(&format!("quick scroll {delta}"));
        }
        if let Some(delta) = page {
            let _ = self.session.run_line(&format!("quick page {delta}"));
        }
    }
}

/// Mouse wheel over the preview as whole rows (about three per notch); `None` when not used.
fn wheel_rows(
    ui: &egui::Ui,
    response: &egui::Response,
    row_height: f32,
    rows: usize,
) -> Option<i64> {
    if !response.hovered() {
        return None;
    }
    let delta: f32 = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                    egui::MouseWheelUnit::Point => delta.y / row_height,
                    egui::MouseWheelUnit::Line => delta.y * 3.0,
                    egui::MouseWheelUnit::Page => delta.y * rows as f32,
                }),
                _ => None,
            })
            .sum()
    });
    if delta == 0.0 {
        return None;
    }
    // Wheel up scrolls towards the start of the file.
    let step = (-delta).round() as i64;
    Some(if step == 0 {
        -delta.signum() as i64
    } else {
        step
    })
}
