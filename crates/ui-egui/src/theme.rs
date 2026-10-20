//! Colour themes. A theme is a palette of named colours; `apply` turns it into egui's
//! `Visuals` and spacing, and the drawing code asks the palette for panel, cursor and text
//! colours instead of using fixed ones. The engine only knows the theme's name
//! (`bluepane_engine::THEMES`).

use egui::{Color32, CornerRadius, Stroke};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    /// A dark palette (egui starts from its dark or light visuals accordingly).
    pub dark: bool,
    /// Behind everything (window background).
    pub bg: Color32,
    /// The active panel.
    pub panel: Color32,
    /// The panel that does not have the keyboard.
    pub panel_inactive: Color32,
    /// Headers (tabs, column titles) and idle buttons.
    pub header: Color32,
    /// Every second row of a list.
    pub row_alt: Color32,
    pub hover: Color32,
    /// The row under the cursor in the active panel.
    pub cursor_fill: Color32,
    pub cursor_stroke: Color32,
    /// ...and in the inactive one.
    pub cursor_fill_inactive: Color32,
    pub cursor_stroke_inactive: Color32,
    pub text: Color32,
    pub muted: Color32,
    /// Marked files (Insert, Space).
    pub selected: Color32,
    pub dir: Color32,
    /// Colours by kind of file (icon and name).
    pub archive: Color32,
    pub document: Color32,
    pub image: Color32,
    pub media: Color32,
    pub code: Color32,
    pub executable: Color32,
    pub accent: Color32,
    pub border: Color32,
    pub warn: Color32,
    pub error: Color32,
    /// Something good or new: copy to the right, characters added by a rename.
    pub ok: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Theme {
    /// Modern Dark: graphite panels, blue accent, amber marks.
    pub fn dark() -> Theme {
        Theme {
            name: "dark",
            dark: true,
            bg: rgb(0x12, 0x14, 0x1c),
            panel: rgb(0x1a, 0x1d, 0x28),
            panel_inactive: rgb(0x15, 0x17, 0x20),
            header: rgb(0x22, 0x26, 0x34),
            row_alt: rgb(0x1e, 0x22, 0x2f),
            hover: rgb(0x29, 0x2e, 0x42),
            cursor_fill: Color32::from_rgba_premultiplied(13, 30, 55, 56),
            cursor_stroke: rgb(0x3d, 0x8b, 0xfd),
            cursor_fill_inactive: rgb(0x24, 0x28, 0x38),
            cursor_stroke_inactive: rgb(0x3a, 0x40, 0x58),
            text: rgb(0xe2, 0xe8, 0xf0),
            muted: rgb(0x7c, 0x85, 0x9e),
            selected: rgb(0xff, 0xb8, 0x4d),
            dir: rgb(0x7d, 0xcf, 0xff),
            archive: rgb(0xc0, 0x9b, 0xf5),
            document: rgb(0xf0, 0x9a, 0x5a),
            image: rgb(0xf4, 0x8f, 0xc0),
            media: rgb(0xe8, 0x7c, 0x9c),
            code: rgb(0x6e, 0xe0, 0xb4),
            executable: rgb(0xe8, 0xd0, 0x6c),
            accent: rgb(0x3d, 0x8b, 0xfd),
            border: rgb(0x2c, 0x31, 0x44),
            warn: rgb(0xfb, 0xbf, 0x24),
            error: rgb(0xf8, 0x71, 0x71),
            ok: rgb(0x6e, 0xe7, 0x9a),
        }
    }

    /// Light: white panels, dark text, the same blue accent.
    pub fn light() -> Theme {
        Theme {
            name: "light",
            dark: false,
            bg: rgb(0xee, 0xf0, 0xf5),
            panel: rgb(0xff, 0xff, 0xff),
            panel_inactive: rgb(0xf5, 0xf6, 0xfa),
            header: rgb(0xe3, 0xe7, 0xef),
            row_alt: rgb(0xf3, 0xf5, 0xf9),
            hover: rgb(0xd9, 0xe2, 0xf3),
            cursor_fill: Color32::from_rgba_premultiplied(8, 22, 40, 40),
            cursor_stroke: rgb(0x1f, 0x6f, 0xeb),
            cursor_fill_inactive: rgb(0xe6, 0xea, 0xf2),
            cursor_stroke_inactive: rgb(0xb4, 0xbc, 0xcf),
            text: rgb(0x16, 0x1b, 0x26),
            muted: rgb(0x57, 0x60, 0x74),
            selected: rgb(0x9a, 0x50, 0x00),
            dir: rgb(0x0a, 0x5f, 0x94),
            archive: rgb(0x70, 0x34, 0xb8),
            document: rgb(0x9a, 0x45, 0x00),
            image: rgb(0xa8, 0x1c, 0x62),
            media: rgb(0xa0, 0x18, 0x40),
            code: rgb(0x08, 0x6b, 0x4a),
            executable: rgb(0x6b, 0x58, 0x00),
            accent: rgb(0x1f, 0x6f, 0xeb),
            border: rgb(0xc6, 0xcc, 0xdb),
            warn: rgb(0x8a, 0x5a, 0x00),
            error: rgb(0xb7, 0x1c, 0x1c),
            ok: rgb(0x13, 0x6f, 0x2e),
        }
    }

    /// The palette named `name`; an unknown name gives the default theme. `system` is not a
    /// palette: the window resolves it with [`Theme::resolve`] first.
    pub fn by_name(name: &str) -> Theme {
        match name {
            "light" => Theme::light(),
            _ => Theme::dark(),
        }
    }

    /// The palette name to use for the saved theme `name`: `system` follows the operating
    /// system's setting (`system_dark`; dark when it cannot be told).
    pub fn resolve(name: &str, system_dark: Option<bool>) -> &str {
        match name {
            "system" if system_dark == Some(false) => "light",
            "system" => "dark",
            other => other,
        }
    }

    /// The same palette with the user's accent colour (cursor frame, focus, links, borders of
    /// hovered widgets).
    pub fn with_accent(mut self, accent: Option<[u8; 3]>) -> Theme {
        if let Some([r, g, b]) = accent {
            let color = rgb(r, g, b);
            self.accent = color;
            self.cursor_stroke = color;
        }
        self
    }

    /// The icon and the colour of a kind of file (the colour is for the name too).
    pub fn look(&self, class: bluepane_engine::FileClass) -> (&'static str, Color32) {
        use bluepane_engine::FileClass as C;
        use egui_phosphor::regular as i;
        match class {
            C::Parent => (i::ARROW_ELBOW_LEFT_UP, self.muted),
            C::Dir => (i::FOLDER_SIMPLE, self.dir),
            C::Symlink => (i::LINK, self.dir),
            C::Archive => (i::FILE_ZIP, self.archive),
            C::Document => (i::FILE_TEXT, self.document),
            C::Image => (i::FILE_IMAGE, self.image),
            C::Audio => (i::MUSIC_NOTE, self.media),
            C::Video => (i::FILM_STRIP, self.media),
            C::Code => (i::FILE_CODE, self.code),
            C::Text => (i::FILE_TEXT, self.text),
            C::Executable => (i::GEAR_SIX, self.executable),
            C::Other => (i::FILE, self.muted),
        }
    }

    /// Hands the palette to egui: widgets, windows, selection and spacing.
    pub fn apply(&self, ctx: &egui::Context, text_percent: u16) {
        let mut style = (*ctx.global_style()).clone();
        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let sharp = CornerRadius::same(3);

        visuals.panel_fill = self.bg;
        visuals.window_fill = self.panel;
        visuals.window_stroke = Stroke::new(1.0, self.border);
        visuals.window_corner_radius = CornerRadius::same(5);
        visuals.extreme_bg_color = self.bg;
        visuals.faint_bg_color = self.row_alt;
        visuals.hyperlink_color = self.accent;
        visuals.warn_fg_color = self.warn;
        visuals.error_fg_color = self.error;
        visuals.selection.bg_fill = self.cursor_fill;
        // Two points wide: this is also the ring around the widget the keyboard focus is on.
        visuals.selection.stroke = Stroke::new(2.0, self.cursor_stroke);

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = self.panel;
        widgets.noninteractive.weak_bg_fill = self.panel;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        widgets.inactive.bg_fill = self.header;
        widgets.inactive.weak_bg_fill = self.header;
        widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        widgets.hovered.bg_fill = self.hover;
        widgets.hovered.weak_bg_fill = self.hover;
        widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent);
        widgets.hovered.fg_stroke = Stroke::new(1.5, self.text);
        widgets.active.bg_fill = self.cursor_fill;
        widgets.active.weak_bg_fill = self.cursor_fill;
        widgets.active.bg_stroke = Stroke::new(1.0, self.accent);
        widgets.active.fg_stroke = Stroke::new(2.0, self.text);
        widgets.open.bg_fill = self.header;
        widgets.open.weak_bg_fill = self.header;
        for state in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            state.corner_radius = sharp;
        }

        // Denser than egui's default, like a file manager.
        style.spacing.item_spacing = egui::vec2(6.0, 3.0);
        style.spacing.button_padding = egui::vec2(6.0, 3.0);
        style.visuals = visuals;
        // Text sizes are scaled from egui's own defaults, so repeated calls do not compound.
        let scale = f32::from(text_percent) / 100.0;
        for (style_name, font) in egui::Style::default().text_styles {
            if let Some(current) = style.text_styles.get_mut(&style_name) {
                current.size = font.size * scale;
            }
        }
        ctx.set_global_style(style);
    }
}

