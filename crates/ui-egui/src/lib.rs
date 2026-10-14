//! egui front end.
//!
//! The UI only translates input into command lines and draws the engine's state.
//! It contains no file system logic.

mod find_ui;
mod pages_ui;
mod queue_ui;
mod rename_ui;
mod scan_ui;
mod sites_ui;
mod sync_ui;

use std::time::Duration;

use bluepane_engine::{
    format_modified, human_size, name_parts, ArchiveFormat, ClickMode, Command, CompressionLevel,
    InputKind, Panel, Pending, Session, SortKey, TabAction, ViewMode,
};
use egui::{Align2, Color32, FontId, Rect, RichText};

const CMD_LINE_ID: &str = "bluepane-command-line";

/// F-key bar: (key, label, command line). Same bindings as the default key map.
const FKEYS: &[(&str, &str, &str)] = &[
    ("F2", "Zmień nazwę", "rename"),
    ("F3", "Podgląd", "view"),
    ("F4", "Edycja", "edit"),
    ("F5", "Kopiuj", "copy"),
    ("F6", "Przenieś", "move"),
    ("F7", "Nowy katalog", "mkdir"),
    ("F8", "Do kosza", "delete"),
    ("Alt+F5", "Pakuj", "pack"),
    ("Alt+F9", "Rozpakuj", "unpack"),
    ("Alt+S", "Synchronizuj", "sync"),
    ("Ctrl+F2", "Zmień nazwy", "multirename"),
    ("Ctrl+F", "Szukaj", "find"),
    ("Ctrl+S", "Serwery", "sites"),
    ("Ctrl+J", "Kolejka", "queue"),
    ("Ctrl+N", "Skaner", "scan"),
    ("F9", "Odśwież", "refresh"),
];

pub struct BluepaneUi {
    session: Session,
    command_line: String,
    focus_command_line: bool,
    /// "Apply to all" checkbox of the conflict dialog.
    apply_to_all: bool,
    /// The pack dialog already put the cursor in its name field.
    pack_focused: bool,
    /// Editable text of each panel's path bar (resynced from the panel while not focused).
    path_text: [String; 2],
    /// Settings being edited in the synchronization window.
    sync_form: Option<sync_ui::SyncForm>,
    /// The multi-rename row whose new name is being typed by hand, and the text so far.
    rename_edit: Option<(usize, String)>,
    /// The selected row of the search results.
    find_selected: usize,
    /// The picture of the page on screen (see `pages_ui`).
    page_texture: Option<(pages_ui::TextureKey, egui::TextureHandle)>,
    /// The site being edited in the site manager.
    site_form: Option<sites_ui::SiteForm>,
}

