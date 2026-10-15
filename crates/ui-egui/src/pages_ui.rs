//! A document shown as pages (F3 on a PDF, or on a Word / Excel / OpenDocument file when
//! LibreOffice is installed). The engine renders; this only turns pictures into textures and
//! sends `page` and `pagezoom` commands.

use bluepane_engine::{Command, PageMove, PageState, PageZoom, ZoomChange};
use egui::{Color32, RichText};

use crate::BluepaneUi;

/// What the texture on screen shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextureKey {
    page: usize,
    width: u32,
    height: u32,
    /// Distinguishes documents that happen to have equal page sizes.
    source: std::path::PathBuf,
}

impl BluepaneUi {
    pub(crate) fn pages_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            if let Some(line) = self.session.keymap.lookup_pages(key).map(str::to_owned) {
                self.dispatch(&line);
            }
        }
    }

    pub(crate) fn pages_ui(&mut self, ui: &mut egui::Ui) {
        let Some(viewer) = self.session.pages.as_ref() else {
            return;
        };
        let name = viewer
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (page, pages, zoom, state) = (viewer.page, viewer.pages(), viewer.zoom, viewer.state());
        let mut commands: Vec<Command> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).strong());
            ui.separator();
            if ui
                .add_enabled(page > 0, egui::Button::new("◀"))
                .on_hover_text("poprzednia strona (PgUp)")
                .clicked()
            {
                commands.push(Command::Page(PageMove::Prev));
            }
            ui.label(format!("strona {} z {}", page + 1, pages.max(1)));
            if ui
                .add_enabled(page + 1 < pages, egui::Button::new("▶"))
                .on_hover_text("następna strona (PgDn, Spacja)")
                .clicked()
            {
                commands.push(Command::Page(PageMove::Next));
            }
            ui.separator();
            if ui.button("−").on_hover_text("pomniejsz (-)").clicked() {
                commands.push(Command::PageZoom(ZoomChange::Out));
            }
            ui.label(match zoom {
                PageZoom::Fit => "dopasowanie do szerokości".to_string(),
                PageZoom::Percent(p) => format!("{p}%"),
            });
            if ui.button("+").on_hover_text("powiększ (+)").clicked() {
                commands.push(Command::PageZoom(ZoomChange::In));
            }
            if ui
                .button("Dopasuj")
                .on_hover_text("do szerokości okna (0)")
                .clicked()
            {
                commands.push(Command::PageZoom(ZoomChange::Fit));
            }
            ui.separator();
            if ui.button("Zamknij (Esc)").clicked() {
                commands.push(Command::PageClose);
            }
        });
        ui.label(
            RichText::new("PgUp/PgDn lub strzałki ←/→: strony · Home/End: pierwsza/ostatnia · +/−/0: powiększenie · przesuwanie: przeciągnij myszą, paski przewijania lub Shift+kółko · Esc/Q: zamknij")
                .weak(),
        );
        ui.separator();

        match state {
            PageState::Converting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Konwersja dokumentu do podglądu (LibreOffice)… to może potrwać kilka sekund");
                });
            }
            PageState::Failed(error) => {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            PageState::Ready => self.page_picture(ui, zoom),
        }
        for command in commands {
            let _ = self.session.execute(command);
        }
    }

    fn page_picture(&mut self, ui: &mut egui::Ui, zoom: PageZoom) {
        let Some(viewer) = self.session.pages.as_ref() else {
            return;
        };
        paint_page(ui, viewer, zoom, &mut self.page_texture);
    }
}

/// Draws the current page of `viewer` as a picture (sharper once it is rendered at the size
/// wanted; until then the old picture is stretched). `texture` keeps the picture on the GPU.
pub(crate) fn paint_page(
    ui: &mut egui::Ui,
    viewer: &bluepane_engine::PageViewer,
    zoom: PageZoom,
    texture_slot: &mut Option<(TextureKey, egui::TextureHandle)>,
) {
    let ppp = ui.ctx().pixels_per_point();
    // How wide the page should be on screen, in egui points.
    let Some((page_width, _)) = viewer.page_size() else {
        return;
    };
    let wanted_points = match zoom {
        PageZoom::Fit => (ui.available_width() - 24.0).max(100.0),
        PageZoom::Percent(p) => page_width * 96.0 / 72.0 * p as f32 / 100.0,
    };
    viewer.want((wanted_points * ppp).round() as u32);
    let Some(rendered) = viewer.rendered() else {
        ui.spinner();
        return;
    };
    let (image, source) = (rendered.image.clone(), viewer.source.clone());
    let key = TextureKey {
        page: rendered.page,
        width: image.width,
        height: image.height,
        source,
    };
    if texture_slot.as_ref().map(|(k, _)| k) != Some(&key) {
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        );
        let handle = ui
            .ctx()
            .load_texture("bluepane-page", color, egui::TextureOptions::LINEAR);
        *texture_slot = Some((key, handle));
    }
    let Some((_, texture)) = texture_slot.as_ref() else {
        return;
    };
    // Until the sharper picture is ready the old one is stretched to the wanted size.
    let aspect = image.height as f32 / image.width as f32;
    let size = egui::vec2(wanted_points, wanted_points * aspect);
    let fits = size.x <= ui.available_width();
    egui::ScrollArea::both()
        .auto_shrink(false)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            // A page wider than the window is not centred, or its left edge would be cut off.
            if fits {
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((texture.id(), size)));
                });
            } else {
                ui.add(egui::Image::new((texture.id(), size)));
            }
        });
}