/// Contrast ratio of two colours (1 to 21), WCAG's definition; used to keep palettes readable.
#[cfg(test)]
fn contrast(a: Color32, b: Color32) -> f32 {
    fn channel(c: u8) -> f32 {
        let c = f32::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: Color32) -> f32 {
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accent_replaces_the_theme_colour_and_the_text_scale_does_not_compound() {
        let t = Theme::dark().with_accent(Some([255, 136, 0]));
        assert_eq!(t.accent, Color32::from_rgb(255, 136, 0));
        assert_eq!(t.cursor_stroke, t.accent);
        assert_eq!(Theme::dark().with_accent(None), Theme::dark());

        let ctx = egui::Context::default();
        let body =
            |ctx: &egui::Context| ctx.global_style().text_styles[&egui::TextStyle::Body].size;
        let normal = egui::Style::default().text_styles[&egui::TextStyle::Body].size;
        t.apply(&ctx, 150);
        t.apply(&ctx, 150);
        assert!((body(&ctx) - normal * 1.5).abs() < 0.01);
        t.apply(&ctx, 100);
        assert!((body(&ctx) - normal).abs() < 0.01);
    }

    #[test]
    fn every_theme_the_engine_offers_has_a_palette_with_the_same_name() {
        for name in bluepane_engine::THEMES {
            let resolved = Theme::resolve(name, Some(true));
            assert_eq!(Theme::by_name(resolved).name, resolved);
        }
        assert_eq!(Theme::resolve("system", Some(false)), "light");
        assert_eq!(Theme::resolve("system", Some(true)), "dark");
        assert_eq!(Theme::resolve("system", None), "dark");
        assert_eq!(Theme::resolve("light", Some(true)), "light");
        assert!(bluepane_engine::THEMES.contains(&bluepane_engine::DEFAULT_THEME));
        assert_eq!(
            Theme::by_name("nonsense").name,
            bluepane_engine::DEFAULT_THEME
        );
    }

    #[test]
    fn text_in_every_theme_is_readable_on_its_backgrounds() {
        for name in ["dark", "light"] {
            let t = Theme::by_name(name);
            for background in [t.panel, t.panel_inactive, t.row_alt] {
                for (label, colour, minimum) in [
                    ("text", t.text, 7.0),
                    ("dir", t.dir, 4.5),
                    ("selected", t.selected, 4.5),
                    ("archive", t.archive, 4.5),
                    ("document", t.document, 4.5),
                    ("image", t.image, 4.5),
                    ("media", t.media, 4.5),
                    ("code", t.code, 4.5),
                    ("executable", t.executable, 4.5),
                    ("muted", t.muted, 3.0),
                    ("warn", t.warn, 4.5),
                    ("error", t.error, 4.5),
                    ("ok", t.ok, 4.5),
                    ("accent", t.accent, 3.0),
                ] {
                    let ratio = contrast(colour, background);
                    assert!(
                        ratio >= minimum,
                        "{name}: {label} on {background:?} has contrast {ratio:.1}, needs {minimum}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_class_has_an_icon_and_the_kinds_are_told_apart_by_colour() {
        use bluepane_engine::FileClass as C;
        let theme = Theme::dark();
        let all = [
            C::Parent,
            C::Dir,
            C::Symlink,
            C::Archive,
            C::Document,
            C::Image,
            C::Audio,
            C::Video,
            C::Code,
            C::Text,
            C::Executable,
            C::Other,
        ];
        for class in all {
            let (icon, _) = theme.look(class);
            assert_eq!(icon.chars().count(), 1, "{class:?} has one glyph");
        }
        let colour = |class| theme.look(class).1;
        // The classes a user needs to tell apart at a glance.
        let distinct = [
            C::Dir,
            C::Archive,
            C::Document,
            C::Image,
            C::Code,
            C::Executable,
        ];
        for (i, a) in distinct.iter().enumerate() {
            for b in &distinct[i + 1..] {
                assert_ne!(colour(*a), colour(*b), "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn the_active_and_inactive_panels_differ() {
        let t = Theme::dark();
        assert_ne!(t.panel, t.panel_inactive);
        assert_ne!(t.cursor_stroke, t.cursor_stroke_inactive);
    }

    #[test]
    fn contrast_of_black_and_white_is_21() {
        let ratio = contrast(Color32::BLACK, Color32::WHITE);
        assert!((ratio - 21.0).abs() < 0.01, "{ratio}");
    }
}
