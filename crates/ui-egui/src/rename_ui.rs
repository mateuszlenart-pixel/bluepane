//! The multi-rename window (Ctrl+F2), drawn in place of the two panels.
//!
//! It only edits the engine's settings (`mrset`) and shows its plan: the list of old and
//! new names is the preview, and nothing is renamed before "Start".

use bluepane_engine::{format_modified, human_size, Command, RenameCase, RenameStatus, RenameView};
use egui::{Align2, Color32, FontId, RichText, Sense};

use crate::BluepaneUi;

const ROW_HEIGHT: f32 = 20.0;

const CASES: &[(RenameCase, &str)] = &[
    (RenameCase::Unchanged, "Niezmienione"),
    (RenameCase::Upper, "WIELKIE LITERY"),
    (RenameCase::Lower, "małe litery"),
    (RenameCase::Words, "Każdy Wyraz Wielką"),
    (RenameCase::Sentence, "Pierwsza wielka"),
];

/// Masks the buttons insert: (button label, text, hover help).
const NAME_TOKENS: &[(&str, &str, &str)] = &[
    ("[N] Nazwa", "[N]", "cała nazwa pliku (bez rozszerzenia)"),
    (
        "[N#-#] Zakres",
        "[N1-3]",
        "znaki od-do, np. [N2-5], [N3-] (do końca), [N2,3] (od, ile)",
    ),
    (
        "[C] Licznik",
        "[C]",
        "numer kolejny; [C10+5:3] = start 10, krok 5, 3 cyfry",
    ),
    (
        "[YMD] Data",
        "[YMD]",
        "data modyfikacji; pojedynczo: [Y] [M] [D]",
    ),
    (
        "[hms] Czas",
        "[hms]",
        "czas modyfikacji; pojedynczo: [h] [m] [s]",
    ),
    ("[P] Katalog", "[P]", "nazwa katalogu nadrzędnego"),
];

const EXT_TOKENS: &[(&str, &str, &str)] = &[
    ("[E] Rozszerzenie", "[E]", "rozszerzenie pliku (bez kropki)"),
    ("[E#-#] Zakres", "[E1-2]", "znaki rozszerzenia od-do"),
    ("[C] Licznik", "[C]", "numer kolejny"),
];

impl BluepaneUi {
    /// F5 in the window: run when nothing is being typed.
    pub(crate) fn rename_keys(&mut self, pressed: &[String]) {
        for key in pressed {
            match key.as_str() {
                "Escape" => self.dispatch("mrclose"),
                "F5" | "Ctrl+Enter" => self.dispatch("mrrun"),
                "Ctrl+Z" => self.dispatch("mrundo"),
                _ => {}
            }
        }
    }

    pub(crate) fn rename_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.session.rename.clone() else {
            self.rename_edit = None;
            return;
        };
        let mut commands: Vec<Command> = Vec::new();
        let set = |field: &str, value: String| Command::MultiRenameSet(field.into(), value);
        let s = &view.settings;

