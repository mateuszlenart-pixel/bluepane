//! The search window (Alt+F7), drawn in place of the two panels.
//!
//! It edits the engine's search form with `findset`, starts the search with `findrun` and
//! shows the hits the worker has found so far (it redraws while the search runs).

use bluepane_engine::{
    format_modified, human_size, Command, FindEncoding, FindHit, FindLine, FindView,
};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, RichText, Sense};

use crate::BluepaneUi;

const ROW_HEIGHT: f32 = 20.0;

const ENCODINGS: &[(FindEncoding, &str, &str)] = &[
    (
        FindEncoding::Auto,
        "auto",
        "Automatycznie (UTF-8/16, CP1250)",
    ),
    (FindEncoding::Utf8, "utf8", "UTF-8"),
    (FindEncoding::Cp1250, "cp1250", "Windows-1250"),
    (FindEncoding::Utf16, "utf16", "UTF-16"),
];

impl BluepaneUi {
    pub(crate) fn find_keys(&mut self, pressed: &[String]) {
        let count = self
            .session
            .find
            .as_ref()
            .map_or(0, |v| v.data().hits.len());
        for key in pressed {
            match key.as_str() {
                "Escape" => {
                    let running = self.session.find.as_ref().is_some_and(FindView::is_running);
                    self.dispatch(if running { "findstop" } else { "findclose" });
                }
                "ArrowDown" => {
                    self.find_selected = (self.find_selected + 1).min(count.saturating_sub(1))
                }
                "ArrowUp" => self.find_selected = self.find_selected.saturating_sub(1),
                "PageDown" => {
                    self.find_selected = (self.find_selected + 15).min(count.saturating_sub(1))
                }
                "PageUp" => self.find_selected = self.find_selected.saturating_sub(15),
                "Enter" if count > 0 => {
                    let line = format!("findgo {}", self.find_selected + 1);
                    self.dispatch(&line);
                }
                "F3" if count > 0 => {
                    let line = format!("findview {}", self.find_selected + 1);
                    self.dispatch(&line);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn find_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.session.find.as_ref() else {
            return;
        };
        let form = view.form.clone();
        let running = view.is_running();
        let progress = view.progress_line();
        let (count, summary, error) = {
            let data = view.data();
            (data.hits.len(), data.summary.clone(), data.error.clone())
        };
        let mut commands: Vec<Command> = Vec::new();
        let mut run = false;
        let set = |field: &str, value: String| Command::FindSet(field.into(), value);

        ui.label(RichText::new("Wyszukiwanie plików").strong());
        ui.add_space(2.0);

        egui::Grid::new("find_main")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Nazwa (maska):");
                ui.horizontal(|ui| {
                    let mut text = form.name.clone();
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("np. *.rs;*.md (puste = wszystkie)")
                            .desired_width(360.0),
                    );
                    if response.changed() {
                        commands.push(set("name", text));
                    }
                    run |= submitted(ui, &response);
                    check(
                        ui,
                        "RegWy",
                        form.name_regex,
                        "namergx",
                        "nazwa jako wyrażenie regularne",
                        &mut commands,
                    );
                    check(
                        ui,
                        "wielkość liter",
                        form.name_case,
                        "namecase",
                        "rozróżniaj wielkość liter w nazwie",
                        &mut commands,
                    );
                });
                ui.end_row();

                ui.label("Szukaj w:");
                ui.horizontal(|ui| {
                    let mut text = form.roots.clone();
                    let response =
                        ui.add(egui::TextEdit::singleline(&mut text).desired_width(360.0));
                    if response.changed() {
                        commands.push(set("roots", text));
                    }
                    run |= submitted(ui, &response);
                    for (label, value, help) in [
                        ("Ten panel", "here", "katalog aktywnego panelu"),
                        ("Oba panele", "both", "katalogi obu paneli"),
                        ("Wszystkie dyski", "all", "każdy dysk / punkt montowania"),
                    ] {
                        if ui.small_button(label).on_hover_text(help).clicked() {
                            commands.push(set("roots", value.into()));
                        }
                    }
                });
                ui.end_row();

                ui.label("Pomiń katalogi:");
                let mut text = form.exclude.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text("np. target;node_modules;.git")
                        .desired_width(360.0),
                );
                if response.changed() {
                    commands.push(set("exclude", text));
                }
                run |= submitted(ui, &response);
                ui.end_row();

                ui.label("Zawiera tekst:");
                ui.horizontal(|ui| {
                    let mut text = form.text.clone();
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("szukany tekst w treści plików")
                            .desired_width(360.0),
                    );
                    if response.changed() {
                        commands.push(set("text", text));
                    }
                    run |= submitted(ui, &response);
                    check(
                        ui,
                        "RegWy",
                        form.text_regex,
                        "regex",
                        "wyrażenie regularne",
                        &mut commands,
                    );
                    check(
                        ui,
                        "wielkość liter",
                        form.text_case,
                        "case",
                        "rozróżniaj wielkość liter",
                        &mut commands,
                    );
                    check(
                        ui,
                        "całe słowa",
                        form.words,
                        "words",
                        "tylko całe słowa",
                        &mut commands,
                    );
                });
                ui.end_row();

                ui.label("Kodowanie:");
                ui.horizontal(|ui| {
                    let current = ENCODINGS
                        .iter()
                        .find(|(e, _, _)| *e == form.encoding)
                        .map_or("", |(_, _, label)| *label);
                    egui::ComboBox::from_id_salt("find_encoding")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (encoding, word, label) in ENCODINGS {
                                if ui
                                    .selectable_label(form.encoding == *encoding, *label)
                                    .clicked()
                                {
                                    commands.push(set("enc", (*word).into()));
                                }
                            }
                        });
                    check(
                        ui,
                        "szukaj też w plikach binarnych",
                        form.binary,
                        "binary",
                        "domyślnie pliki binarne są pomijane",
                        &mut commands,
                    );
                });
                ui.end_row();

                ui.label("Archiwa:");
                ui.horizontal(|ui| {
                    check(
                        ui,
                        "szukaj w archiwach (ZIP, TAR, 7z)",
                        form.archives,
                        "archives",
                        "wchodzi do archiwów bez rozpakowywania",
                        &mut commands,
                    );
                    ui.label("hasło:");
                    let mut password = form.password.clone();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut password)
                                .password(true)
                                .desired_width(140.0),
                        )
                        .changed()
                    {
                        commands.push(set("password", password));
                    }
                });
                ui.end_row();

                ui.label("Rozmiar:");
                ui.horizontal(|ui| {
                    for (label, field, value) in
                        [("od", "min", &form.min), ("do", "max", &form.max)]
                    {
                        ui.label(label);
                        let mut text = value.clone();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut text)
                                    .hint_text("np. 100k, 5m")
                                    .desired_width(90.0),
                            )
                            .changed()
                        {
                            commands.push(set(field, text));
                        }
                    }
                    ui.separator();
                    ui.label("Zmieniony");
                    for (label, field, value) in
                        [("od", "after", &form.after), ("do", "before", &form.before)]
                    {
                        ui.label(label);
                        let mut text = value.clone();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut text)
                                    .hint_text("RRRR-MM-DD")
                                    .desired_width(90.0),
                            )
                            .changed()
                        {
                            commands.push(set(field, text));
                        }
                    }
                });
                ui.end_row();

                ui.label("Opcje:");
                ui.horizontal(|ui| {
                    check(
                        ui,
                        "pliki ukryte",
                        form.hidden,
                        "hidden",
                        "szukaj też w ukrytych plikach i katalogach",
                        &mut commands,
                    );
                    ui.label("limit wyników:");
                    let mut text = form.limit.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(60.0))
                        .changed()
                    {
                        commands.push(set("limit", text));
                    }
                });
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("Szukaj (Enter w polu)"))
                .clicked()
            {
                run = true;
            }
            if ui
                .add_enabled(running, egui::Button::new("Zatrzymaj (Esc)"))
                .clicked()
            {
                commands.push(Command::FindStop);
            }
            if ui
                .add_enabled(count > 0, egui::Button::new("Przejdź do (Enter)"))
                .clicked()
            {
                commands.push(Command::FindGo(self.find_selected + 1));
            }
            if ui
                .add_enabled(count > 0, egui::Button::new("Podgląd (F3)"))
                .clicked()
            {
                commands.push(Command::FindView(self.find_selected + 1));
            }
            if ui.button("Ustawienia domyślne").clicked() {
                commands.push(set("reset", String::new()));
            }
            if ui.button("Zamknij").clicked() {
                commands.push(Command::FindClose);
            }
        });
        self.find_status(
            ui,
            running,
            progress,
            count,
            summary.as_ref(),
            error.as_deref(),
        );
        ui.separator();
        self.find_rows(ui, count, &mut commands);

        if run && !running {
            commands.push(Command::FindRun);
            self.find_selected = 0;
        }
        for command in commands {
            let _ = self.session.execute(command);
        }
    }

    fn find_status(
        &self,
        ui: &mut egui::Ui,
        running: bool,
        progress: Option<(String, u64)>,
        count: usize,
        summary: Option<&bluepane_engine::FindSummary>,
        error: Option<&str>,
    ) {
        if let Some(error) = error {
            ui.colored_label(Color32::LIGHT_RED, error);
        } else if running {
            let (current, files) = progress.unwrap_or_default();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "Szukanie… znaleziono: {count}, przejrzano plików: {files}"
                ));
            });
            ui.label(RichText::new(current).weak());
        } else if let Some(summary) = summary {
            let mut text = format!(
                "Znaleziono: {} · katalogów: {} · plików: {} · archiwów: {}",
                summary.hits, summary.folders, summary.files, summary.archives
            );
            if summary.cancelled {
                text = format!("Zatrzymano. {text}");
            }
            ui.label(RichText::new(text).strong());
            let mut notes = Vec::new();
            if summary.truncated {
                notes.push("osiągnięto limit wyników (zwiększ „limit wyników”)".to_string());
            }
            if summary.unreadable > 0 {
                notes.push(format!("nie dało się odczytać: {}", summary.unreadable));
            }
            if summary.encrypted > 0 {
                notes.push(format!("zaszyfrowane (podaj hasło): {}", summary.encrypted));
            }
            if summary.binary > 0 {
                notes.push(format!("pominięto binarne: {}", summary.binary));
            }
            if !notes.is_empty() {
                ui.colored_label(Color32::YELLOW, notes.join(" · "));
            }
        }
    }

    fn find_rows(&mut self, ui: &mut egui::Ui, count: usize, commands: &mut Vec<Command>) {
        let font = FontId::monospace(13.0);
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            ROW_HEIGHT,
            count,
            |ui, range| {
                let hits: Vec<(usize, FindHit)> = {
                    let Some(view) = self.session.find.as_ref() else {
                        return;
                    };
                    let data = view.data();
                    range
                        .clone()
                        .filter_map(|i| data.hits.get(i).map(|h| (i, h.clone())))
                        .collect()
                };
                for (index, hit) in hits {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        Sense::click(),
                    );
                    if index == self.find_selected {
                        ui.painter()
                            .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            ui.visuals().widgets.hovered.weak_bg_fill,
                        );
                    }
                    let w = rect.width();
                    let at = |f: f32| rect.left() + w * f;
                    let weak = ui.visuals().weak_text_color();
                    let normal = ui.visuals().text_color();
                    let y = rect.center().y;
                    let painter = ui.painter();
                    painter.text(
                        egui::pos2(at(0.0), y),
                        Align2::LEFT_CENTER,
                        format!("{}", index + 1),
                        font.clone(),
                        weak,
                    );
                    let path_clip = egui::Rect::from_min_max(
                        egui::pos2(at(0.04), rect.top()),
                        egui::pos2(at(0.47), rect.bottom()),
                    );
                    let shown = if hit.is_dir {
                        format!("[{}]", hit.location.display())
                    } else {
                        hit.location.display()
                    };
                    painter.with_clip_rect(path_clip).text(
                        egui::pos2(path_clip.left(), y),
                        Align2::LEFT_CENTER,
                        shown,
                        font.clone(),
                        normal,
                    );
                    if let Some(line) = hit.lines.first() {
                        let more = hit.matching_lines.saturating_sub(1);
                        let galley = painter.layout_job(snippet_job(line, more, &font, normal));
                        let clip = egui::Rect::from_min_max(
                            egui::pos2(at(0.48), rect.top()),
                            egui::pos2(at(0.80), rect.bottom()),
                        );
                        painter.with_clip_rect(clip).galley(
                            egui::pos2(clip.left(), y - galley.size().y / 2.0),
                            galley,
                            normal,
                        );
                    }
                    painter.text(
                        egui::pos2(at(0.90), y),
                        Align2::RIGHT_CENTER,
                        if hit.is_dir {
                            String::new()
                        } else {
                            human_size(hit.size)
                        },
                        font.clone(),
                        weak,
                    );
                    painter.text(
                        egui::pos2(at(0.91), y),
                        Align2::LEFT_CENTER,
                        format_modified(hit.modified),
                        font.clone(),
                        weak,
                    );
                    if response.clicked() {
                        self.find_selected = index;
                    }
                    if response.double_clicked() {
                        commands.push(Command::FindGo(index + 1));
                    }
                }
            },
        );
    }
}

