//! The grid view of a panel (`viewmode grid`, Ctrl+Shift+G): tiles with a thumbnail for
//! pictures and an icon for the rest. Clicks, marks, the context menu and dragging work as
//! in the list, through the same commands.

use std::collections::HashMap;
use std::path::PathBuf;

use bluepane_engine::{class_of, FileClass, Panel, Thumbnails};
use egui::{Align2, Color32, FontId, Rect, Sense, TextureHandle};

use crate::theme::Theme;
use crate::DropZone;

pub(crate) const CELL_WIDTH: f32 = 144.0;
const CELL_HEIGHT: f32 = 164.0;
const PICTURE: f32 = 128.0;
/// Textures kept for thumbnails; beyond this the oldest set is dropped and rebuilt.
const TEXTURES_MAX: usize = 800;

/// What the user did in the grid this frame (rows are entry indices, from 0).
#[derive(Default)]
pub(crate) struct GridInput {
    pub clicked: Option<(usize, (bool, bool))>,
    pub entered: Option<usize>,
    pub right_clicked: Option<usize>,
    pub drag_started: Option<usize>,
}

/// One texture per thumbnail, remembered by the address of the engine's picture so a new
/// thumbnail (the file changed) replaces the old texture.
pub(crate) type Textures = HashMap<PathBuf, (usize, TextureHandle)>;

/// How many tiles fit in a row of this width.
pub(crate) fn columns(width: f32) -> usize {
    ((width / CELL_WIDTH).floor() as usize).max(1)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn grid(
    ui: &mut egui::Ui,
    index: usize,
    panel: &Panel,
    is_active: bool,
    theme: &Theme,
    thumbs: &Thumbnails,
    textures: &mut Textures,
    zones: &mut Vec<DropZone>,
    max_height: f32,
) -> GridInput {
    let mut input = GridInput::default();
    let cols = columns(ui.available_width());
    let rows = panel.entries.len().div_ceil(cols);
    let cursor = panel.cursor;
    let pictures_here = panel.remote.is_none() && panel.archive.is_none();
    if textures.len() > TEXTURES_MAX {
        textures.clear();
    }
    egui::ScrollArea::vertical()
        .id_salt(("grid", index))
        .auto_shrink([false, false])
        .max_height(max_height)
        .show_rows(ui, CELL_HEIGHT, rows, |ui, range| {
            if is_active {
                let cursor_row = cursor / cols;
                if cursor_row < range.start || cursor_row >= range.end {
                    let y = ui.min_rect().min.y
                        + (cursor_row as f32 - range.start as f32)
                            * (CELL_HEIGHT + ui.spacing().item_spacing.y);
                    ui.scroll_to_rect(
                        Rect::from_min_size(
                            egui::pos2(ui.min_rect().min.x, y),
                            egui::vec2(1.0, CELL_HEIGHT),
                        ),
                        None,
                    );
                }
            }
            for row in range {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let i = row * cols + col;
                        let Some(entry) = panel.entries.get(i) else {
                            break;
                        };
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(CELL_WIDTH - ui.spacing().item_spacing.x, CELL_HEIGHT),
                            Sense::click_and_drag(),
                        );
                        if entry.is_dir() && !bluepane_engine::is_parent_entry(entry) {
                            zones.push(DropZone {
                                rect,
                                panel: index,
                                row: Some(i + 1),
                            });
                        }
                        let selected = panel.selected.contains(&entry.path);
                        let painter = ui.painter();
                        if i == cursor {
                            let (fill, outline) = if is_active {
                                (theme.cursor_fill, theme.cursor_stroke)
                            } else {
                                (theme.cursor_fill_inactive, theme.cursor_stroke_inactive)
                            };
                            painter.rect(
                                rect,
                                4.0,
                                fill,
                                egui::Stroke::new(1.0, outline),
                                egui::StrokeKind::Inside,
                            );
                        } else if response.hovered() {
                            painter.rect_filled(rect, 4.0, theme.hover);
                        }
                        let picture_area = Rect::from_center_size(
                            egui::pos2(rect.center().x, rect.top() + 6.0 + PICTURE / 2.0),
                            egui::vec2(PICTURE, PICTURE),
                        );
                        let class = class_of(entry);
                        let thumb = (pictures_here && class == FileClass::Image)
                            .then(|| thumbs.get(&entry.path, entry.modified))
                            .flatten();
                        match thumb {
                            Some(picture) => {
                                let address = std::sync::Arc::as_ptr(&picture) as usize;
                                let fresh = textures
                                    .get(&entry.path)
                                    .is_some_and(|(a, _)| *a == address);
                                if !fresh {
                                    let image = egui::ColorImage::from_rgba_unmultiplied(
                                        [picture.width as usize, picture.height as usize],
                                        &picture.rgba,
                                    );
                                    let texture = ui.ctx().load_texture(
                                        format!("thumb-{}", entry.path.display()),
                                        image,
                                        egui::TextureOptions::LINEAR,
                                    );
                                    textures.insert(entry.path.clone(), (address, texture));
                                }
                                if let Some((_, texture)) = textures.get(&entry.path) {
                                    let size = texture.size_vec2();
                                    let shown = Rect::from_center_size(picture_area.center(), size);
                                    ui.painter().image(
                                        texture.id(),
                                        shown,
                                        Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        Color32::WHITE,
                                    );
                                }
                            }
                            None => {
                                let (icon, color) = theme.look(class);
                                ui.painter().text(
                                    picture_area.center(),
                                    Align2::CENTER_CENTER,
                                    icon,
                                    FontId::proportional(48.0),
                                    color,
                                );
                            }
                        }
                        let name_color = if selected { theme.selected } else { theme.text };
                        let name_rect = Rect::from_min_max(
                            egui::pos2(rect.left() + 4.0, picture_area.bottom() + 4.0),
                            egui::pos2(rect.right() - 4.0, rect.bottom()),
                        );
                        ui.painter().with_clip_rect(name_rect).text(
                            name_rect.center_top(),
                            Align2::CENTER_TOP,
                            &entry.name,
                            FontId::proportional(12.5),
                            name_color,
                        );
                        if response.drag_started() {
                            input.drag_started = Some(i);
                        }
                        if response.clicked() {
                            let m = ui.input(|i| i.modifiers);
                            input.clicked = Some((i, (m.command, m.shift)));
                        }
                        if response.double_clicked() {
                            input.entered = Some(i);
                        }
                        if response.secondary_clicked() {
                            input.right_clicked = Some(i);
                        }
                        response.on_hover_text(&entry.name);
                    }
                });
            }
        });
    input
}
