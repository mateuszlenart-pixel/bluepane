//! The bottom of the window: the running job as a slim bar (progress, speed, cancel, a way to
//! the queue window) and the function-key buttons, two-tone: a key badge and the action.

use bluepane_engine::human_size;
use egui::{Align2, FontId, Rect, RichText, Sense, Stroke};

use crate::theme::Theme;
use crate::BluepaneUi;

/// Smoothed speed of a transfer, from the byte counter sampled once per frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpeedMeter {
    last: Option<(f64, u64)>,
    speed: f64,
}

impl SpeedMeter {
    /// Feeds the counter at time `now` (seconds); returns bytes per second, smoothed so the
    /// number does not jump with every frame. A counter that went back means a new job.
    pub(crate) fn update(&mut self, now: f64, bytes: u64) -> f64 {
        match self.last {
            Some((_, before)) if bytes < before => {
                self.speed = 0.0;
                self.last = Some((now, bytes));
            }
            Some((then, before)) if now - then >= 0.25 => {
                let instant = (bytes - before) as f64 / (now - then);
                self.speed = if self.speed == 0.0 {
                    instant
                } else {
                    self.speed * 0.7 + instant * 0.3
                };
                self.last = Some((now, bytes));
            }
            Some(_) => {}
            None => self.last = Some((now, bytes)),
        }
        self.speed
    }

    pub(crate) fn reset(&mut self) {
        *self = SpeedMeter::default();
    }
}

/// "42.5 MB/s".
pub(crate) fn speed_text(bytes_per_second: f64) -> String {
    format!("{}/s", human_size(bytes_per_second.max(0.0) as u64))
}

/// "12 s", "3 min 05 s", "1 h 20 min".
pub(crate) fn eta_text(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min {:02} s", seconds / 60, seconds % 60),
        _ => format!("{} h {:02} min", seconds / 3600, seconds % 3600 / 60),
    }
}

/// The two rows of function-key buttons: file operations, then tools.
const FILE_KEYS: &[(&str, &str, &str)] = &[
    ("F2", "Nazwa", "rename"),
    ("F3", "Podgląd", "view"),
    ("F4", "Edycja", "edit"),
    ("F5", "Kopiuj", "copy"),
    ("F6", "Przenieś", "move"),
    ("F7", "Katalog", "mkdir"),
    ("F8", "Kosz", "delete"),
    ("F9", "Odśwież", "refresh"),
];
const TOOL_KEYS: &[(&str, &str, &str)] = &[
    ("Ctrl+F2", "Zmiana nazw", "multirename"),
    ("Alt+S", "Synchronizuj", "sync"),
    ("Ctrl+F", "Szukaj", "find"),
    ("Alt+F5", "Pakuj", "pack"),
    ("Alt+F9", "Rozpakuj", "unpack"),
    ("Ctrl+S", "Serwery", "sites"),
    ("Ctrl+N", "Skaner", "scan"),
    ("Ctrl+J", "Kolejka", "queue"),
];