impl BluepaneUi {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            command_line: String::new(),
            focus_command_line: false,
            apply_to_all: false,
            pack_focused: false,
            path_text: [String::new(), String::new()],
            sync_form: None,
            rename_edit: None,
            find_selected: 0,
            page_texture: None,
            site_form: None,
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Draws one frame. Call this from `eframe::App::ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Text a command wants on the clipboard (`copynames`).
        if let Some(text) = self.session.take_clipboard() {
            ctx.copy_text(text);
        }
        // Collect what the background job reported; keep redrawing while it runs.
        self.session.poll();
        let searching = self.session.find.as_ref().is_some_and(|f| f.is_running())
            || self.session.scan.as_ref().is_some_and(|v| v.is_running());
        if self.session.is_busy() || searching {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        let cmd_id = egui::Id::new(CMD_LINE_ID);
        // While any text field (command line, path bar) has focus, keys belong to it.
        if !ctx.egui_wants_keyboard_input() {
            self.handle_keys(&ctx);
        }

        egui::Panel::bottom("bottom").show(ui, |ui| {
            self.bottom_bar(ui, cmd_id);
        });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            if self.session.pages.is_some() {
                self.pages_ui(ui);
            } else if self.session.viewer.is_some() {
                self.viewer_ui(ui);
            } else if self.session.sync.is_some() {
                self.sync_ui(ui);
            } else if self.session.scan.as_ref().is_some_and(|v| v.visible) {
                self.scan_ui(ui);
            } else if self.session.queue_open {
                self.queue_ui(ui);
            } else if self.session.sites_open {
                self.sites_ui(ui);
            } else if self.session.rename.is_some() {
                self.rename_ui(ui);
            } else if self.session.find.as_ref().is_some_and(|f| f.visible) {
                self.find_ui(ui);
            } else {
                ui.columns(2, |columns| {
                    for (index, ui) in columns.iter_mut().enumerate() {
                        self.panel_ui(ui, index);
                    }
                });
            }
        });
        if !matches!(self.session.pending, Some(Pending::Pack(_))) {
            self.pack_focused = false;
        }
        match &self.session.pending {
            Some(Pending::Input { .. }) => self.input_dialog(&ctx),
            Some(Pending::Conflict(_)) => self.conflict_dialog(&ctx),
            Some(Pending::Pack(_)) => self.pack_dialog(&ctx),
            _ => {}
        }
    }

    /// Name dialog for F7 (new directory) and F2 (rename).
    fn input_dialog(&mut self, ctx: &egui::Context) {
        let session = &mut self.session;
        let Some(Pending::Input { kind, text }) = &mut session.pending else {
            return;
        };
        let title = match kind {
            InputKind::Mkdir => "Nowy katalog",
            InputKind::Rename(_) => "Zmień nazwę",
            InputKind::ViewerFind => "Szukaj w pliku",
            InputKind::ArchivePassword => "Hasło archiwum",
            InputKind::SitePassword(_) => "Hasło do serwera SFTP",
        };
        let secret = matches!(
            kind,
            InputKind::ArchivePassword | InputKind::SitePassword(_)
        );
        let (status, is_error) = (&session.status, session.status_is_error);
        let mut accepted = None;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let color = if is_error {
                    Color32::LIGHT_RED
                } else {
                    ui.visuals().weak_text_color()
                };
                ui.label(RichText::new(status).color(color));
                let response = ui.add(
                    egui::TextEdit::singleline(text)
                        .password(secret)
                        .desired_width(340.0),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    accepted = Some(true);
                }
                // The dialog is modal in spirit: keep the cursor in the field.
                response.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK (Enter)").clicked() {
                        accepted = Some(true);
                    }
                    if ui.button("Anuluj (Esc)").clicked() {
                        accepted = Some(false);
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    accepted = Some(false);
                }
            });
        match accepted {
            Some(true) => self.dispatch("confirm"),
            Some(false) => self.dispatch("cancel"),
            None => {}
        }
    }

    /// The settings of `pack` (Alt+F5): name, format, compression, password, volume size.
    fn pack_dialog(&mut self, ctx: &egui::Context) {
        let focus_name = !self.pack_focused;
        self.pack_focused = true;
        let Some(Pending::Pack(form)) = &mut self.session.pending else {
            return;
        };
        let mut action = None;
        let enter = |response: &egui::Response, ui: &egui::Ui| {
            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
        };
        egui::Window::new("Pakowanie")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Elementów: {}   Katalog docelowy: {}",
                    form.item_count(),
                    form.destination().display()
                ));
                ui.add_space(4.0);
                egui::Grid::new("pack-grid")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Archiwum:");
                        ui.vertical(|ui| {
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut form.name).desired_width(300.0),
                            );
                            if focus_name {
                                response.request_focus();
                            }
                            if enter(&response, ui) {
                                action = Some(true);
                            }
                            ui.label(
                                RichText::new(format!("Powstanie plik: {}", form.output_name()))
                                    .weak(),
                            );
                        });
                        ui.end_row();

                        ui.label("Format:");
                        ui.horizontal(|ui| {
                            for (format, label) in [
                                (ArchiveFormat::Zip, "ZIP"),
                                (ArchiveFormat::SevenZ, "7Z"),
                                (ArchiveFormat::Tar, "TAR"),
                                (ArchiveFormat::TarGz, "TAR.GZ"),
                            ] {
                                ui.selectable_value(&mut form.format, format, label);
                            }
                        });
                        ui.end_row();

                        ui.label("Kompresja:");
                        egui::ComboBox::from_id_salt("pack-level")
                            .selected_text(level_label(form.level))
                            .show_ui(ui, |ui| {
                                for level in [
                                    CompressionLevel::Store,
                                    CompressionLevel::Fast,
                                    CompressionLevel::Normal,
                                    CompressionLevel::Maximum,
                                ] {
                                    ui.selectable_value(&mut form.level, level, level_label(level));
                                }
                            });
                        ui.end_row();

                        let encryptable = form.format.supports_password();
                        ui.label("Hasło:");
                        let first = ui.add_enabled(
                            encryptable,
                            egui::TextEdit::singleline(&mut form.password)
                                .password(true)
                                .desired_width(300.0),
                        );
                        if enter(&first, ui) {
                            action = Some(true);
                        }
                        ui.end_row();
                        ui.label("Powtórz hasło:");
                        let again = ui.add_enabled(
                            encryptable,
                            egui::TextEdit::singleline(&mut form.password_again)
                                .password(true)
                                .desired_width(300.0),
                        );
                        if enter(&again, ui) {
                            action = Some(true);
                        }
                        ui.end_row();
                        if !encryptable {
                            ui.label("");
                            ui.label(
                                RichText::new("TAR nie obsługuje haseł (użyj ZIP lub 7Z)").weak(),
                            );
                            ui.end_row();
                        }
                        if form.format.can_encrypt_names() {
                            ui.label("");
                            ui.checkbox(&mut form.encrypt_names, "Szyfruj też nazwy plików");
                            ui.end_row();
                        }

                        ui.label("Podziel na części:");
                        let split = ui.add(
                            egui::TextEdit::singleline(&mut form.split)
                                .hint_text("np. 100m, 700m, 4g (puste = jeden plik)")
                                .desired_width(300.0),
                        );
                        if enter(&split, ui) {
                            action = Some(true);
                        }
                        ui.end_row();
                    });
                if let Some(error) = &form.error {
                    ui.add_space(4.0);
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Pakuj (Enter)").clicked() {
                        action = Some(true);
                    }
                    if ui.button("Anuluj (Esc)").clicked() {
                        action = Some(false);
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    action = Some(false);
                }
            });
        match action {
            Some(true) => self.dispatch("confirm"),
            Some(false) => self.dispatch("cancel"),
            None => {}
        }
    }

    /// Asks what to do about an existing file while copying or moving.
    fn conflict_dialog(&mut self, ctx: &egui::Context) {
        let Some(Pending::Conflict(prompt)) = &self.session.pending else {
            return;
        };
        let conflict = prompt.conflict.clone();
        let source_is_newer = matches!(
            (conflict.src_modified, conflict.dst_modified),
            (Some(new), Some(old)) if new > old
        );
        let mut answer: Option<&'static str> = None;
        egui::Window::new("Plik już istnieje")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(conflict.dst.display().to_string()).strong());
                ui.add_space(4.0);
                egui::Grid::new("conflict-grid").show(ui, |ui| {
                    ui.label("Istniejący:");
                    ui.label(format!(
                        "{}   {}",
                        human_size(conflict.dst_size),
                        format_modified(conflict.dst_modified)
                    ));
                    ui.end_row();
                    ui.label("Nowy:");
                    let note = if source_is_newer { "   (nowszy)" } else { "" };
                    ui.label(format!(
                        "{}   {}{note}",
                        human_size(conflict.src_size),
                        format_modified(conflict.src_modified)
                    ));
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.checkbox(&mut self.apply_to_all, "Dla wszystkich kolejnych plików");
                ui.horizontal_wrapped(|ui| {
                    for (label, command) in [
                        ("Nadpisz", "overwrite"),
                        ("Nadpisz starsze", "newer"),
                        ("Pomiń", "skip"),
                        ("Zachowaj oba", "keepboth"),
                        ("Anuluj", "cancel"),
                    ] {
                        if ui.button(label).clicked() {
                            answer = Some(command);
                        }
                    }
                });
            });
        match answer {
            Some("cancel") => self.dispatch("cancel"),
            Some(command) => {
                let suffix = if self.apply_to_all { "-all" } else { "" };
                self.dispatch(&format!("{command}{suffix}"));
            }
            None => {}
        }
        if answer.is_some() {
            self.apply_to_all = false;
        }
    }

    /// The file viewer (F3), drawn in place of the two panels.
    fn viewer_ui(&mut self, ui: &mut egui::Ui) {
        let font = FontId::monospace(13.0);
        let glyph = ui
            .painter()
            .layout_no_wrap("M".to_string(), font.clone(), Color32::WHITE)
            .size();
        let Some(viewer) = self.session.viewer.as_mut() else {
            return;
        };
        let name = viewer
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mode = match viewer.mode() {
            ViewMode::Text => "tekst",
            ViewMode::Hex => "hex",
        };
        ui.label(
            RichText::new(format!(
                "{name}   [{mode}]   bajt {} z {} ({})   {}%",
                viewer.offset(),
                viewer.size(),
                human_size(viewer.size()),
                viewer.percent()
            ))
            .strong(),
        );
        ui.label(RichText::new(viewer.message.clone()).weak());
        ui.label(
            RichText::new("Esc/Q zamknij · strzałki, PgUp, PgDn, Spacja, Home, End · H hex/tekst · / lub F7 szukaj · N dalej, Shift+N wstecz")
                .weak(),
        );
        ui.separator();

        let available = ui.available_size();
        let rows = ((available.y - 4.0) / glyph.y).floor().max(1.0) as usize;
        let cols = (available.x / glyph.x).floor().max(8.0) as usize;
        viewer.set_viewport(rows, cols);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(available.x, rows as f32 * glyph.y),
            egui::Sense::hover(),
        );
        let hit = viewer.match_offset();
        let shown = viewer.visible_rows();
        // The row holding the last search hit is the last one that starts at or before it.
        let hit_row = hit.and_then(|m| shown.iter().rposition(|r| r.offset <= m));
        let painter = ui.painter_at(rect);
        for (i, row) in shown.iter().enumerate() {
            let top = rect.top() + i as f32 * glyph.y;
            if hit_row == Some(i) {
                let band = Rect::from_min_size(
                    egui::pos2(rect.left(), top),
                    egui::vec2(rect.width(), glyph.y),
                );
                painter.rect_filled(band, 0.0, ui.visuals().selection.bg_fill);
            }
            painter.text(
                egui::pos2(rect.left() + 2.0, top),
                Align2::LEFT_TOP,
                &row.text,
                font.clone(),
                ui.visuals().text_color(),
            );
        }

        // Mouse wheel: about three rows per notch, whatever unit the platform reports.
        let wheel_rows: f32 = if response.hovered() {
            ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                            egui::MouseWheelUnit::Point => delta.y / glyph.y,
                            egui::MouseWheelUnit::Line => delta.y * 3.0,
                            egui::MouseWheelUnit::Page => delta.y * rows as f32,
                        }),
                        _ => None,
                    })
                    .sum()
            })
        } else {
            0.0
        };
        if wheel_rows != 0.0 {
            // Wheel up (positive) scrolls the content down, i.e. towards the file start.
            let step = (-wheel_rows).round() as i64;
            let step = if step == 0 {
                -wheel_rows.signum() as i64
            } else {
                step
            };
            self.dispatch(&format!("viewer scroll {step}"));
        }
    }

    /// Progress of the running job with a cancel button.
    fn progress_ui(&mut self, ui: &mut egui::Ui) {
        let Some(job) = self.session.job_status() else {
            return;
        };
        ui.horizontal(|ui| {
            let queued = if job.queued > 0 {
                format!(" (w kolejce: {})", job.queued)
            } else {
                String::new()
            };
            ui.label(format!("{}{queued}", job.label));
            let bar = match job.fraction() {
                Some(fraction) => egui::ProgressBar::new(fraction)
                    .desired_width(240.0)
                    .show_percentage(),
                None => egui::ProgressBar::new(0.0)
                    .desired_width(240.0)
                    .animate(true)
                    .text("liczenie…"),
            };
            ui.add(bar);
            if job.bytes_total > 0 {
                ui.label(format!(
                    "{} / {}",
                    human_size(job.bytes_done),
                    human_size(job.bytes_total)
                ));
            }
            ui.label(RichText::new(&job.current).weak());
            if job.waiting_for_answer {
                ui.label(RichText::new("czeka na decyzję").color(Color32::YELLOW));
            }
            if ui.button("Anuluj").clicked() {
                self.dispatch("stop");
            }
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let (pressed, typed) = ctx.input(|i| {
            let mut pressed = Vec::new();
            let mut typed = String::new();
            // Alt/Ctrl shortcuts can also arrive as text ("Alt+2" -> "2"); the modifier state
            // of the frame may already be released, so remember it from the key events.
            let mut shortcut = i.modifiers.alt || i.modifiers.ctrl;
            for event in &i.events {
                match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        shortcut |= modifiers.alt || modifiers.ctrl;
                        pressed.push(key_name(*key, *modifiers));
                    }
                    egui::Event::Text(text) => {
                        typed.extend(
                            text.chars()
                                .filter(|c| !c.is_whitespace() && !c.is_control()),
                        );
                    }
                    _ => {}
                }
            }
            if shortcut {
                typed.clear();
            }
            (pressed, typed)
        });
        if self.session.pending.is_some() {
            // A question is open: only the answer keys count.
            for key in pressed {
                match key.as_str() {
                    "Enter" => self.dispatch("confirm"),
                    "Escape" => self.dispatch("cancel"),
                    _ => {}
                }
            }
            return;
        }
        if self.session.pages.is_some() {
            self.pages_keys(&pressed);
            return;
        }
        if self.session.viewer.is_some() {
            // The viewer has its own keys; typing must not start the panel filter.
            for key in pressed {
                if let Some(line) = self.session.keymap.lookup_viewer(&key).map(str::to_owned) {
                    self.dispatch(&line);
                }
            }
            return;
        }
        if self.session.scan.as_ref().is_some_and(|v| v.visible) {
            self.scan_keys(&pressed);
            return;
        }
        if self.session.queue_open {
            self.queue_keys(&pressed);
            return;
        }
        if self.session.sites_open {
            self.sites_keys(&pressed);
            return;
        }
        if self.session.find.as_ref().is_some_and(|f| f.visible) {
            // Typing in the search window must not start the panel filter.
            self.find_keys(&pressed);
            return;
        }
        if self.session.rename.is_some() {
            // Typing in the multi-rename window must not start the panel filter.
            self.rename_keys(&pressed);
            return;
        }
        if self.session.sync.is_some() {
            // The synchronization window has few keys; typing must not start the panel filter.
            let dirty = self.sync_form_is_dirty();
            for key in pressed {
                match key.as_str() {
                    "Escape" => self.dispatch("syncclose"),
                    // Running needs the settings on screen to be the ones compared.
                    "F5" if !dirty => self.dispatch("syncrun"),
                    "F9" => self.sync_compare_now(),
                    _ => {}
                }
            }
            return;
        }
        for key in pressed {
            // While a filter is being typed, Space is part of the name, not "select".
            if key == "Space" && !self.session.active_panel().filter.is_empty() {
                let filter = format!("{} ", self.session.active_panel().filter);
                let _ = self.session.execute(Command::Filter(filter));
                continue;
            }
            if let Some(line) = self.session.keymap.lookup(&key).map(str::to_owned) {
                self.dispatch(&line);
            }
        }
        if !typed.is_empty() {
            let filter = format!("{}{typed}", self.session.active_panel().filter);
            let _ = self.session.execute(Command::Filter(filter));
        }
    }

    /// Runs a command line, or puts it in the command line if it needs an argument.
    fn dispatch(&mut self, line: &str) {
        if line.ends_with(' ') {
            self.command_line = line.to_string();
            self.focus_command_line = true;
        } else {
            let _ = self.session.run_line(line);
        }
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, cmd_id: egui::Id) {
        let status_color = if self.session.status_is_error {
            Color32::LIGHT_RED
        } else {
            ui.visuals().weak_text_color()
        };
        self.progress_ui(ui);
        // Conflicts and name dialogs have their own window; only yes/no questions use the bar.
        let simple_question = matches!(
            self.session.pending,
            Some(
                Pending::Duplicate(_)
                    | Pending::ConfirmDelete(_)
                    | Pending::ConfirmRemove { .. }
                    | Pending::TrustHost(_)
                    | Pending::ConfirmRemoteDelete { .. }
            )
        );
        if simple_question {
            ui.label(RichText::new(&self.session.status).color(Color32::YELLOW));
            ui.horizontal(|ui| {
                if ui.button("Tak (Enter)").clicked() {
                    self.dispatch("confirm");
                }
                if ui.button("Nie (Esc)").clicked() {
                    self.dispatch("cancel");
                }
            });
        } else {
            ui.label(RichText::new(&self.session.status).color(status_color));
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(">").monospace());
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.command_line)
                    .id(cmd_id)
                    .desired_width(f32::INFINITY)
                    .hint_text("komenda, np. cd /tmp, mkdir Raporty, copy"),
            );
            if self.focus_command_line {
                response.request_focus();
                self.focus_command_line = false;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let line = std::mem::take(&mut self.command_line);
                if !line.trim().is_empty() {
                    let _ = self.session.run_line(&line);
                }
            }
        });

        ui.horizontal(|ui| {
            for (key, label, line) in FKEYS {
                if ui.button(format!("{key} {label}")).clicked() {
                    self.dispatch(line);
                }
            }
        });
    }

    /// The tab row above a panel: click to switch, × or middle-click to close, + for a new tab.
    fn tabs_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let strip = self.session.tab_strip(index);
        let several = strip.titles.len() > 1;
        let side_is_active = self.session.active == index;
        let (mut switch_to, mut close, mut new_tab) = (None, None, false);
        ui.horizontal_wrapped(|ui| {
            for (i, title) in strip.titles.iter().enumerate() {
                let selected = i == strip.active;
                let label: String = if title.chars().count() > 18 {
                    format!("{}…", title.chars().take(17).collect::<String>())
                } else {
                    title.clone()
                };
                let response = ui
                    .selectable_label(selected && (side_is_active || !several), label)
                    .on_hover_text(strip.paths[i].display().to_string());
                if response.clicked() {
                    switch_to = Some(i);
                }
                if several && response.middle_clicked() {
                    close = Some(i);
                }
                if several
                    && ui
                        .small_button("×")
                        .on_hover_text("Zamknij zakładkę")
                        .clicked()
                {
                    close = Some(i);
                }
                ui.add_space(4.0);
            }
            if ui
                .small_button("+")
                .on_hover_text("Nowa zakładka (Ctrl+T)")
                .clicked()
            {
                new_tab = true;
            }
        });
        let action = match (close, switch_to, new_tab) {
            (Some(i), _, _) => Some(TabAction::Close(Some(i + 1))),
            (None, Some(i), _) if i != strip.active => Some(TabAction::Goto(i + 1)),
            (None, _, true) => Some(TabAction::New),
            _ => None,
        };
        // Clicking anywhere in a panel's tab row also makes that side the active one.
        if close.is_some() || switch_to.is_some() || new_tab {
            self.session.active = index;
        }
        if let Some(action) = action {
            let _ = self.session.execute(Command::Tab(action));
        }
    }

    /// Volume picker and editable path bar of one panel.
    fn header_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let is_active = self.session.active == index;
        let current_volume = self.session.volume_of(index);
        let mut chosen = None;
        let path_id = egui::Id::new(("path-bar", index));
        let editing = ui.ctx().memory(|m| m.has_focus(path_id));
        if !editing {
            let panel = &self.session.panels[index];
            self.path_text[index] = match &panel.virtual_list {
                Some(list) => list.title.clone(),
                None => panel.location().display().to_string(),
            };
        }

        ui.horizontal(|ui| {
            let selected_text = current_volume
                .map(|i| self.session.volumes[i].name.clone())
                .unwrap_or_else(|| "?".to_string());
            egui::ComboBox::from_id_salt(("volume", index))
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (i, volume) in self.session.volumes.iter().enumerate() {
                        let label = format!("{} {}", i + 1, volume.name);
                        if ui
                            .selectable_label(current_volume == Some(i), label)
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                    }
                });

            let mut edit = egui::TextEdit::singleline(&mut self.path_text[index])
                .id(path_id)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY);
            if is_active {
                edit = edit.text_color(Color32::from_rgb(120, 200, 255));
            }
            let response = ui.add(edit);
            if response.gained_focus() {
                self.session.active = index;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.session.active = index;
                let line = format!("cd {}", self.path_text[index]);
                let _ = self.session.run_line(&line);
            }
        });

        if let Some(i) = chosen {
            self.session.active = index;
            let _ = self.session.execute(Command::Volume((i + 1).to_string()));
        }
    }

    fn panel_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let is_active = self.session.active == index;
        self.tabs_ui(ui, index);
        self.header_ui(ui, index);
        let panel: &Panel = &self.session.panels[index];

        if let Some(err) = &panel.error {
            ui.colored_label(Color32::LIGHT_RED, err);
        }
        if !panel.filter.is_empty() {
            ui.label(
                RichText::new(format!("Filtr: {}  (Esc czyści)", panel.filter))
                    .color(Color32::YELLOW),
            );
        }
        ui.separator();

        let row_height = ui.spacing().interact_size.y;
        let stride = row_height + ui.spacing().item_spacing.y;
        let cursor = panel.cursor;
        let sort = panel.sort;
        let mut clicked = None;
        let mut entered = None;
        let mut sort_by = None;

        // Column header: click to sort by that column, click again to flip the direction.
        let (header_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_height),
            egui::Sense::hover(),
        );
        let columns = Columns::split(header_rect);
        let font = FontId::monospace(13.0);
        for (rect, key, label) in [
            (columns.name, SortKey::Name, "Nazwa"),
            (columns.ext, SortKey::Ext, "Typ"),
            (columns.size, SortKey::Size, "Rozmiar"),
            (columns.date, SortKey::Date, "Data"),
        ] {
            let response = ui.interact(
                rect,
                ui.id().with(("sort", index, label)),
                egui::Sense::click(),
            );
            let arrow = match (sort.key == key, sort.descending) {
                (false, _) => "",
                (true, false) => " ▲",
                (true, true) => " ▼",
            };
            let color = if response.hovered() {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            let (pos, align) = if key == SortKey::Size {
                (
                    egui::pos2(rect.right() - COLUMN_PAD, rect.center().y),
                    Align2::RIGHT_CENTER,
                )
            } else {
                (
                    egui::pos2(rect.left() + COLUMN_PAD, rect.center().y),
                    Align2::LEFT_CENTER,
                )
            };
            ui.painter()
                .text(pos, align, format!("{label}{arrow}"), font.clone(), color);
            if response.clicked() {
                sort_by = Some(key);
            }
        }

        egui::ScrollArea::vertical()
            .id_salt(("panel", index))
            .auto_shrink([false, false])
            .show_rows(ui, row_height, panel.entries.len(), |ui, range| {
                if is_active {
                    let y =
                        ui.min_rect().min.y + cursor.saturating_sub(range.start) as f32 * stride;
                    if cursor < range.start || cursor >= range.end {
                        let rect = Rect::from_min_size(
                            egui::pos2(ui.min_rect().min.x, y),
                            egui::vec2(1.0, row_height),
                        );
                        ui.scroll_to_rect(rect, None);
                    }
                }
                for row in range {
                    let entry = &panel.entries[row];
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_height),
                        egui::Sense::click(),
                    );
                    let is_selected = panel.selected.contains(&entry.path);
                    if row == cursor {
                        let fill = if is_active {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().widgets.inactive.weak_bg_fill
                        };
                        ui.painter().rect_filled(rect, 2.0, fill);
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            ui.visuals().widgets.hovered.weak_bg_fill,
                        );
                    }
                    let color = if is_selected {
                        Color32::YELLOW
                    } else if entry.hidden {
                        ui.visuals().weak_text_color()
                    } else {
                        ui.visuals().text_color()
                    };
                    let cols = Columns::split(rect);
                    let (stem, ext) = name_parts(&entry.name, entry.is_dir());
                    let mut name = if entry.is_dir() {
                        format!("[{stem}]")
                    } else {
                        stem.to_string()
                    };
                    // In a list of results the folder is part of what identifies a row.
                    if panel.virtual_list.is_some() && !bluepane_engine::is_parent_entry(entry) {
                        if let Some(dir) = entry.path.parent() {
                            name = format!("{name}    {}", dir.display());
                        }
                    }
                    let size = if entry.is_dir() {
                        "<DIR>".to_string()
                    } else {
                        human_size(entry.size)
                    };
                    let painter = ui.painter();
                    let y = rect.center().y;
                    // Each text is clipped to its own column so long names do not spill over.
                    let cell = |col: Rect, align: Align2, text: String| {
                        let x = if align == Align2::RIGHT_CENTER {
                            col.right() - COLUMN_PAD
                        } else {
                            col.left() + COLUMN_PAD
                        };
                        painter.with_clip_rect(col).text(
                            egui::pos2(x, y),
                            align,
                            text,
                            font.clone(),
                            color,
                        );
                    };
                    cell(cols.name, Align2::LEFT_CENTER, name);
                    cell(cols.ext, Align2::LEFT_CENTER, ext.to_string());
                    cell(cols.size, Align2::RIGHT_CENTER, size);
                    cell(
                        cols.date,
                        Align2::LEFT_CENTER,
                        format_modified(entry.modified),
                    );
                    if response.clicked() {
                        let m = ui.input(|i| i.modifiers);
                        clicked = Some((row, (m.command, m.shift)));
                    }
                    if response.double_clicked() {
                        entered = Some(row);
                    }
                }
            });

        if let Some(row) = entered {
            self.session.active = index;
            self.session.panels[index].cursor = row;
        }
        if let Some((row, (ctrl, shift))) = clicked {
            // Selecting with the mouse is a command like every other action.
            self.session.active = index;
            let mode = match (ctrl, shift) {
                (false, false) => ClickMode::Plain,
                (true, false) => ClickMode::Ctrl,
                (false, true) => ClickMode::Shift,
                (true, true) => ClickMode::CtrlShift,
            };
            let _ = self.session.execute(Command::Click(row + 1, mode));
        }
        if entered.is_some() {
            // Through the engine: Enter on an archive file opens it instead of doing nothing.
            let _ = self.session.execute(Command::Enter);
        }
        if let Some(key) = sort_by {
            self.session.active = index;
            let _ = self.session.execute(Command::Sort(key, None));
        }
    }
}

const COLUMN_PAD: f32 = 6.0;

/// Horizontal layout of a panel row: a flexible name column and fixed columns on the right.
struct Columns {
    name: Rect,
    ext: Rect,
    size: Rect,
    date: Rect,
}

impl Columns {
    const EXT: f32 = 56.0;
    const SIZE: f32 = 80.0;
    const DATE: f32 = 140.0;

    fn split(row: Rect) -> Columns {
        let date_left = row.right() - Self::DATE;
        let size_left = date_left - Self::SIZE;
        let ext_left = size_left - Self::EXT;
        let cut = |left: f32, right: f32| {
            Rect::from_min_max(egui::pos2(left, row.top()), egui::pos2(right, row.bottom()))
        };
        Columns {
            name: cut(row.left(), ext_left.max(row.left())),
            ext: cut(ext_left.max(row.left()), size_left.max(row.left())),
            size: cut(size_left.max(row.left()), date_left.max(row.left())),
            date: cut(date_left.max(row.left()), row.right()),
        }
    }
}

fn level_label(level: CompressionLevel) -> &'static str {
    match level {
        CompressionLevel::Store => "Bez kompresji",
        CompressionLevel::Fast => "Szybka",
        CompressionLevel::Normal => "Normalna",
        CompressionLevel::Maximum => "Maksymalna",
    }
}

/// Key name as the engine's `Keymap` knows it: `F5`, `Alt+ArrowLeft`, `Ctrl+L`.
fn key_name(key: egui::Key, modifiers: egui::Modifiers) -> String {
    let mut name = String::new();
    if modifiers.ctrl {
        name.push_str("Ctrl+");
    }
    if modifiers.alt {
        name.push_str("Alt+");
    }
    if modifiers.shift {
        name.push_str("Shift+");
    }
    name.push_str(&format!("{key:?}"));
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_carry_modifiers() {
        let none = egui::Modifiers::NONE;
        assert_eq!(key_name(egui::Key::F5, none), "F5");
        assert_eq!(
            key_name(egui::Key::ArrowLeft, egui::Modifiers::ALT),
            "Alt+ArrowLeft"
        );
        assert_eq!(key_name(egui::Key::L, egui::Modifiers::CTRL), "Ctrl+L");
    }

    #[test]
    fn columns_tile_the_row_without_overlap() {
        let row = Rect::from_min_max(egui::pos2(10.0, 0.0), egui::pos2(500.0, 20.0));
        let c = Columns::split(row);
        assert_eq!(c.name.left(), 10.0);
        assert_eq!(c.name.right(), c.ext.left());
        assert_eq!(c.ext.right(), c.size.left());
        assert_eq!(c.size.right(), c.date.left());
        assert_eq!(c.date.right(), 500.0);
    }

    #[test]
    fn narrow_rows_do_not_produce_inverted_columns() {
        let row = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 20.0));
        let c = Columns::split(row);
        for rect in [c.name, c.ext, c.size, c.date] {
            assert!(rect.width() >= 0.0);
        }
    }
}
