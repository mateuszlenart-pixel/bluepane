//! Colour themes. A theme is a palette of named colours; `apply` turns it into egui's
//! `Visuals` and spacing, and the drawing code asks the palette for panel, cursor and text
//! colours instead of using fixed ones. The engine only knows the theme's name
//! (`bluepane_engine::THEMES`).

use egui::{Color32, CornerRadius, Stroke};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
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
    pub accent: Color32,
    pub border: Color32,
    pub warn: Color32,
    pub error: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Theme {
    /// Modern Dark: graphite panels, blue accent, amber marks.
    pub fn dark() -> Theme {
        Theme {
            name: "dark",
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
            accent: rgb(0x3d, 0x8b, 0xfd),
            border: rgb(0x2c, 0x31, 0x44),
            warn: rgb(0xfb, 0xbf, 0x24),
            error: rgb(0xf8, 0x71, 0x71),
        }
    }

    /// The palette named `name`; an unknown name gives the default theme.
    pub fn by_name(name: &str) -> Theme {
        match name {
            "dark" => Theme::dark(),
            _ => Theme::dark(),
        }
    }

    /// Hands the palette to egui: widgets, windows, selection and spacing.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.global_style()).clone();
        let mut visuals = egui::Visuals::dark();
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
        visuals.selection.stroke = Stroke::new(1.0, self.cursor_stroke);

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
    fn every_theme_the_engine_offers_has_a_palette_with_the_same_name() {
        for name in bluepane_engine::THEMES {
            assert_eq!(Theme::by_name(name).name, *name);
        }
        assert!(bluepane_engine::THEMES.contains(&bluepane_engine::DEFAULT_THEME));
        assert_eq!(
            Theme::by_name("nonsense").name,
            bluepane_engine::DEFAULT_THEME
        );
    }

    #[test]
    fn text_in_every_theme_is_readable_on_its_backgrounds() {
        for name in bluepane_engine::THEMES {
            let t = Theme::by_name(name);
            for background in [t.panel, t.panel_inactive, t.row_alt] {
                for (label, colour, minimum) in [
                    ("text", t.text, 7.0),
                    ("dir", t.dir, 4.5),
                    ("selected", t.selected, 4.5),
                    ("muted", t.muted, 3.0),
                    ("warn", t.warn, 4.5),
                    ("error", t.error, 4.5),
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
