//! Desktop entry point: `bluepane [left_dir [right_dir]]`.

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
}

fn start_dir(arg: Option<String>) -> PathBuf {
    arg.map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let left = start_dir(args.next());
    let right = start_dir(args.next());
    let session = Session::new(left, right);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("bluepane")
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
