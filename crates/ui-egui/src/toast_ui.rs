//! Notices in the corner of the window: a job that has ended says how it went, so a
//! long copy does not have to be watched. Successes fade after a few seconds; a failure
//! stays until it is dismissed. A click on a notice opens the queue window.

use bluepane_engine::Notice;
use egui::{Align2, RichText};
use egui_phosphor::regular as icon;

use crate::BluepaneUi;

/// Notices shown at once; older ones make room.
const TOASTS_MAX: usize = 4;
/// Seconds a successful job's notice stays.
const LIFETIME: f64 = 6.0;

pub(crate) struct Toast {
    notice: Notice,
    born: f64,
}

impl BluepaneUi {
    /// How many notices are on screen (for tests).
    pub fn toast_count(&self) -> usize {
        self.toasts.len()
    }

    pub(crate) fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        for notice in self.session.take_notices() {
            self.toasts.push(Toast { notice, born: now });
        }
        if self.toasts.len() > TOASTS_MAX {
            let extra = self.toasts.len() - TOASTS_MAX;
            self.toasts.drain(..extra);
        }
        self.toasts
            .retain(|t| t.notice.is_error || now - t.born < LIFETIME);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        let (error_color, ok_color, muted) =
            (self.theme.error, self.theme.accent, self.theme.muted);
        let mut dismissed: Option<usize> = None;
        let mut open_queue = false;
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, [-14.0, -64.0])
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                for (index, toast) in self.toasts.iter().enumerate() {
                    let notice = &toast.notice;
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (symbol, color) = if notice.is_error {
                                (icon::WARNING, error_color)
                            } else if notice.cancelled {
                                (icon::PROHIBIT, muted)
                            } else {
                                (icon::CHECK_CIRCLE, ok_color)
                            };
                            ui.label(RichText::new(symbol).color(color));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&notice.title).strong());
                                let body = ui.add(
                                    egui::Label::new(RichText::new(&notice.text).color(muted))
                                        .sense(egui::Sense::click()),
                                );
                                if body.clicked() {
                                    open_queue = true;
                                    dismissed = Some(index);
                                }
                            });
                            if ui
                                .add(egui::Button::new(icon::X).frame(false).small())
                                .on_hover_text("Zamknij")
                                .clicked()
                            {
                                dismissed = Some(index);
                            }
                        });
                    });
                }
            });
        if let Some(index) = dismissed {
            self.toasts.remove(index);
        }
        if open_queue {
            let _ = self.session.run_line("queue");
        }
    }
}
