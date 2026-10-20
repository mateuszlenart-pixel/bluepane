//! egui front end.
//!
//! The UI only translates input into command lines and draws the engine's state.
//! It contains no file system logic.

mod bottom_ui;
mod context_ui;
mod dock_ui;
mod drag_ui;
mod find_ui;
mod grid_ui;
mod keys_ui;
mod omnibox_ui;
mod pages_ui;
mod palette_ui;
mod places_ui;
mod queue_ui;
mod quick_ui;
mod rename_ui;
mod scan_ui;
mod sites_ui;
mod sync_ui;
mod theme;
mod toast_ui;

use std::time::Duration;

use bluepane_engine::{
    format_modified, human_size, name_parts, ArchiveFormat, ClickMode, Column, Command,
    CompressionLevel, InputKind, Panel, Pending, Session, SortKey, TabAction, ViewMode,
};
use egui::{Align2, Color32, FontId, Rect, RichText};

const CMD_LINE_ID: &str = "bluepane-command-line";

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
    /// The open right-click menu: where it is and what it offers.
    context_menu: Option<ContextMenu>,
    toasts: Vec<toast_ui::Toast>,
    /// Textures of the thumbnails in the grid view.
    thumb_textures: grid_ui::Textures,
    /// Tiles per row in each panel's grid, as drawn in the last frame.
    grid_cols: [usize; 2],
    /// Since when Ctrl or Alt alone has been held (the key sheet appears after a moment).
    modifier_hold: Option<(&'static str, f64)>,
    /// The name being edited in its row has been given the keyboard.
    rename_focused: bool,
    /// A row of this frame holds the in-place name editor (the name dialog is not needed).
    inline_rename_drawn: bool,
    /// A drag of marked entries in progress.
    drag: Option<DragState>,
    /// Where something can be dropped, as drawn in the last frame and in this one.
    drop_zones: Vec<DropZone>,
    last_zones: Vec<DropZone>,
    /// Counts frames (to tell the frame a menu was opened in).
    frame: u64,
    /// The folders listed in the open breadcrumb menu, with the folder they are inside of.
    crumb_menu: Option<(std::path::PathBuf, Vec<bluepane_engine::Crumb>)>,
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
    /// The palette in use and the name of the theme egui was last given.
    theme: theme::Theme,
    theme_applied: bool,
    /// The theme name, text size and accent the egui style was last built from.
    look_applied: (String, u16, Option<[u8; 3]>),
    /// Set when a click on the empty part of a path bar asked to edit the path.
    path_edit: [bool; 2],
    /// Speed of the running job, for the bar at the bottom.
    speed: bottom_ui::SpeedMeter,
    /// The window as a dock of tabs (panels and tools).
    dock: Option<egui_dock::DockState<dock_ui::Tab>>,
    /// The tab the user last clicked in; tool keys go to it.
    focus: dock_ui::Tab,
    /// The layout text last written and when the file was last looked at.
    layout_saved: String,
    layout_saved_at: std::time::Instant,
}

