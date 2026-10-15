//! The synchronization window (Alt+S), drawn in place of the two panels.
//!
//! It only shows the engine's plan and sends commands: `sync` (compare with the settings
//! typed here), `syncset` (override one row) and `syncrun`.

use bluepane_engine::{
    format_modified, human_size, Command, CompareBy, SyncArgs, SyncDeletion, SyncDirection,
    SyncKind, SyncNode, SyncOp, SyncVariant,
};
use egui::{Align2, Color32, FontId, RichText, Sense};

use crate::BluepaneUi;

const ROW_HEIGHT: f32 = 20.0;

/// What the user has typed or picked; sent to the engine only by "Porównaj".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncForm {
    variant: SyncVariant,
    /// 0 = date and size, 1 = size, 2 = content.
    compare: u8,
    /// 0 = trash, 1 = versions folder, 2 = permanent.
    deletion: u8,
    versions: String,
    include: String,
    exclude: String,
}

impl SyncForm {
    fn from_view(view: &bluepane_engine::SyncView) -> SyncForm {
        let (deletion, versions) = match &view.settings.deletion {
            SyncDeletion::Trash => (0, String::new()),
            SyncDeletion::Versioning(dir) => (1, dir.display().to_string()),
            SyncDeletion::Permanent => (2, String::new()),
        };
        SyncForm {
            variant: view.settings.variant,
            compare: match view.settings.compare {
                CompareBy::TimeAndSize { .. } => 0,
                CompareBy::Size => 1,
                CompareBy::Content => 2,
            },
            deletion,
            versions,
            include: view.include.clone(),
            exclude: view.exclude.clone(),
        }
    }

    fn args(&self, tolerance_from: &bluepane_engine::SyncView) -> SyncArgs {
        SyncArgs {
            variant: Some(self.variant),
            compare: Some(match self.compare {
                1 => CompareBy::Size,
                2 => CompareBy::Content,
                // Keep the tolerance that is already in use for dates.
                _ => match tolerance_from.settings.compare {
                    keep @ CompareBy::TimeAndSize { .. } => keep,
                    _ => CompareBy::default(),
                },
            }),
            tolerance: None,
            include: Some(self.include.clone()),
            exclude: Some(self.exclude.clone()),
            deletion: Some(match self.deletion {
                1 => SyncDeletion::Versioning(self.versions.trim().into()),
                2 => SyncDeletion::Permanent,
                _ => SyncDeletion::Trash,
            }),
        }
    }

    fn problem(&self) -> Option<&'static str> {
        (self.deletion == 1 && self.versions.trim().is_empty())
            .then_some("Podaj katalog, do którego trafią poprzednie wersje i usunięte pliki")
    }
}

fn direction_of(op: &SyncOp) -> Option<SyncDirection> {
    match op {
        SyncOp::CopyToRight | SyncOp::DeleteRight => Some(SyncDirection::ToRight),
        SyncOp::CopyToLeft | SyncOp::DeleteLeft => Some(SyncDirection::ToLeft),
        SyncOp::Skip => Some(SyncDirection::Skip),
        SyncOp::Conflict(_) => None,
    }
}

fn next_direction(op: &SyncOp) -> &'static str {
    match direction_of(op) {
        Some(SyncDirection::ToRight) => "left",
        Some(SyncDirection::ToLeft) => "skip",
        _ => "right",
    }
}

fn symbol(op: &SyncOp) -> (&'static str, Color32) {
    match op {
        SyncOp::CopyToRight => ("->", Color32::LIGHT_GREEN),
        SyncOp::CopyToLeft => ("<-", Color32::LIGHT_GREEN),
        SyncOp::DeleteRight => ("x>", Color32::LIGHT_RED),
        SyncOp::DeleteLeft => ("<x", Color32::LIGHT_RED),
        SyncOp::Skip => ("==", Color32::GRAY),
        SyncOp::Conflict(_) => ("!!", Color32::YELLOW),
    }
}

fn describe(node: &Option<SyncNode>) -> String {
    match node {
        None => String::new(),
        Some(n) if n.kind == SyncKind::Dir => "<DIR>".to_string(),
        Some(n) => format!("{:>9}  {}", human_size(n.size), format_modified(n.modified)),
    }
}

impl BluepaneUi {
    /// True while the window has unsent changes (then "Synchronizuj" must wait for "Porównaj").
    pub(crate) fn sync_form_is_dirty(&self) -> bool {
        match (&self.session.sync, &self.sync_form) {
            (Some(view), Some(form)) => *form != SyncForm::from_view(view),
            _ => false,
        }
    }

    /// F9 in the window: compare again with the settings as they are on screen.
    pub(crate) fn sync_compare_now(&mut self) {
        let (Some(view), Some(form)) = (&self.session.sync, &self.sync_form) else {
            let _ = self.session.run_line("sync");
            return;
        };
        if form.problem().is_none() {
            let args = form.args(view);
            let _ = self.session.execute(Command::Sync(args));
        }
    }

    pub(crate) fn sync_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.session.sync.clone() else {
            self.sync_form = None;
            return;
        };
        let mut form = self
            .sync_form
            .take()
            .unwrap_or_else(|| SyncForm::from_view(&view));
        let dirty = form != SyncForm::from_view(&view);
        let busy = self.session.is_busy();
        let mut commands: Vec<String> = Vec::new();
        let mut compare = false;