        ui.label(
            RichText::new(format!(
                "Narzędzie wielokrotnej zmiany nazw: {} ({} plików)",
                view.dir.display(),
                view.entries.len()
            ))
            .strong(),
        );
        ui.add_space(4.0);

        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.set_width(300.0);
                ui.vertical(|ui| {
                    ui.label("Maska nazwy pliku:");
                    let mut text = s.name_mask.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(280.0))
                        .changed()
                    {
                        commands.push(set("name", text.clone()));
                    }
                    token_buttons(ui, NAME_TOKENS, &s.name_mask, "name", &mut commands);
                });
            });
            ui.group(|ui| {
                ui.set_width(170.0);
                ui.vertical(|ui| {
                    ui.label("Rozszerzenie:");
                    let mut text = s.ext_mask.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(150.0))
                        .changed()
                    {
                        commands.push(set("ext", text.clone()));
                    }
                    token_buttons(ui, EXT_TOKENS, &s.ext_mask, "ext", &mut commands);
                });
            });
            ui.group(|ui| {
                ui.set_width(330.0);
                ui.vertical(|ui| {
                    ui.label("Szukaj i zamień:");
                    egui::Grid::new("rename_find")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Szukaj:");
                            let mut find = s.find.clone();
                            if ui
                                .add(egui::TextEdit::singleline(&mut find).desired_width(240.0))
                                .changed()
                            {
                                commands.push(set("find", find));
                            }
                            ui.end_row();
                            ui.label("Zamień na:");
                            let mut replace = s.replace.clone();
                            if ui
                                .add(egui::TextEdit::singleline(&mut replace).desired_width(240.0))
                                .changed()
                            {
                                commands.push(set("replace", replace));
                            }
                            ui.end_row();
                        });
                    ui.horizontal_wrapped(|ui| {
                        let mut flag =
                            |ui: &mut egui::Ui, label: &str, on: bool, field: &str, help: &str| {
                                let mut value = on;
                                if ui.checkbox(&mut value, label).on_hover_text(help).changed() {
                                    commands
                                        .push(set(field, if value { "on" } else { "off" }.into()));
                                }
                            };
                        flag(ui, "1x", s.once, "once", "tylko pierwsze trafienie");
                        flag(
                            ui,
                            "[E]",
                            s.include_ext,
                            "inclext",
                            "szukaj też w rozszerzeniu",
                        );
                        flag(
                            ui,
                            "RegWy",
                            s.regex,
                            "regex",
                            "wyrażenie regularne ($1 = grupa)",
                        );
                        flag(
                            ui,
                            "bez wielkości liter",
                            s.ignore_case,
                            "ignorecase",
                            "ignoruj wielkość liter przy szukaniu",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Duża/mała:");
                        let current = CASES
                            .iter()
                            .find(|(c, _)| *c == s.case)
                            .map_or("", |(_, label)| *label);
                        egui::ComboBox::from_id_salt("rename_case")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (case, label) in CASES {
                                    if ui.selectable_label(s.case == *case, *label).clicked() {
                                        commands.push(set("case", case_word(*case).into()));
                                    }
                                }
                            });
                    });
                });
            });
            ui.group(|ui| {
                ui.set_width(150.0);
                ui.vertical(|ui| {
                    ui.label("Licznik [C]:");
                    egui::Grid::new("rename_counter")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Start od:");
                            let mut start = s.counter_start;
                            if ui.add(egui::DragValue::new(&mut start)).changed() {
                                commands.push(set("start", start.to_string()));
                            }
                            ui.end_row();
                            ui.label("Krok co:");
                            let mut step = s.counter_step;
                            if ui.add(egui::DragValue::new(&mut step)).changed() {
                                commands.push(set("step", step.to_string()));
                            }
                            ui.end_row();
                            ui.label("Cyfr:");
                            let mut digits = s.counter_digits;
                            if ui
                                .add(egui::DragValue::new(&mut digits).range(1..=12))
                                .changed()
                            {
                                commands.push(set("digits", digits.to_string()));
                            }
                            ui.end_row();
                        });
                });
            });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let blocked = view.error.is_some() || view.plan.conflicts() > 0;
            let can_run = !blocked && view.plan.changes() > 0;
            if ui
                .add_enabled(can_run, egui::Button::new("Start! (F5)"))
                .clicked()
            {
                commands.push(Command::MultiRenameRun);
            }
            if ui
                .add_enabled(!view.undo.is_empty(), egui::Button::new("Cofnij (Ctrl+Z)"))
                .clicked()
            {
                commands.push(Command::MultiRenameUndo);
            }
            if ui.button("Ustawienia domyślne").clicked() {
                commands.push(set("reset", String::new()));
            }
            if ui.button("Zamknij (Esc)").clicked() {
                commands.push(Command::MultiRenameClose);
            }
            ui.separator();
            ui.label(format!(
                "Do zmiany: {} · konflikty: {} · bez zmian: {}",
                view.plan.changes(),
                view.plan.conflicts(),
                view.plan.rows.len() - view.plan.changes() - view.plan.conflicts()
            ));
        });
        if let Some(error) = &view.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        } else if let Some(summary) = &view.summary {
            ui.label(RichText::new(summary).weak());
        } else {
            ui.label(
                RichText::new("Kliknij w nową nazwę, żeby wpisać ją ręcznie. Nic nie zmieni się przed „Start”.")
                    .weak(),
            );
        }
        ui.separator();
        self.rename_rows(ui, &view, &mut commands);

        for command in commands {
            let _ = self.session.execute(command);
        }
    }

    fn rename_rows(&mut self, ui: &mut egui::Ui, view: &RenameView, commands: &mut Vec<Command>) {
        let font = FontId::monospace(13.0);
        let rows = &view.plan.rows;
        let mut finish: Option<(usize, String, bool)> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            ROW_HEIGHT,
            rows.len().min(view.entries.len()),
            |ui, range| {
                for index in range {
                    let row = &rows[index];
                    let entry = &view.entries[index];
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        Sense::hover(),
                    );
                    let w = rect.width();
                    let at = |fraction: f32| rect.left() + w * fraction;
                    let weak = ui.visuals().weak_text_color();
                    let normal = ui.visuals().text_color();
                    let painter = ui.painter();
                    painter.text(
                        egui::pos2(at(0.0), rect.center().y),
                        Align2::LEFT_CENTER,
                        format!("{}", index + 1),
                        font.clone(),
                        weak,
                    );
                    painter.text(
                        egui::pos2(at(0.04), rect.center().y),
                        Align2::LEFT_CENTER,
                        &row.old_name,
                        font.clone(),
                        normal,
                    );
                    let (color, hint) = match &row.status {
                        RenameStatus::Unchanged => (weak, None),
                        RenameStatus::Ready => (Color32::LIGHT_GREEN, None),
                        RenameStatus::Conflict(why) => (Color32::LIGHT_RED, Some(why.as_str())),
                    };
                    let name_rect = egui::Rect::from_min_max(
                        egui::pos2(at(0.36), rect.top()),
                        egui::pos2(at(0.70), rect.bottom()),
                    );
                    let editing = self.rename_edit.as_ref().is_some_and(|(i, _)| *i == index);
                    if editing {
                        let buffer = &mut self.rename_edit.as_mut().expect("checked").1;
                        let response = ui.put(
                            name_rect,
                            egui::TextEdit::singleline(buffer).font(font.clone()),
                        );
                        response.request_focus();
                        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if escape || enter || response.lost_focus() {
                            finish = Some((index, buffer.clone(), !escape));
                        }
                    } else {
                        let response = ui.interact(
                            name_rect,
                            egui::Id::new(("rename_new_name", index)),
                            Sense::click(),
                        );
                        ui.painter().text(
                            egui::pos2(name_rect.left(), rect.center().y),
                            Align2::LEFT_CENTER,
                            &row.new_name,
                            font.clone(),
                            color,
                        );
                        let response = match hint {
                            Some(text) => response.on_hover_text(text),
                            None => response.on_hover_text("Kliknij, żeby wpisać nazwę ręcznie"),
                        };
                        if response.clicked() {
                            self.rename_edit = Some((index, row.new_name.clone()));
                        }
                    }
                    let painter = ui.painter();
                    painter.text(
                        egui::pos2(at(0.72), rect.center().y),
                        Align2::LEFT_CENTER,
                        if entry.item.is_dir {
                            "<DIR>".to_string()
                        } else {
                            human_size(entry.size)
                        },
                        font.clone(),
                        weak,
                    );
                    painter.text(
                        egui::pos2(at(0.82), rect.center().y),
                        Align2::LEFT_CENTER,
                        format_modified(entry.modified),
                        font.clone(),
                        weak,
                    );
                }
            },
        );
        if let Some((index, text, accept)) = finish {
            self.rename_edit = None;
            if accept {
                let unchanged = rows.get(index).is_some_and(|r| r.new_name == text);
                if !unchanged {
                    commands.push(Command::MultiRenameName(
                        index + 1,
                        Some(text).filter(|t| !t.is_empty()),
                    ));
                }
            }
        }
    }
}

fn token_buttons(
    ui: &mut egui::Ui,
    tokens: &[(&str, &str, &str)],
    current: &str,
    field: &str,
    commands: &mut Vec<Command>,
) {
    ui.horizontal_wrapped(|ui| {
        for (label, token, help) in tokens {
            if ui.small_button(*label).on_hover_text(*help).clicked() {
                commands.push(Command::MultiRenameSet(
                    field.into(),
                    format!("{current}{token}"),
                ));
            }
        }
    });
}

fn case_word(case: RenameCase) -> &'static str {
    match case {
        RenameCase::Unchanged => "unchanged",
        RenameCase::Upper => "upper",
        RenameCase::Lower => "lower",
        RenameCase::Words => "words",
        RenameCase::Sentence => "sentence",
    }
}