/// `L12: text before MATCH text after (+3)` with the match highlighted.
fn snippet_job(line: &FindLine, more: u64, font: &FontId, color: Color32) -> LayoutJob {
    let chars: Vec<char> = line.text.chars().collect();
    let part = |from: usize, to: usize| -> String {
        chars[from.min(chars.len())..to.min(chars.len())]
            .iter()
            .collect()
    };
    let plain = TextFormat::simple(font.clone(), color);
    let marked = TextFormat {
        background: Color32::from_rgb(120, 100, 0),
        ..TextFormat::simple(font.clone(), Color32::WHITE)
    };
    let mut job = LayoutJob::default();
    job.append(
        &format!("L{}: ", line.line),
        0.0,
        TextFormat::simple(font.clone(), Color32::GRAY),
    );
    job.append(part(0, line.start).trim_start(), 0.0, plain.clone());
    job.append(&part(line.start, line.end), 0.0, marked);
    job.append(&part(line.end, chars.len()), 0.0, plain.clone());
    if more > 0 {
        job.append(
            &format!("  (+{more})"),
            0.0,
            TextFormat::simple(font.clone(), Color32::GRAY),
        );
    }
    job
}

/// Enter in a text field starts the search.
fn submitted(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

fn check(
    ui: &mut egui::Ui,
    label: &str,
    on: bool,
    field: &str,
    help: &str,
    commands: &mut Vec<Command>,
) {
    let mut value = on;
    if ui.checkbox(&mut value, label).on_hover_text(help).changed() {
        commands.push(Command::FindSet(
            field.into(),
            if value { "on" } else { "off" }.into(),
        ));
    }
}