        ui.label(
            RichText::new(format!(
                "Synchronizacja:  {}   <=>   {}",
                view.left.display(),
                view.right.display()
            ))
            .strong(),
        );
        ui.horizontal(|ui| {
            ui.label("Wariant:");
            for (variant, label) in [
                (SyncVariant::TwoWay, "Dwukierunkowa"),
                (SyncVariant::Mirror, "Lustro (lewy -> prawy)"),
                (SyncVariant::Update, "Aktualizacja (lewy -> prawy)"),
            ] {
                ui.selectable_value(&mut form.variant, variant, label);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Porównanie:");
            for (kind, label) in [(0, "Data i rozmiar"), (1, "Rozmiar"), (2, "Zawartość")] {
                ui.selectable_value(&mut form.compare, kind, label);
            }
            ui.separator();
            ui.label("Usunięte i zastąpione pliki:");
            for (kind, label) in [(0, "do kosza"), (1, "do katalogu wersji"), (2, "trwale")] {
                ui.selectable_value(&mut form.deletion, kind, label);
            }
        });
        if form.deletion == 1 {
            ui.horizontal(|ui| {
                ui.label("Katalog wersji:");
                ui.add(egui::TextEdit::singleline(&mut form.versions).desired_width(360.0));
            });
        }
        if form.deletion == 2 {
            ui.colored_label(
                Color32::LIGHT_RED,
                "Trwale: usunięte i nadpisane pliki znikną bez możliwości odzyskania.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Tylko:");
            ui.add(
                egui::TextEdit::singleline(&mut form.include)
                    .hint_text("np. *.rs;docs/ (puste = wszystko)")
                    .desired_width(220.0),
            );
            ui.label("Pomiń:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut form.exclude)
                    .hint_text("np. *.tmp;node_modules/")
                    .desired_width(220.0),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                compare = true;
            }
        });
        let problem = form.problem();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    problem.is_none() && !busy,
                    egui::Button::new("Porównaj (F9)"),
                )
                .clicked()
            {
                compare = true;
            }
            let runnable =
                view.preview.as_ref().is_some_and(|p| !p.plan.is_noop()) && !dirty && !busy;
            if ui
                .add_enabled(runnable, egui::Button::new("Synchronizuj (F5)"))
                .clicked()
            {
                commands.push("syncrun".into());
            }
            if busy && ui.button("Zatrzymaj").clicked() {
                commands.push("stop".into());
            }
            if ui.button("Zamknij (Esc)").clicked() {
                commands.push("syncclose".into());
            }
            if dirty {
                ui.colored_label(
                    Color32::YELLOW,
                    "Zmieniono ustawienia: kliknij „Porównaj”, żeby je zastosować",
                );
            }
        });
        if let Some(problem) = problem {
            ui.colored_label(Color32::LIGHT_RED, problem);
        }

        match &view.preview {
            None => {
                ui.add_space(8.0);
                ui.label("Porównywanie…");
            }
            Some(preview) => {
                let plan = &preview.plan;
                ui.label(
                    RichText::new(format!(
                        "Do zrobienia: kopiowanie {}, usuwanie {}, konflikty {} · do skopiowania {}",
                        plan.copies(),
                        plan.deletions(),
                        plan.conflicts(),
                        human_size(plan.bytes())
                    ))
                    .strong(),
                );
                if form.variant == SyncVariant::TwoWay && !preview.used_snapshot {
                    ui.label(
                        RichText::new(
                            "Pierwsza synchronizacja tej pary folderów: usunięć jeszcze nie ma jak wykryć, więc nic nie zostanie usunięte.",
                        )
                        .weak(),
                    );
                }
                if let Some(summary) = &view.summary {
                    ui.label(RichText::new(summary).weak());
                }
                ui.label(
                    RichText::new(
                        "-> kopiuj w prawo · <- kopiuj w lewo · x> / <x usuń po prawej / lewej · == pomiń · !! konflikt (nic nie zrobi się samo). Klik w symbol zmienia kierunek.",
                    )
                    .weak(),
                );
                ui.separator();
                self.sync_rows(ui, preview, &mut commands);
            }
        }

        self.sync_form = Some(form.clone());
        if compare && form.problem().is_none() {
            let args = form.args(&view);
            let _ = self.session.execute(Command::Sync(args));
        }
        for line in commands {
            let _ = self.session.run_line(&line);
        }
    }

    fn sync_rows(
        &self,
        ui: &mut egui::Ui,
        preview: &bluepane_engine::SyncPreview,
        commands: &mut Vec<String>,
    ) {
        let actions = &preview.plan.actions;
        let font = FontId::monospace(13.0);
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            ROW_HEIGHT,
            actions.len(),
            |ui, range| {
                for index in range {
                    let action = &actions[index];
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        Sense::click(),
                    );
                    let w = rect.width();
                    let at =
                        |fraction: f32| egui::pos2(rect.left() + w * fraction, rect.center().y);
                    let painter = ui.painter();
                    let weak = ui.visuals().weak_text_color();
                    let text = ui.visuals().text_color();
                    painter.text(
                        at(0.0),
                        Align2::LEFT_CENTER,
                        format!("{}", index + 1),
                        font.clone(),
                        weak,
                    );
                    painter.text(
                        at(0.05),
                        Align2::LEFT_CENTER,
                        &action.path,
                        font.clone(),
                        text,
                    );
                    painter.text(
                        at(0.42),
                        Align2::LEFT_CENTER,
                        describe(&action.left),
                        font.clone(),
                        text,
                    );
                    let (label, color) = symbol(&action.op);
                    painter.text(at(0.72), Align2::LEFT_CENTER, label, font.clone(), color);
                    painter.text(
                        at(0.77),
                        Align2::LEFT_CENTER,
                        describe(&action.right),
                        font.clone(),
                        text,
                    );
                    if let SyncOp::Conflict(reason) = &action.op {
                        response.clone().on_hover_text(reason.as_str());
                    }
                    if response.clicked() {
                        commands.push(format!(
                            "syncset {} {}",
                            index + 1,
                            next_direction(&action.op)
                        ));
                    }
                }
            },
        );
    }
}