impl BluepaneUi {
    pub fn new(mut session: Session) -> Self {
        // The saved settings (theme) are needed before the first frame.
        session.load_config();
        let theme_name = session.theme().to_string();
        Self {
            session,
            command_line: String::new(),
            focus_command_line: false,
            apply_to_all: false,
            pack_focused: false,
            path_text: [String::new(), String::new()],
            context_menu: None,
            toasts: Vec::new(),
            thumb_textures: grid_ui::Textures::default(),
            grid_cols: [1, 1],
            modifier_hold: None,
            rename_focused: false,
            inline_rename_drawn: false,
            drag: None,
            drop_zones: Vec::new(),
            last_zones: Vec::new(),
            frame: 0,
            crumb_menu: None,
            sync_form: None,
            rename_edit: None,
            find_selected: 0,
            page_texture: None,
            site_form: None,
            theme: theme::Theme::by_name(&theme_name),
            theme_applied: false,
            look_applied: (String::new(), 100, None),
            path_edit: [false; 2],
            speed: bottom_ui::SpeedMeter::default(),
            dock: None,
            focus: dock_ui::Tab::Panel(0),
            layout_saved: String::new(),
            layout_saved_at: std::time::Instant::now(),
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The session, for driving the window from outside (tests, scripts).
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Draws one frame. Call this from `eframe::App::ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        self.last_zones = std::mem::take(&mut self.drop_zones);
        self.inline_rename_drawn = false;
        if !matches!(
            self.session.pending,
            Some(Pending::Input {
                kind: InputKind::Rename(_),
                ..
            })
        ) {
            self.rename_focused = false;
        }
        // The colours follow the session's theme (`theme` command, saved settings).
        if !self.theme_applied {
            // Icon font: Phosphor symbols become available in every proportional text.
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            ctx.set_fonts(fonts);
            // egui closes the program on Ctrl+Q by default; Ctrl+Q is Quick View here.
            ctx.options_mut(|options| options.quit_shortcuts.clear());
        }
        let system_dark = ctx.system_theme().map(|t| t == egui::Theme::Dark);
        let look = (
            theme::Theme::resolve(self.session.theme(), system_dark).to_string(),
            self.session.text_size(),
            self.session.accent(),
        );
        if !self.theme_applied || self.look_applied != look {
            self.theme = theme::Theme::by_name(&look.0).with_accent(look.2);
            self.theme.apply(&ctx, look.1);
            self.look_applied = look;
            self.theme_applied = true;
        }
        // Text a command wants on the clipboard (`copynames`).
        if let Some(text) = self.session.take_clipboard() {
            ctx.copy_text(text);
        }
        // A command chosen in the palette that still needs its argument.
        if let Some(line) = self.session.command_prefill.take() {
            self.command_line = line;
            self.focus_command_line = true;
        }
        // Collect what the background job reported; keep redrawing while it runs.
        self.session.poll();
        let searching = self.session.find.as_ref().is_some_and(|f| f.is_running())
            || self.session.scan.as_ref().is_some_and(|v| v.is_running())
            || self.session.dir_sizes.is_running()
            || self.session.disk_usage.is_asking()
            || self.session.quick_pending()
            || self.session.thumbnails.is_working();
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
        if self.session.places_open {
            self.places_bar(ui);
        }
        egui::CentralPanel::default_margins().show(ui, |ui| self.dock_ui(ui));
        if !matches!(self.session.pending, Some(Pending::Pack(_))) {
            self.pack_focused = false;
        }
        if std::mem::take(&mut self.session.menu_requested) {
            let pos = ctx
                .pointer_latest_pos()
                .unwrap_or(ctx.content_rect().center());
            self.open_context_menu(pos, true);
        }
        self.key_sheet_ui(&ctx);
        self.toasts_ui(&ctx);
        self.drag_overlay(&ctx);
        self.context_menu_popup(&ctx);
        self.palette_dialog(&ctx);
        self.jump_dialog(&ctx);
        self.box_dialog(&ctx);
        match &self.session.pending {
            // F2 edits the name in its row; the dialog is the fallback when the row is not shown.
            Some(Pending::Input {
                kind: InputKind::Rename(_),
                ..
            }) if self.inline_rename_drawn => {}
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
            InputKind::PdfPassword(_) => "Hasło do dokumentu PDF",
        };
        let secret = matches!(
            kind,
            InputKind::ArchivePassword | InputKind::SitePassword(_) | InputKind::PdfPassword(_)
        );
        let (status, is_error) = (&session.status, session.status_is_error);
        let mut accepted = None;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let color = if is_error {
                    ui.visuals().error_fg_color
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
                    ui.colored_label(ui.visuals().error_fg_color, error);
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

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let (pressed, mut typed) = ctx.input(|i| {
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
        // The key event and the text of `?` may come in different frames: while `?` opens the
        // sheet, it never goes into the filter.
        let sheet_key = ["Questionmark", "Shift+Questionmark"]
            .iter()
            .any(|k| self.session.keymap.lookup(k) == Some("keys"));
        // A tool tab that has the focus gets the keys; otherwise they are for the panels.
        if let Some(tool) = self.focused_tool() {
            self.tool_keys(tool, &pressed);
            return;
        }
        for key in pressed {
            // In the grid the arrows move by a tile sideways and by a row of tiles up and down.
            if self.session.active_panel().view == bluepane_engine::PanelView::Grid {
                let cols = self.grid_cols[self.session.active] as i64;
                let step = match key.as_str() {
                    "ArrowUp" => Some(-cols),
                    "ArrowDown" => Some(cols),
                    "ArrowLeft" => Some(-1),
                    "ArrowRight" => Some(1),
                    _ => None,
                };
                if let Some(step) = step {
                    let _ = self.session.run_line(&format!("cursor {step}"));
                    continue;
                }
            }
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
        if sheet_key {
            typed.retain(|c| c != '?');
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
            self.theme.error
        } else {
            self.theme.muted
        };
        self.job_bar(ui);
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
            ui.label(RichText::new(&self.session.status).color(self.theme.warn));
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

        self.action_bar_ui(ui);
        self.fkey_bar(ui);
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

    /// Volume picker with the drive's free space, and the path bar of one panel: clickable
    /// segments, or an editable text after a click on the empty part of the bar.
    fn header_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        use egui_phosphor::regular as icon;
        let is_active = self.session.active == index;
        let current_volume = self.session.volume_of(index);
        let mut chosen = None;
        let mut go_to: Option<std::path::PathBuf> = None;
        let path_id = egui::Id::new(("path-bar", index));
        let focused = ui.ctx().memory(|m| m.has_focus(path_id));
        let editing = focused || self.path_edit[index];
        if !editing {
            let panel = &self.session.panels[index];
            self.path_text[index] = match &panel.virtual_list {
                Some(list) => list.title.clone(),
                None => panel.location().display().to_string(),
            };
        }
        let theme = self.theme.clone();
        let space = self.session.volume_space(index);

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
            // How full the drive is: a thin bar and the numbers.
            if let Some(space) = space {
                let used = space.used_fraction();
                let fill = if used >= 0.95 {
                    theme.error
                } else if used >= 0.85 {
                    theme.warn
                } else {
                    theme.accent
                };
                ui.add(
                    egui::ProgressBar::new(used)
                        .desired_width(70.0)
                        .desired_height(8.0)
                        .fill(fill),
                );
                ui.label(
                    RichText::new(format!(
                        "{} wolne z {}",
                        human_size(space.free),
                        human_size(space.total)
                    ))
                    .color(theme.muted)
                    .small(),
                );
            }
        });

        let crumbs = if editing {
            Vec::new()
        } else {
            self.session.panel_crumbs(index)
        };
        if crumbs.is_empty() {
            // An archive, a result list, or an edit in progress: the path as text.
            let mut edit = egui::TextEdit::singleline(&mut self.path_text[index])
                .id(path_id)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY);
            if is_active {
                edit = edit.text_color(theme.accent);
            }
            let response = ui.add(edit);
            if self.path_edit[index] {
                response.request_focus();
                self.path_edit[index] = false;
            }
            if response.gained_focus() {
                self.session.active = index;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.session.active = index;
                let line = format!("go {}", self.path_text[index]);
                let _ = self.session.run_line(&line);
            } else if response.has_focus() || response.lost_focus() {
                self.omnibox_hints_popup(ui.ctx(), index, &response);
            }
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                // A long path shows its tail; the part cut off is one click away. How many
                // segments fit is worked out from the width of a monospace character.
                let char_width = ui
                    .painter()
                    .layout_no_wrap("M".to_string(), FontId::monospace(13.0), Color32::WHITE)
                    .size()
                    .x;
                let budget = ui.available_width() - 40.0;
                let mut used = 0.0;
                let mut hidden = crumbs.len() - 1;
                for (i, crumb) in crumbs.iter().enumerate().rev() {
                    used += crumb.label.chars().count() as f32 * char_width + 22.0;
                    if used > budget && i + 1 < crumbs.len() {
                        break;
                    }
                    hidden = i;
                }
                if hidden > 0
                    && ui
                        .add(egui::Button::new("…").frame(false))
                        .on_hover_text(crumbs[hidden - 1].path.display().to_string())
                        .clicked()
                {
                    go_to = Some(crumbs[hidden - 1].path.clone());
                }
                for (i, crumb) in crumbs.iter().enumerate().skip(hidden) {
                    if i > hidden || hidden > 0 {
                        // The separator opens the folders inside the segment before it.
                        let caret = ui
                            .add(
                                egui::Button::new(
                                    RichText::new(icon::CARET_RIGHT).color(theme.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Pokaż katalogi w środku");
                        let parent = crumbs[i - 1].path.clone();
                        if caret.clicked() {
                            let items = self.session.crumb_subfolders(index, &parent);
                            self.crumb_menu = Some((parent.clone(), items));
                        }
                        let items = match &self.crumb_menu {
                            Some((menu_parent, items)) if *menu_parent == parent => items.clone(),
                            _ => Vec::new(),
                        };
                        egui::Popup::menu(&caret).show(|ui| {
                            ui.set_max_height(360.0);
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                if items.is_empty() {
                                    ui.label(RichText::new("Brak katalogów").color(theme.muted));
                                }
                                for folder in &items {
                                    if ui.button(&folder.label).clicked() {
                                        go_to = Some(folder.path.clone());
                                    }
                                }
                            });
                        });
                    }
                    let last = i + 1 == crumbs.len();
                    let color = if last && is_active {
                        theme.accent
                    } else if last {
                        theme.text
                    } else {
                        theme.dir
                    };
                    let text = RichText::new(&crumb.label).monospace().color(color);
                    let text = if last { text.strong() } else { text };
                    if ui
                        .add(egui::Button::new(text).frame(false))
                        .on_hover_text(crumb.path.display().to_string())
                        .clicked()
                    {
                        go_to = Some(crumb.path.clone());
                    }
                }
                // The empty rest of the bar: a click turns the path into text to edit.
                let rest = egui::vec2(ui.available_width().max(12.0), ui.spacing().interact_size.y);
                let (_, response) = ui.allocate_exact_size(rest, egui::Sense::click());
                if response
                    .on_hover_text("Kliknij, żeby wpisać ścieżkę")
                    .clicked()
                {
                    self.path_edit[index] = true;
                    self.session.active = index;
                }
            });
        }

        if let Some(i) = chosen {
            self.session.active = index;
            let _ = self.session.execute(Command::Volume((i + 1).to_string()));
        }
        if let Some(path) = go_to {
            self.session.active = index;
            let _ = self.session.run_line(&format!("cd {}", path.display()));
        }
    }

    /// One side: a frame that shows whether it has the keyboard, with the panel inside.
    fn panel_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let (fill, stroke) = if self.session.active == index {
            (self.theme.panel, egui::Stroke::new(1.5, self.theme.accent))
        } else {
            (
                self.theme.panel_inactive,
                egui::Stroke::new(1.0, self.theme.border),
            )
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(4)
            .inner_margin(6)
            .show(ui, |ui| self.panel_body(ui, index));
    }

    fn panel_body(&mut self, ui: &mut egui::Ui, index: usize) {
        let is_active = self.session.active == index;
        self.tabs_ui(ui, index);
        self.header_ui(ui, index);
        let panel: &Panel = &self.session.panels[index];

        if let Some(err) = &panel.error {
            ui.colored_label(self.theme.error, err);
        }
        let mut clear_filter = false;
        if !panel.filter.is_empty() {
            egui::Frame::new()
                .fill(self.theme.header)
                .stroke(egui::Stroke::new(1.0, self.theme.warn))
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(8, 1))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} Filtr: {}",
                                egui_phosphor::regular::MAGNIFYING_GLASS,
                                panel.filter
                            ))
                            .color(self.theme.warn),
                        );
                        if ui
                            .add(
                                egui::Button::new(egui_phosphor::regular::X)
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text("Wyczyść filtr (Esc)")
                            .clicked()
                        {
                            clear_filter = true;
                        }
                    });
                });
        }
        ui.separator();

        let theme = self.theme.clone();
        let session = &self.session;
        // Footer text and the size the longest bar stands for: cached by the engine.
        let stats = session.panel_stats(index);
        let (biggest, summary) = (stats.biggest, stats.summary);
        let row_height = match (self.session.row_height(), self.session.density()) {
            (Some(points), _) => f32::from(points),
            (None, bluepane_engine::Density::Comfortable) => ui.spacing().interact_size.y,
            (None, bluepane_engine::Density::Compact) => {
                (ui.spacing().interact_size.y - 4.0).max(14.0)
            }
        };
        let stride = row_height + ui.spacing().item_spacing.y;
        let cursor = panel.cursor;
        let sort = panel.sort;
        // F2: the name is edited in its row (the engine keeps the text in the pending question).
        let inline_rename: Option<(std::path::PathBuf, String)> = match &session.pending {
            Some(Pending::Input {
                kind: InputKind::Rename(path),
                text,
            }) if is_active => Some((path.clone(), text.clone())),
            _ => None,
        };
        let first_frame_of_rename = !self.rename_focused;
        let mut rename_text: Option<String> = inline_rename.as_ref().map(|(_, t)| t.clone());
        let mut rename_drawn = false;
        let mut rename_end: Option<bool> = None;
        let mut clicked = None;
        let mut zones: Vec<DropZone> = Vec::new();
        let mut drag_started: Option<usize> = None;
        let mut entered = None;
        let mut sort_by = None;

        // Column header: click to sort by that column, click again to flip the direction.
        let (header_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_height),
            egui::Sense::hover(),
        );
        let shown_columns = self.session.columns();
        let columns = Columns::split(header_rect, &shown_columns);
        let font = FontId::monospace(13.0);
        for (rect, key, label) in [
            (columns.name, Some(SortKey::Name), "Nazwa"),
            (columns.ext, Some(SortKey::Ext), "Typ"),
            (columns.size, Some(SortKey::Size), "Rozmiar"),
            (columns.created, None, "Utworzono"),
            (columns.date, Some(SortKey::Date), "Data"),
        ] {
            // A column dropped because the panel is narrow has no title either.
            if rect.width() < 1.0 {
                continue;
            }
            let response = ui.interact(
                rect,
                ui.id().with(("sort", index, label)),
                egui::Sense::click(),
            );
            let arrow = match (Some(sort.key) == key, sort.descending) {
                (false, _) => "",
                (true, false) => " ▲",
                (true, true) => " ▼",
            };
            let color = if response.hovered() {
                self.theme.text
            } else {
                self.theme.muted
            };
            let (pos, align) = if key == Some(SortKey::Size) {
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
                sort_by = key;
            }
        }

        // The line under the list (marked files, totals) keeps its own room.
        let footer_height = row_height + 6.0;
        // Behind the rows: a right click on the empty part of the list lands here.
        let list_rect = Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(
                ui.available_width(),
                (ui.available_height() - footer_height).max(row_height * 3.0),
            ),
        );
        let backdrop = ui.interact(
            list_rect,
            ui.id().with(("list-backdrop", index)),
            egui::Sense::click(),
        );
        zones.push(DropZone {
            rect: list_rect,
            panel: index,
            row: None,
        });
        let mut right_clicked: Option<usize> = None;
        let list_height = (ui.available_height() - footer_height).max(row_height * 3.0);
        if panel.view == bluepane_engine::PanelView::Grid {
            self.grid_cols[index] = grid_ui::columns(ui.available_width());
            let input = grid_ui::grid(
                ui,
                index,
                panel,
                is_active,
                &theme,
                &session.thumbnails,
                &mut self.thumb_textures,
                &mut zones,
                list_height,
            );
            clicked = input.clicked;
            entered = input.entered;
            right_clicked = input.right_clicked;
            drag_started = input.drag_started;
        } else {
            egui::ScrollArea::vertical()
                .id_salt(("panel", index))
                .auto_shrink([false, false])
                .max_height((ui.available_height() - footer_height).max(row_height * 3.0))
                .show_rows(ui, row_height, panel.entries.len(), |ui, range| {
                    if is_active {
                        let y = ui.min_rect().min.y
                            + cursor.saturating_sub(range.start) as f32 * stride;
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
                            egui::Sense::click_and_drag(),
                        );
                        if entry.is_dir() && !bluepane_engine::is_parent_entry(entry) {
                            zones.push(DropZone {
                                rect,
                                panel: index,
                                row: Some(row + 1),
                            });
                        }
                        if response.drag_started() {
                            drag_started = Some(row);
                        }
                        let is_selected = panel.selected.contains(&entry.path);
                        if row == cursor {
                            // The cursor is a filled, outlined row; marks are text colour.
                            let (fill, outline) = if is_active {
                                (theme.cursor_fill, theme.cursor_stroke)
                            } else {
                                (theme.cursor_fill_inactive, theme.cursor_stroke_inactive)
                            };
                            ui.painter().rect(
                                rect,
                                2.0,
                                fill,
                                egui::Stroke::new(1.0, outline),
                                egui::StrokeKind::Inside,
                            );
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 2.0, theme.hover);
                        } else if row % 2 == 1 {
                            ui.painter().rect_filled(rect, 0.0, theme.row_alt);
                        }
                        // The kind of file gives the icon and, unless the row is marked or hidden,
                        // the colour of the name.
                        let (icon, class_color) = theme.look(bluepane_engine::class_of(entry));
                        let color = if is_selected {
                            theme.selected
                        } else if entry.hidden {
                            theme.muted
                        } else if matches!(
                            bluepane_engine::class_of(entry),
                            bluepane_engine::FileClass::Text | bluepane_engine::FileClass::Other
                        ) {
                            theme.text
                        } else {
                            class_color
                        };
                        let icon_color = if is_selected {
                            theme.selected
                        } else if entry.hidden {
                            theme.muted
                        } else {
                            class_color
                        };
                        let cols = Columns::split(rect, &shown_columns);
                        let (stem, ext) = name_parts(&entry.name, entry.is_dir());
                        let mut name = if entry.is_dir() {
                            format!("[{stem}]")
                        } else {
                            stem.to_string()
                        };
                        // In a list of results the folder is part of what identifies a row.
                        if panel.virtual_list.is_some() && !bluepane_engine::is_parent_entry(entry)
                        {
                            if let Some(dir) = entry.path.parent() {
                                name = format!("{name}    {}", dir.display());
                            }
                        }
                        let known_size = session.entry_size(entry);
                        let size = match (entry.is_dir(), known_size) {
                            (true, None) => "<DIR>".to_string(),
                            (_, Some(bytes)) => human_size(bytes),
                            (false, None) => String::new(),
                        };
                        // A bar behind the size, as long as the size is relative to the biggest.
                        if let Some(bytes) = known_size.filter(|b| {
                            *b > 0
                                && cols.size.width() > 1.0
                                && !bluepane_engine::is_parent_entry(entry)
                        }) {
                            let fraction = (bytes as f64 / biggest as f64).clamp(0.02, 1.0) as f32;
                            let bar = Rect::from_min_max(
                                egui::pos2(
                                    cols.size.right() - cols.size.width() * fraction,
                                    rect.top() + 2.0,
                                ),
                                egui::pos2(cols.size.right(), rect.bottom() - 2.0),
                            );
                            ui.painter().rect_filled(
                                bar,
                                2.0,
                                theme.accent.gamma_multiply(if entry.is_dir() {
                                    0.30
                                } else {
                                    0.18
                                }),
                            );
                        }
                        let painter = ui.painter().clone();
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
                        // Icon in its own narrow column, then the name next to it.
                        painter.with_clip_rect(cols.name).text(
                            egui::pos2(cols.name.left() + COLUMN_PAD, y),
                            Align2::LEFT_CENTER,
                            icon,
                            FontId::proportional(15.0),
                            icon_color,
                        );
                        let name_col = Rect::from_min_max(
                            egui::pos2(cols.name.left() + ICON_WIDTH, cols.name.top()),
                            cols.name.max,
                        );
                        let renaming_here = row == cursor
                            && inline_rename
                                .as_ref()
                                .is_some_and(|(path, _)| *path == entry.path);
                        if renaming_here {
                            let text = rename_text.get_or_insert_with(String::new);
                            let id = egui::Id::new(("inline-rename", index));
                            let edit = egui::TextEdit::singleline(text)
                                .id(id)
                                .font(font.clone())
                                .desired_width(name_col.width());
                            let response = ui.put(name_col.shrink2(egui::vec2(0.0, 1.0)), edit);
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                rename_end = Some(true);
                            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                rename_end = Some(false);
                            } else {
                                response.request_focus();
                            }
                            if first_frame_of_rename {
                                // Everything but the extension is selected, like in a file manager.
                                let (stem, _) = bluepane_engine::name_parts(text, entry.is_dir());
                                let range = egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(stem.chars().count()),
                                );
                                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                                    state.cursor.set_char_range(Some(range));
                                    state.store(ui.ctx(), id);
                                }
                            }
                            rename_drawn = true;
                        } else {
                            cell(name_col, Align2::LEFT_CENTER, name);
                        }
                        cell(cols.ext, Align2::LEFT_CENTER, ext.to_string());
                        cell(cols.size, Align2::RIGHT_CENTER, size);
                        cell(
                            cols.created,
                            Align2::LEFT_CENTER,
                            format_modified(entry.created),
                        );
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
                        if response.secondary_clicked() {
                            right_clicked = Some(row);
                        }
                    }
                });
        }

        ui.separator();
        ui.label(RichText::new(summary).color(theme.muted));

        if clear_filter {
            self.session.active = index;
            let _ = self.session.run_line("filter");
        }
        self.drop_zones.extend(zones);
        if rename_drawn {
            self.rename_focused = true;
            self.inline_rename_drawn = true;
            if let (Some(new_text), Some(Pending::Input { text, .. })) =
                (rename_text, &mut self.session.pending)
            {
                *text = new_text;
            }
            match rename_end {
                Some(true) => self.dispatch("confirm"),
                Some(false) => self.dispatch("cancel"),
                None => {}
            }
        }
        if let Some(row) = drag_started {
            // The dragged row is carried along with the marks; an unmarked row alone.
            self.session.active = index;
            let marked = self.session.panels[index]
                .entries
                .get(row)
                .is_some_and(|e| self.session.panels[index].selected.contains(&e.path));
            if marked {
                self.session.panels[index].cursor = row;
            } else {
                let _ = self
                    .session
                    .execute(Command::Click(row + 1, ClickMode::Plain));
            }
            let targets = self.session.panels[index].target_entries();
            self.drag = Some(DragState {
                from: index,
                count: targets.len(),
                first: targets.first().map(|e| e.name.clone()).unwrap_or_default(),
            });
        }
        let pointer = ui.ctx().pointer_latest_pos().unwrap_or(list_rect.center());
        if let Some(row) = right_clicked {
            // A click on a marked row keeps the marks; on any other row it moves the cursor.
            self.session.active = index;
            let marked = self.session.panels[index]
                .entries
                .get(row)
                .is_some_and(|e| self.session.panels[index].selected.contains(&e.path));
            if marked {
                self.session.panels[index].cursor = row;
            } else {
                let _ = self
                    .session
                    .execute(Command::Click(row + 1, ClickMode::Plain));
            }
            self.open_context_menu(pointer, true);
        } else if backdrop.secondary_clicked() {
            self.session.active = index;
            self.open_context_menu(pointer, false);
        }
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

/// Entries being dragged from a panel.
#[derive(Debug, Clone)]
struct DragState {
    from: usize,
    count: usize,
    first: String,
}

/// A part of the window that takes a drop: a panel's list, or one folder row of it.
#[derive(Debug, Clone, Copy)]
struct DropZone {
    rect: Rect,
    panel: usize,
    row: Option<usize>,
}

/// An open right-click menu.
struct ContextMenu {
    pos: egui::Pos2,
    items: Vec<bluepane_engine::MenuItem>,
    /// The frame it was opened in: the click that opened it must not close it.
    opened_in: u64,
}

const COLUMN_PAD: f32 = 6.0;
/// Room for the file type icon before the name.
const ICON_WIDTH: f32 = 22.0;

/// Horizontal layout of a panel row: a flexible name column and fixed columns on the right.
struct Columns {
    name: Rect,
    ext: Rect,
    size: Rect,
    created: Rect,
    date: Rect,
}

impl Columns {
    const EXT: f32 = 56.0;
    const SIZE: f32 = 80.0;
    const DATE: f32 = 140.0;
    /// The name column never gets narrower than this while another column is shown.
    const NAME_MIN: f32 = 140.0;

    fn width(column: Column) -> f32 {
        match column {
            Column::Ext => Self::EXT,
            Column::Size => Self::SIZE,
            Column::Modified | Column::Created => Self::DATE,
        }
    }

    /// The columns of a row, of those the user chose (`shown`). Size is dropped last, then
    /// the type, then the date, and the creation date goes first: a column that would squeeze
    /// the name below [`Self::NAME_MIN`] is not shown (zero width at the right edge), so a
    /// narrow panel (tools docked beside it) still shows names.
    fn split(row: Rect, shown: &[Column]) -> Columns {
        let cut = |left: f32, right: f32| {
            Rect::from_min_max(egui::pos2(left, row.top()), egui::pos2(right, row.bottom()))
        };
        let mut room = row.width() - Self::NAME_MIN;
        let mut fits = true;
        let mut visible = Vec::new();
        for column in [Column::Size, Column::Ext, Column::Modified, Column::Created] {
            if !shown.contains(&column) {
                continue;
            }
            fits = fits && room >= Self::width(column);
            if fits {
                room -= Self::width(column);
                visible.push(column);
            }
        }
        // Placed from the right edge: date, creation date, size, type; the name takes the rest.
        let mut right = row.right();
        let mut place = |column: Column| {
            if visible.contains(&column) {
                let width = Self::width(column);
                let rect = cut(right - width, right);
                right -= width;
                rect
            } else {
                cut(row.right(), row.right())
            }
        };
        let date = place(Column::Modified);
        let created = place(Column::Created);
        let size = place(Column::Size);
        let ext = place(Column::Ext);
        Columns {
            name: cut(row.left(), right),
            ext,
            size,
            created,
            date,
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
        let c = Columns::split(row, &bluepane_engine::DEFAULT_COLUMNS);
        assert_eq!(c.name.left(), 10.0);
        assert_eq!(c.name.right(), c.ext.left());
        assert_eq!(c.ext.right(), c.size.left());
        assert_eq!(c.size.right(), c.date.left());
        assert_eq!(c.date.right(), 500.0);
    }

    #[test]
    fn a_panel_beside_a_tool_drops_columns_but_keeps_the_name_readable() {
        let columns = |w: f32| {
            Columns::split(
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(w, 20.0)),
                &bluepane_engine::DEFAULT_COLUMNS,
            )
        };
        // Wide: everything.
        let wide = columns(500.0);
        assert!(wide.date.width() > 0.0 && wide.ext.width() > 0.0 && wide.size.width() > 0.0);
        // Narrower: the date goes first, then the type; the size stays longest.
        let medium = columns(340.0);
        assert_eq!(medium.date.width(), 0.0);
        assert!(medium.ext.width() > 0.0 && medium.size.width() > 0.0);
        let narrow = columns(240.0);
        assert_eq!((narrow.date.width(), narrow.ext.width()), (0.0, 0.0));
        assert!(narrow.size.width() > 0.0);
        let tiny = columns(150.0);
        assert_eq!(tiny.size.width(), 0.0);
        assert_eq!(tiny.name.width(), 150.0);
        // Whenever a column is shown the name keeps its minimum width.
        for w in (100..700).step_by(10) {
            let c = columns(w as f32);
            if c.size.width() > 0.0 {
                assert!(
                    c.name.width() >= Columns::NAME_MIN,
                    "{w}: {}",
                    c.name.width()
                );
            }
            assert!(c.name.width() >= 0.0);
        }
    }

    #[test]
    fn narrow_rows_do_not_produce_inverted_columns() {
        let row = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 20.0));
        let c = Columns::split(row, &bluepane_engine::DEFAULT_COLUMNS);
        for rect in [c.name, c.ext, c.size, c.date] {
            assert!(rect.width() >= 0.0);
        }
    }

    #[test]
    fn chosen_columns_are_shown_in_row_order_and_the_creation_date_is_dropped_first() {
        use bluepane_engine::Column;
        let all = [Column::Ext, Column::Size, Column::Created, Column::Modified];
        let row = |w: f32| Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(w, 20.0));
        let wide = Columns::split(row(800.0), &all);
        assert_eq!(wide.name.right(), wide.ext.left());
        assert_eq!(wide.size.right(), wide.created.left());
        assert_eq!(wide.created.right(), wide.date.left());
        assert_eq!(wide.date.right(), 800.0);
        // Not enough room for both dates: the creation date goes, the rest stays.
        let medium = Columns::split(row(500.0), &all);
        assert_eq!(medium.created.width(), 0.0);
        assert!(medium.date.width() > 0.0);
        // Only the name and the size: the size sits at the right edge.
        let few = Columns::split(row(500.0), &[Column::Size]);
        assert_eq!(few.size.right(), 500.0);
        assert_eq!(
            (few.ext.width(), few.date.width(), few.created.width()),
            (0.0, 0.0, 0.0)
        );
        // No columns at all: the name takes the whole row.
        assert_eq!(Columns::split(row(300.0), &[]).name.width(), 300.0);
    }
}