impl BluepaneUi {
    /// The running job: cancel, name, bar, bytes, speed, time left, file, queue window.
    pub(crate) fn job_bar(&mut self, ui: &mut egui::Ui) {
        let Some(job) = self.session.job_status() else {
            self.speed.reset();
            return;
        };
        let now = ui.input(|i| i.time);
        let speed = self.speed.update(now, job.bytes_done);
        let theme = self.theme.clone();
        let mut go_to_queue = false;
        let mut cancel = false;
        egui::Frame::new()
            .fill(theme.header)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // The cancel button stays first so a long file name cannot push it away.
                    if ui.button("Anuluj").clicked() {
                        cancel = true;
                    }
                    let queued = if job.queued > 0 {
                        format!(" (+{})", job.queued)
                    } else {
                        String::new()
                    };
                    ui.label(
                        RichText::new(format!("{}{queued}", job.label))
                            .color(theme.accent)
                            .strong(),
                    );
                    let bar = match job.fraction() {
                        Some(fraction) => egui::ProgressBar::new(fraction)
                            .desired_width(220.0)
                            .show_percentage(),
                        None => egui::ProgressBar::new(0.0)
                            .desired_width(220.0)
                            .animate(true)
                            .text("liczenie…"),
                    };
                    ui.add(bar);
                    if job.bytes_total > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                human_size(job.bytes_done),
                                human_size(job.bytes_total)
                            ))
                            .color(theme.muted),
                        );
                    }
                    if speed > 0.0 && job.bytes_total > 0 {
                        let left = job.bytes_total.saturating_sub(job.bytes_done) as f64 / speed;
                        ui.label(
                            RichText::new(format!(
                                "{}  ~{}",
                                speed_text(speed),
                                eta_text(left as u64)
                            ))
                            .color(theme.text),
                        );
                    } else if speed > 0.0 {
                        ui.label(RichText::new(speed_text(speed)).color(theme.text));
                    }
                    if job.waiting_for_answer {
                        ui.label(RichText::new("czeka na decyzję").color(theme.warn));
                    }
                    if ui
                        .button("Kolejka")
                        .on_hover_text("okno kolejki zadań (Ctrl+J)")
                        .clicked()
                    {
                        go_to_queue = true;
                    }
                    ui.add(
                        egui::Label::new(RichText::new(&job.current).color(theme.muted)).truncate(),
                    );
                });
            });
        if cancel {
            self.dispatch("stop");
        }
        if go_to_queue {
            self.dispatch("queue");
        }
    }

    /// Two rows of buttons, each spread over the whole width.
    pub(crate) fn fkey_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        let mut line: Option<&'static str> = None;
        for row in [FILE_KEYS, TOOL_KEYS] {
            let gap = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - gap * (row.len() as f32 - 1.0))
                / row.len() as f32)
                .max(40.0);
            ui.horizontal(|ui| {
                for (key, label, command) in row {
                    if key_button(ui, &theme, key, label, width).clicked() {
                        line = Some(command);
                    }
                }
            });
        }
        if let Some(line) = line {
            self.dispatch(line);
        }
    }
}

/// A button of two tones: the key on a darker badge, the action next to it.
fn key_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    key: &str,
    label: &str,
    width: f32,
) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let painter = ui.painter();
    let hovered = response.hovered();
    painter.rect(
        rect,
        3.0,
        if hovered { theme.hover } else { theme.header },
        Stroke::new(1.0, if hovered { theme.accent } else { theme.border }),
        egui::StrokeKind::Inside,
    );
    let font = FontId::proportional(12.5);
    let key_galley = painter.layout_no_wrap(key.to_string(), font.clone(), theme.accent);
    let badge_width = (key_galley.size().x + 10.0).min(rect.width() * 0.6);
    let badge = Rect::from_min_size(rect.min, egui::vec2(badge_width, rect.height()));
    painter.rect_filled(badge, 3.0, theme.bg);
    painter.with_clip_rect(badge).text(
        badge.center(),
        Align2::CENTER_CENTER,
        key,
        font.clone(),
        theme.accent,
    );
    let text_area = Rect::from_min_max(egui::pos2(badge.right() + 4.0, rect.top()), rect.max);
    painter.with_clip_rect(text_area).text(
        egui::pos2(text_area.left(), text_area.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        theme.text,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_measured_between_samples_and_smoothed() {
        let mut meter = SpeedMeter::default();
        assert_eq!(meter.update(0.0, 0), 0.0, "nothing to compare with yet");
        // Too soon after the last sample: unchanged.
        assert_eq!(meter.update(0.1, 1_000_000), 0.0);
        let first = meter.update(1.0, 10_000_000);
        assert!((first - 10_000_000.0).abs() < 1.0, "{first}");
        // A slower second: the number moves toward it, but not all the way.
        let second = meter.update(2.0, 12_000_000);
        assert!(second < first && second > 2_000_000.0, "{second}");
    }

    #[test]
    fn a_counter_that_goes_back_starts_over() {
        let mut meter = SpeedMeter::default();
        meter.update(0.0, 0);
        meter.update(1.0, 5_000_000);
        assert!(meter.update(2.0, 10_000_000) > 0.0);
        assert_eq!(meter.update(3.0, 100), 0.0, "a new job began");
    }

    #[test]
    fn text_helpers_read_naturally() {
        assert_eq!(speed_text(44_564_480.0), "42.5 MB/s");
        assert_eq!(eta_text(12), "12 s");
        assert_eq!(eta_text(185), "3 min 05 s");
        assert_eq!(eta_text(4800), "1 h 20 min");
    }

    #[test]
    fn every_function_key_button_is_a_real_command() {
        for (key, _, line) in FILE_KEYS.iter().chain(TOOL_KEYS) {
            assert!(
                bluepane_engine::Command::parse(line).is_ok(),
                "{key} -> {line}"
            );
        }
    }
}
