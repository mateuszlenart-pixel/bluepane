//! The disk usage map: the tiles of `Session::map_tiles`, drawn by hand. A click on a folder
//! tile goes into it (`map into`); the engine does the counting and the layout.

use bluepane_engine::human_size;
use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, Vec2};

use crate::BluepaneUi;

impl BluepaneUi {
    pub(crate) fn map_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            match key.as_str() {
                "Escape" => self.dispatch("map close"),
                "Backspace" => self.dispatch("map up"),
                _ => {}
            }
        }
    }

    pub(crate) fn map_ui(&mut self, ui: &mut egui::Ui) {
        let Some(root) = self.session.diskmap.as_ref().map(|m| m.root.clone()) else {
            return;
        };
        let running = self
            .session
            .diskmap
            .as_ref()
            .is_some_and(|m| m.is_running());
        let mut line: Option<String> = None;

        ui.label(RichText::new("Mapa zajętości").strong());
        ui.horizontal(|ui| {
            if ui.button("W górę (Backspace)").clicked() {
                line = Some("map up".into());
            }
            if ui.button("Pokaż w panelu").clicked() {
                line = Some("map show".into());
            }
            if ui.button("Zamknij (Esc)").clicked() {
                line = Some("map close".into());
            }
            ui.label(RichText::new(root.display().to_string()).color(self.theme.muted));
        });
        if let Some((total, pending)) = self.session.map_summary() {
            let text = if running {
                format!(
                    "Razem {}; liczę jeszcze katalogów: {pending}…",
                    human_size(total)
                )
            } else {
                format!("Razem {}", human_size(total))
            };
            ui.label(RichText::new(text).color(self.theme.muted));
        }

        let size = Vec2::new(
            ui.available_width(),
            (ui.available_height() - 4.0).max(60.0),
        );
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let (tiles, left_out) = self.session.map_tiles(rect.width(), rect.height());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, self.theme.bg);
        for (i, tile) in tiles.iter().enumerate() {
            let tile_rect = egui::Rect::from_min_size(
                rect.min + Vec2::new(tile.x, tile.y),
                Vec2::new(tile.w, tile.h),
            );
            let shown = tile_rect.shrink(1.0);
            if shown.width() < 2.0 || shown.height() < 2.0 {
                continue;
            }
            let id = ui.id().with(("map-tile", i));
            let response = ui.interact(shown, id, Sense::click());
            let base = if tile.item.is_dir {
                // Folders take hues from the accent colour; files are a calm grey-blue.
                let hue = (i as f32 * 0.137 + 0.55).fract();
                egui::ecolor::Hsva::new(
                    hue,
                    0.45,
                    if ui.visuals().dark_mode { 0.55 } else { 0.85 },
                    1.0,
                )
                .into()
            } else {
                self.theme.header
            };
            let fill = if response.hovered() {
                self.theme.hover
            } else {
                base
            };
            painter.rect(
                shown,
                2.0,
                fill,
                Stroke::new(1.0, self.theme.border),
                egui::StrokeKind::Inside,
            );
            if shown.width() > 50.0 && shown.height() > 18.0 {
                let label = format!(
                    "{}  {}",
                    tile.item.name,
                    human_size(tile.item.size.unwrap_or(0))
                );
                painter.with_clip_rect(shown.shrink(2.0)).text(
                    shown.min + Vec2::new(4.0, 3.0),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(12.5),
                    if ui.visuals().dark_mode {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    },
                );
            }
            let hover = format!(
                "{}{}\n{}",
                tile.item.name,
                if tile.item.is_dir { "/" } else { "" },
                human_size(tile.item.size.unwrap_or(0))
            );
            let response = response.on_hover_text(hover);
            if tile.item.is_dir && response.clicked() {
                line = Some(format!("map into {}", tile.item.name));
            }
        }
        if left_out > 0 {
            ui.label(
                RichText::new(format!(
                    "Pominięto mniejsze lub jeszcze nieobliczone wpisy: {left_out}"
                ))
                .color(self.theme.muted),
            );
        }
        if running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(line) = line {
            self.dispatch(&line);
        }
    }
}
