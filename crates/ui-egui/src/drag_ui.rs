//! Dragging marked entries to the other panel or onto a folder, and dropping files from the
//! system's file manager. The engine does the work (`dragto`, `drop`); this file follows the
//! pointer, shows where the drop would land and turns the release into a command line.

use egui::{Align2, Color32, FontId, Stroke};

use crate::{BluepaneUi, DropZone};

impl BluepaneUi {
    /// The zone under the pointer; the last one drawn wins (a folder row over its list).
    fn zone_at(&self, pos: egui::Pos2) -> Option<DropZone> {
        self.last_zones
            .iter()
            .rev()
            .find(|z| z.rect.contains(pos))
            .copied()
    }

    pub(crate) fn drag_overlay(&mut self, ctx: &egui::Context) {
        let pointer = ctx.pointer_latest_pos();
        let zone = pointer.and_then(|p| self.zone_at(p));

        // Files coming from the system's file manager.
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if hovering {
            if let Some(zone) = zone {
                self.paint_target(ctx, zone, "Upuść, aby skopiować tutaj");
            }
        }
        if !dropped.is_empty() {
            let (panel, row) = zone.map_or((self.session.active, None), |z| (z.panel, z.row));
            let mut line = format!("drop {}", panel + 1);
            if let Some(row) = row {
                line.push_str(&format!(" {row}"));
            }
            for path in &dropped {
                line.push_str(&format!(" \"{}\"", path.display()));
            }
            self.session.active = panel;
            let _ = self.session.run_line(&line);
        }

        // Marked entries being dragged between the panels.
        let Some(drag) = self.drag.clone() else {
            return;
        };
        ctx.request_repaint();
        let shift = ctx.input(|i| i.modifiers.shift);
        let valid = zone.filter(|z| z.panel != drag.from || z.row.is_some());
        let verb = if shift { "Przenieś" } else { "Kopiuj" };
        let what = if drag.count == 1 {
            drag.first.clone()
        } else {
            format!("{} elementów", drag.count)
        };
        if let Some(zone) = valid {
            let target = match zone.row {
                Some(_) => "do tego katalogu",
                None => "do tego panelu",
            };
            self.paint_target(ctx, zone, &format!("{verb} {target}"));
        }
        if let Some(pos) = pointer {
            let text = format!("{verb}: {what}");
            egui::Area::new(egui::Id::new("drag-label"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos + egui::vec2(14.0, 14.0))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(text);
                    });
                });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.drag = None;
        } else if ctx.input(|i| i.pointer.any_released()) {
            self.drag = None;
            if let Some(zone) = valid {
                let mut line = format!(
                    "dragto {} {}",
                    if shift { "move" } else { "copy" },
                    zone.panel + 1
                );
                if let Some(row) = zone.row {
                    line.push_str(&format!(" {row}"));
                }
                self.session.active = drag.from;
                let _ = self.session.run_line(&line);
            }
        }
    }

    /// Outlines the drop zone and says what the drop would do.
    fn paint_target(&self, ctx: &egui::Context, zone: DropZone, text: &str) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("drop-target"),
        ));
        painter.rect_stroke(
            zone.rect.shrink(1.0),
            2.0,
            Stroke::new(2.0, self.theme.accent),
            egui::StrokeKind::Inside,
        );
        if zone.row.is_none() {
            painter.text(
                zone.rect.center_bottom() - egui::vec2(0.0, 12.0),
                Align2::CENTER_BOTTOM,
                text,
                FontId::proportional(14.0),
                Color32::WHITE,
            );
        }
    }
}
