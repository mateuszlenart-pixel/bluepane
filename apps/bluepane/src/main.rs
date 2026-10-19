//! Desktop entry point: `bluepane [left_dir [right_dir]]`.
//!
//! The only place with a `cfg(windows)`: the release build there has no console window (a
//! GUI program should not open one). The test build (`--features console`) keeps it, so that
//! messages are visible; panics also go to `bluepane.log` in the config directory.
#![cfg_attr(
    all(windows, not(debug_assertions), not(feature = "console")),
    windows_subsystem = "windows"
)]

mod log;

use std::path::PathBuf;

use bluepane_engine::Session;
use bluepane_ui_egui::BluepaneUi;

struct App {
    ui: BluepaneUi,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.ui.show(ui);
    }

    fn on_exit(&mut self) {
        self.ui.session().save_last_session();
    }
}

fn start_dir(arg: Option<String>) -> PathBuf {
    arg.map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The window icon: 64x64 RGBA, drawn by `assets/icon-64.rgba`.
fn icon() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: include_bytes!("../assets/icon-64.rgba").to_vec(),
        width: 64,
        height: 64,
    }
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let given = first.is_some();
    let left = start_dir(first);
    let right = start_dir(args.next());
    let mut session = Session::new(left, right);
    // The settings (and the place for the log) are needed before the window opens.
    session.load_config();
    // Folders on the command line mean "start here"; otherwise the last session comes back.
    if !given {
        session.restore_last_session();
    }
    if let Some(dir) = session.config_dir() {
        log::install(dir.join("bluepane.log"));
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("bluepane")
            .with_icon(std::sync::Arc::new(icon()))
            .with_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "bluepane",
        options,
        Box::new(|_cc| {
            Ok(Box::new(App {
                ui: BluepaneUi::new(session),
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_is_a_full_64_by_64_picture_with_transparent_corners() {
        let icon = super::icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert_eq!(icon.rgba[3], 0, "the corner is transparent");
        let middle = ((32 * 64 + 32) * 4 + 3) as usize;
        assert_eq!(icon.rgba[middle], 255, "the middle is opaque");
    }
}
