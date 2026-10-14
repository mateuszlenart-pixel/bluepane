//! The queue window (Ctrl+J): the running job, the waiting ones and the finished ones.
//! It only shows the engine's state and sends commands (`queuepause`, `queuedel`, ...).

use bluepane_engine::{human_size, Command};
use egui::{Color32, RichText};

use crate::BluepaneUi;

impl BluepaneUi {
    pub(crate) fn queue_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            if key == "Escape" {
                self.dispatch("queueclose");
            }
        }
    }

    pub(crate) fn queue_ui(&mut self, ui: &mut egui::Ui) {
        let lines = self.session.queue_lines();
        let status = self.session.job_status();
        let paused = self.session.paused;
        let mut commands: Vec<Command> = Vec::new();

        ui.label(RichText::new("Kolejka zadań").strong());
        ui.horizontal(|ui| {
            if paused {
                if ui.button("Wznów kolejkę").clicked() {
                    commands.push(Command::QueueResume);
                }
            } else if ui.button("Wstrzymaj kolejkę").clicked() {
                commands.push(Command::QueuePause);
            }
            if ui
                .add_enabled(!lines.is_empty(), egui::Button::new("Zatrzymaj wszystko"))
                .on_hover_text("przerywa bieżące zadanie i usuwa oczekujące")
                .clicked()
            {
                commands.push(Command::StopJob);
            }
            if ui
                .add_enabled(
                    !self.session.finished.is_empty(),
                    egui::Button::new("Wyczyść historię"),
                )
                .clicked()
            {
                commands.push(Command::QueueClear);
            }
            if ui.button("Zamknij (Esc)").clicked() {
                commands.push(Command::QueueClose);
            }
            if paused {
                ui.colored_label(
                    Color32::YELLOW,
                    "Kolejka wstrzymana: bieżące zadanie się dokończy, następne czekają.",
                );
            }
        });
        ui.separator();

        ui.label(RichText::new("Trwające i oczekujące").strong());
        if lines.is_empty() {
            ui.label(RichText::new("Nic nie czeka.").weak());
        }
        for (index, line) in lines.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", index + 1));
                if line.running {
                    ui.label(RichText::new(&line.title).strong());
                    if let Some(status) = &status {
                        if let Some(fraction) = status.fraction() {
                            ui.add(
                                egui::ProgressBar::new(fraction)
                                    .desired_width(160.0)
                                    .show_percentage(),
                            );
                        } else {
                            ui.spinner();
                        }
                        if status.bytes_total > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "{} z {}",
                                    human_size(status.bytes_done),
                                    human_size(status.bytes_total)
                                ))
                                .weak(),
                            );
                        }
                        ui.label(RichText::new(&status.current).weak());
                    }
                } else {
                    ui.label(&line.title);
                    if ui.small_button("Usuń z kolejki").clicked() {
                        commands.push(Command::QueueDelete(index + 1));
                    }
                }
            });
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Zakończone (najnowsze u góry)").strong());
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if self.session.finished.is_empty() {
                    ui.label(RichText::new("Jeszcze nic się nie skończyło.").weak());
                }
                for (index, job) in self.session.finished.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", index + 1));
                        ui.label(&job.title);
                        let color = if job.is_error {
                            Color32::LIGHT_RED
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        ui.colored_label(color, &job.result);
                        if ui
                            .small_button("Ponów")
                            .on_hover_text("ustaw to zadanie w kolejce jeszcze raz")
                            .clicked()
                        {
                            commands.push(Command::QueueRetry(index + 1));
                        }
                    });
                }
            });

        for command in commands {
            let _ = self.session.execute(command);
        }
    }
}
