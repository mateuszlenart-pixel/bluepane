use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use bluepane_ops::{self as ops, Conflict, Resolution};
use bluepane_vfs::Volume;

use crate::job::{Answer, Event, JobKind, JobSpec, Report, RunningJob};
use crate::{
    is_parent_entry, Command, EngineError, Keymap, Mask, Panel, TabAction, Viewer, ViewerAction,
};

/// What a name dialog is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// Name of a new directory in the active panel.
    Mkdir,
    /// New name for this entry.
    Rename(PathBuf),
    /// Text to search for in the open viewer.
    ViewerFind,
}

/// A file conflict the running job is waiting on.
#[derive(Debug, Clone)]
pub struct ConflictPrompt {
    pub conflict: Conflict,
    reply: Sender<Answer>,
}

impl ConflictPrompt {
    fn answer(&self, resolution: Option<Resolution>, for_all: bool) {
        // The worker may already be gone (cancelled elsewhere); nothing to do then.
        let _ = self.reply.send(Answer {
            resolution,
            for_all,
        });
    }
}

/// A question waiting for the user's answer. `confirm` accepts it and `cancel` rejects it,
/// except for conflicts, which are answered with `overwrite`, `newer`, `skip`, `keepboth`
/// (each also as `...-all`) or `cancel` (stops the job).
#[derive(Debug, Clone)]
pub enum Pending {
    /// Both panels show the same directory: make copies of these paths next to the originals.
    Duplicate(Vec<PathBuf>),
    /// Move these paths to the trash.
    ConfirmDelete(Vec<PathBuf>),
    /// A name is being typed; the UI edits `text` and `confirm` uses it.
    Input { kind: InputKind, text: String },
    /// An existing file stands in the way of the running copy or move.
    Conflict(ConflictPrompt),
}

/// What the progress bar shows about the running job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStatus {
    pub label: &'static str,
    pub current: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub items_done: u64,
    pub items_total: u64,
    /// Jobs waiting behind this one.
    pub queued: usize,
    /// The job is blocked on a conflict question.
    pub waiting_for_answer: bool,
}

impl JobStatus {
    /// Completed share between 0 and 1, or `None` while the total is still being counted.
    pub fn fraction(&self) -> Option<f32> {
        let (done, total) = if self.bytes_total > 0 {
            (self.bytes_done, self.bytes_total)
        } else if self.items_total > 0 {
            (self.items_done, self.items_total)
        } else {
            return None;
        };
        Some((done as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }
}

/// The tabs of one side other than the active one. Together with the active panel they
/// form the tab row: `others` in order with the active panel inserted at `position`.
#[derive(Debug, Default)]
struct ParkedTabs {
    others: Vec<Panel>,
    position: usize,
}

/// What the UI needs to draw one side's tab row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStrip {
    pub titles: Vec<String>,
    /// Full directory of each tab, for tooltips.
    pub paths: Vec<PathBuf>,
    /// Index of the active tab in `titles`.
    pub active: usize,
}

/// Resolves `.` and `..` components lexically, like `cd` in a shell (symlinks are not
/// followed), so `a/b/..` becomes `a` and the panel never shows a path containing `..`.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never climb above the root (or a Windows drive prefix).
                if !matches!(
                    out.components().next_back(),
                    Some(Component::RootDir | Component::Prefix(_)) | None
                ) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Short label for a tab: the directory name, or the whole path for a root.
fn tab_title(cwd: &Path) -> String {
    cwd.file_name().map_or_else(
        || cwd.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The whole application state except the UI: two panels, which one is active,
/// the key map and the last status message.
#[derive(Debug)]
pub struct Session {
    /// The active tab of each side.
    pub panels: [Panel; 2],
    pub active: usize,
    pub keymap: Keymap,
    pub status: String,
    /// True when `status` reports a problem (the UI shows it in red).
    pub status_is_error: bool,
    /// Mounted volumes, re-read on `refresh`.
    pub volumes: Vec<Volume>,
    /// Set while the user is asked a question; the question text is in `status`.
    pub pending: Option<Pending>,
    /// Answer conflicts automatically instead of asking (scripts and the CLI).
    pub auto_resolve: Option<Resolution>,
    /// The open file viewer (F3); while it is set the UI shows it instead of the panels.
    pub viewer: Option<Viewer>,
    parked: [ParkedTabs; 2],
    current: Option<RunningJob>,
    queue: VecDeque<JobSpec>,
}

/// Index of the volume that contains `cwd` (the one with the longest matching path).
fn volume_index(volumes: &[Volume], cwd: &Path) -> Option<usize> {
    volumes
        .iter()
        .enumerate()
        .filter(|(_, v)| cwd.starts_with(&v.path))
        .max_by_key(|(_, v)| v.path.components().count())
        .map(|(i, _)| i)
}

impl Session {
    pub fn new(left: impl Into<PathBuf>, right: impl Into<PathBuf>) -> Self {
        Session {
            panels: [Panel::new(left), Panel::new(right)],
            active: 0,
            keymap: Keymap::default(),
            status: String::new(),
            status_is_error: false,
            volumes: bluepane_vfs::volumes(),
            pending: None,
            auto_resolve: None,
            viewer: None,
            parked: [ParkedTabs::default(), ParkedTabs::default()],
            current: None,
            queue: VecDeque::new(),
        }
    }

    /// Index into `volumes` of the volume the given panel is on.
    pub fn volume_of(&self, panel: usize) -> Option<usize> {
        volume_index(&self.volumes, &self.panels[panel].cwd)
    }

    pub fn active_panel(&self) -> &Panel {
        &self.panels[self.active]
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        &mut self.panels[self.active]
    }

    /// Parses and executes a command line. On error the message is also put in `status`.
    pub fn run_line(&mut self, line: &str) -> Result<(), EngineError> {
        match Command::parse(line) {
            Ok(command) => self.execute(command),
            Err(err) => {
                self.status = err.0.clone();
                self.status_is_error = true;
                Err(err)
            }
        }
    }

    /// Executes a command. Long operations are queued as background jobs: call [`poll`]
    /// regularly (every frame) to collect their progress and results.
    ///
    /// [`poll`]: Session::poll
    pub fn execute(&mut self, command: Command) -> Result<(), EngineError> {
        let result = self.execute_inner(command);
        match &result {
            Ok(()) => self.status_is_error = false,
            Err(err) => {
                self.status = err.0.clone();
                self.status_is_error = true;
            }
        }
        result
    }

    fn execute_inner(&mut self, command: Command) -> Result<(), EngineError> {
        // Any other command drops an unanswered question, except a conflict: the worker
        // thread is blocked on it and only an answer (or `stop`) may release it.
        let answers = matches!(
            command,
            Command::Confirm | Command::Cancel | Command::Resolve(..) | Command::StopJob
        );
        if !answers && !matches!(self.pending, Some(Pending::Conflict(_))) {
            self.pending = None;
        }
        match command {
            Command::Confirm => self.confirm()?,
            Command::Cancel => self.cancel_question(),
            Command::Resolve(resolution, for_all) => self.resolve(resolution, for_all)?,
            Command::StopJob => self.stop_jobs(),
            Command::Tab(action) => self.tab_action(action)?,
            Command::View => self.open_viewer()?,
            Command::Edit => self.edit_file()?,
            Command::Viewer(action) => self.viewer_action(action)?,
            Command::SwitchPanel => self.active = 1 - self.active,
            Command::Up => {
                let panel = self.active_panel_mut();
                if !panel.filter_backspace() {
                    panel.up();
                }
            }
            Command::Enter => self.active_panel_mut().enter(),
            Command::Cursor(delta) => self.active_panel_mut().move_cursor(delta),
            Command::ToggleSelect => self.active_panel_mut().toggle_select(),
            Command::Refresh => {
                self.volumes = bluepane_vfs::volumes();
                self.panels.iter_mut().for_each(Panel::reload);
            }
            Command::Back => self.active_panel_mut().back()?,
            Command::Forward => self.active_panel_mut().forward()?,
            Command::SelectMask(text) => self.select_by_mask(&text, true)?,
            Command::UnselectMask(text) => self.select_by_mask(&text, false)?,
            Command::SelectAll => {
                let count = self.active_panel_mut().select_all();
                self.status = format!("Zaznaczono: {count}");
            }
            Command::UnselectAll => {
                self.active_panel_mut().unselect_all();
                self.status = "Odznaczono wszystko".into();
            }
            Command::InvertSelection => {
                self.active_panel_mut().invert_selection();
                self.status = format!("Zaznaczono: {}", self.active_panel().selected.len());
            }
            Command::Sort(key, descending) => self.active_panel_mut().set_sort(key, descending),
            Command::Filter(text) => self.active_panel_mut().set_filter(&text),
            Command::Volume(which) => self.go_to_volume(&which)?,
            Command::Select(name) => {
                let panel = self.active_panel_mut();
                let Some(index) = panel
                    .entries
                    .iter()
                    .position(|e| e.name == name && !is_parent_entry(e))
                else {
                    return Err(EngineError(format!("brak elementu: {name}")));
                };
                panel.cursor = index;
                panel.selected.insert(panel.entries[index].path.clone());
            }
            Command::Cd(path) => {
                let panel = self.active_panel_mut();
                let target = normalize_path(&if path.is_absolute() {
                    path
                } else {
                    panel.cwd.join(path)
                });
                if !target.is_dir() {
                    return Err(EngineError(format!("to nie katalog: {}", target.display())));
                }
                panel.change_dir(target);
            }
            Command::Copy => self.transfer(false)?,
            Command::Move => self.transfer(true)?,
            Command::Mkdir(Some(name)) => self.make_dir(&name)?,
            Command::Mkdir(None) => {
                self.status = "Nazwa nowego katalogu (Enter = utwórz, Esc = anuluj)".into();
                self.pending = Some(Pending::Input {
                    kind: InputKind::Mkdir,
                    text: String::new(),
                });
            }
            Command::Rename(Some(name)) => {
                let path = self.entry_to_rename()?;
                self.rename_entry(&path, &name)?;
            }
            Command::Rename(None) => {
                let path = self.entry_to_rename()?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.status = "Nowa nazwa (Enter = zmień, Esc = anuluj)".into();
                self.pending = Some(Pending::Input {
                    kind: InputKind::Rename(path),
                    text: name,
                });
            }
            Command::Delete => self.propose_delete()?,
        }
        Ok(())
    }

    fn select_by_mask(&mut self, text: &str, select: bool) -> Result<(), EngineError> {
        let mask = Mask::parse(text).ok_or_else(|| EngineError("pusta maska".into()))?;
        let count = self.active_panel_mut().select_matching(&mask, select);
        let verb = if select { "Zaznaczono" } else { "Odznaczono" };
        self.status = format!("{verb}: {count} ({text})");
        Ok(())
    }

    /// Jumps the active panel to a volume given as a 1-based number or a name.
    fn go_to_volume(&mut self, which: &str) -> Result<(), EngineError> {
        let found = match which.parse::<usize>() {
            Ok(n) => n.checked_sub(1).and_then(|i| self.volumes.get(i)),
            Err(_) => self
                .volumes
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(which) || v.path == Path::new(which)),
        };
        let Some(volume) = found else {
            let known: Vec<String> = self
                .volumes
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{} {}", i + 1, v.name))
                .collect();
            return Err(EngineError(format!(
                "nie ma wolumenu: {which} (dostępne: {})",
                known.join(", ")
            )));
        };
        let path = volume.path.clone();
        if !path.is_dir() {
            return Err(EngineError(format!(
                "wolumen niedostępny: {}",
                path.display()
            )));
        }
        self.active_panel_mut().change_dir(path);
        Ok(())
    }

    /// Copy or move the active panel's targets into the other panel's directory.
    fn transfer(&mut self, remove_source: bool) -> Result<(), EngineError> {
        let source = self.active_panel();
        let dest = self.panels[1 - self.active].cwd.clone();
        let targets = source.targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        if dest == source.cwd {
            if remove_source {
                return Err(EngineError("oba panele wskazują ten sam katalog".into()));
            }
            return self.propose_duplicates(targets);
        }
        let kind = if remove_source {
            JobKind::Move
        } else {
            JobKind::Copy
        };
        self.enqueue(JobSpec {
            kind,
            sources: targets,
            dest: Some(dest),
        });
        Ok(())
    }

    /// F5 with the same directory in both panels: ask before making `(kopia)` duplicates.
    fn propose_duplicates(&mut self, targets: Vec<PathBuf>) -> Result<(), EngineError> {
        let first = ops::duplicate_path(&targets[0]).map_err(|e| EngineError(e.to_string()))?;
        let first_name = first
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = if targets.len() == 1 {
            format!("Oba panele pokazują ten sam katalog. Utworzyć kopię „{first_name}”? (Enter = tak, Esc = nie)")
        } else {
            format!(
                "Oba panele pokazują ten sam katalog. Utworzyć kopie ({}) z dopiskiem „(kopia)”, np. „{first_name}”? (Enter = tak, Esc = nie)",
                targets.len()
            )
        };
        self.pending = Some(Pending::Duplicate(targets));
        Ok(())
    }

    /// F8: ask first, then move to the trash.
    fn propose_delete(&mut self) -> Result<(), EngineError> {
        let targets = self.active_panel().targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        self.status = if let [only] = targets.as_slice() {
            let name = only
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("Przenieść do kosza „{name}”? (Enter = tak, Esc = nie)")
        } else {
            format!(
                "Przenieść do kosza zaznaczone elementy ({})? (Enter = tak, Esc = nie)",
                targets.len()
            )
        };
        self.pending = Some(Pending::ConfirmDelete(targets));
        Ok(())
    }

    /// Accepts the open question (`confirm`, Enter).
    fn confirm(&mut self) -> Result<(), EngineError> {
        match self.pending.take() {
            Some(Pending::Duplicate(sources)) => self.enqueue(JobSpec {
                kind: JobKind::Duplicate,
                sources,
                dest: None,
            }),
            Some(Pending::ConfirmDelete(sources)) => self.enqueue(JobSpec {
                kind: JobKind::Delete,
                sources,
                dest: None,
            }),
            Some(Pending::Input { kind, text }) => {
                let result = match &kind {
                    InputKind::Mkdir => self.make_dir(&text),
                    InputKind::Rename(path) => self.rename_entry(path, &text),
                    InputKind::ViewerFind => {
                        self.viewer_action(ViewerAction::Find(Some(text.clone())))
                    }
                };
                if let Err(err) = result {
                    // Keep the dialog open so the name can be corrected.
                    self.pending = Some(Pending::Input { kind, text });
                    return Err(err);
                }
            }
            Some(conflict @ Pending::Conflict(_)) => {
                self.pending = Some(conflict);
                return Err(EngineError(
                    "plik już istnieje: wybierz nadpisz, nadpisz starsze, pomiń, zachowaj oba lub anuluj"
                        .into(),
                ));
            }
            None => return Err(EngineError("nie ma o co pytać".into())),
        }
        Ok(())
    }

    /// Rejects the open question (`cancel`, Escape). For a conflict this stops the job.
    fn cancel_question(&mut self) {
        match self.pending.take() {
            Some(Pending::Conflict(prompt)) => {
                prompt.answer(None, false);
                self.queue.clear();
                self.status = "Anulowanie zadania…".into();
            }
            Some(_) => self.status = "Anulowano".into(),
            None => {}
        }
    }

    fn resolve(&mut self, resolution: Resolution, for_all: bool) -> Result<(), EngineError> {
        match self.pending.take() {
            Some(Pending::Conflict(prompt)) => {
                prompt.answer(Some(resolution), for_all);
                self.status = format!("{}…", self.current_label());
                Ok(())
            }
            other => {
                self.pending = other;
                Err(EngineError("nie ma konfliktu do rozstrzygnięcia".into()))
            }
        }
    }

    fn current_label(&self) -> &'static str {
        self.current
            .as_ref()
            .map_or("Gotowe", |job| job.spec.kind.label())
    }

    fn stop_jobs(&mut self) {
        self.queue.clear();
        let Some(job) = &self.current else {
            self.status = "Nic się nie wykonuje".into();
            return;
        };
        job.progress.cancel();
        if let Some(Pending::Conflict(prompt)) = self.pending.take() {
            prompt.answer(None, false);
        }
        self.status = "Zatrzymywanie…".into();
    }

    /// Runs the job now, or queues it behind the running one.
    fn enqueue(&mut self, spec: JobSpec) {
        self.active_panel_mut().selected.clear();
        if self.current.is_none() {
            self.start(spec);
        } else {
            self.queue.push_back(spec);
            self.status = format!("W kolejce: {}", self.queue.len());
        }
    }

    fn start(&mut self, spec: JobSpec) {
        self.status = format!("{}…", spec.kind.label());
        self.status_is_error = false;
        self.current = Some(RunningJob::start(spec));
    }

    /// True while a job runs or waits in the queue.
    pub fn is_busy(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    /// Progress of the running job, for the progress bar.
    pub fn job_status(&self) -> Option<JobStatus> {
        let job = self.current.as_ref()?;
        let progress = &job.progress;
        Some(JobStatus {
            label: job.spec.kind.label(),
            current: progress.current(),
            bytes_done: progress.bytes_done(),
            bytes_total: progress.bytes_total(),
            items_done: progress.items_done(),
            items_total: progress.items_total(),
            queued: self.queue.len(),
            waiting_for_answer: matches!(self.pending, Some(Pending::Conflict(_))),
        })
    }

    /// Collects what the background job reported since the last call: conflicts to ask
    /// about and the final result. Cheap; call it every frame.
    pub fn poll(&mut self) {
        loop {
            let Some(job) = &self.current else { return };
            match job.try_event() {
                Ok(None) => return,
                Ok(Some(Event::Conflict { conflict, reply })) => {
                    let prompt = ConflictPrompt { conflict, reply };
                    if let Some(resolution) = self.auto_resolve {
                        prompt.answer(Some(resolution), true);
                        continue;
                    }
                    let name = prompt
                        .conflict
                        .dst
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.status = format!("Plik już istnieje: {name}");
                    self.status_is_error = false;
                    self.pending = Some(Pending::Conflict(prompt));
                    return;
                }
                Ok(Some(Event::Finished(report))) => self.finish_job(&report),
                Err(_) => self.finish_job(&Report {
                    failed: vec!["zadanie przerwane nieoczekiwanie".into()],
                    ..Report::default()
                }),
            }
        }
    }

    fn finish_job(&mut self, report: &Report) {
        let Some(mut job) = self.current.take() else {
            return;
        };
        job.join();
        let (kind, total) = (job.spec.kind, job.spec.sources.len());
        self.pending = None;
        self.panels.iter_mut().for_each(Panel::reload);

        let verb = kind.verb();
        let mut text = match report.failed.first() {
            Some(first) => format!("{verb}: {} z {total}, błąd: {first}", report.done),
            None => format!("{verb}: {}", report.done),
        };
        if report.skipped > 0 {
            text.push_str(&format!(", pominięto: {}", report.skipped));
        }
        if report.cancelled {
            text = format!("Anulowano. {text}");
        }
        self.status = text;
        self.status_is_error = !report.failed.is_empty();

        if let Some(next) = self.queue.pop_front() {
            let message = std::mem::take(&mut self.status);
            let was_error = self.status_is_error;
            self.start(next);
            // Keep the result of the finished job visible until the next one says something.
            if was_error {
                self.status = message;
                self.status_is_error = true;
            }
        }
    }

    /// Blocks until every job has finished. Returns false when it stopped because a
    /// conflict needs an answer (answer it, then call this again). For scripts and tests.
    pub fn wait_idle(&mut self) -> bool {
        loop {
            self.poll();
            if !self.is_busy() {
                return true;
            }
            if matches!(self.pending, Some(Pending::Conflict(_))) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The tab row of one side, for drawing.
    pub fn tab_strip(&self, side: usize) -> TabStrip {
        let parked = &self.parked[side];
        let mut paths: Vec<PathBuf> = parked.others.iter().map(|p| p.cwd.clone()).collect();
        paths.insert(parked.position, self.panels[side].cwd.clone());
        TabStrip {
            titles: paths.iter().map(|p| tab_title(p)).collect(),
            paths,
            active: parked.position,
        }
    }

    fn tab_count(&self, side: usize) -> usize {
        self.parked[side].others.len() + 1
    }

    fn tab_action(&mut self, action: TabAction) -> Result<(), EngineError> {
        let side = self.active;
        let count = self.tab_count(side);
        let position = self.parked[side].position;
        match action {
            TabAction::New => {
                let cwd = self.panels[side].cwd.clone();
                self.open_tab(side, cwd);
            }
            TabAction::Open => {
                let entry = self
                    .active_panel()
                    .current()
                    .filter(|e| !is_parent_entry(e) && e.is_dir())
                    .ok_or_else(|| EngineError("kursor nie wskazuje katalogu".into()))?;
                let path = entry.path.clone();
                self.open_tab(side, path);
            }
            TabAction::Close(which) => {
                let index = which.map_or(position, |n| n - 1);
                self.close_tab(side, index)?;
            }
            TabAction::Next => self.switch_tab(side, (position + 1) % count),
            TabAction::Prev => self.switch_tab(side, (position + count - 1) % count),
            TabAction::Goto(n) => {
                if n > count {
                    return Err(EngineError(format!("nie ma zakładki {n} (jest {count})")));
                }
                self.switch_tab(side, n - 1);
            }
        }
        let strip = self.tab_strip(side);
        self.status = format!("Zakładka {} z {}", strip.active + 1, strip.titles.len());
        Ok(())
    }

    /// Makes tab `index` (0-based, in tab-row order) the active one of `side`.
    fn switch_tab(&mut self, side: usize, index: usize) {
        let parked = &mut self.parked[side];
        if index == parked.position {
            return;
        }
        let current = std::mem::replace(&mut self.panels[side], Panel::blank());
        parked.others.insert(parked.position, current);
        self.panels[side] = parked.others.remove(index);
        parked.position = index;
        // The directory may have changed while the tab was in the background.
        self.panels[side].reload();
    }

    /// Opens a new tab for `cwd` right after the active one and makes it active.
    fn open_tab(&mut self, side: usize, cwd: PathBuf) {
        let parked = &mut self.parked[side];
        let current = std::mem::replace(&mut self.panels[side], Panel::new(cwd));
        parked.others.insert(parked.position, current);
        parked.position += 1;
    }

    fn close_tab(&mut self, side: usize, index: usize) -> Result<(), EngineError> {
        let count = self.tab_count(side);
        if count == 1 {
            return Err(EngineError("to jedyna zakładka".into()));
        }
        if index >= count {
            return Err(EngineError(format!(
                "nie ma zakładki {} (jest {count})",
                index + 1
            )));
        }
        let parked = &mut self.parked[side];
        if index == parked.position {
            // Closing the active tab activates its right neighbour, or the left one at the end.
            let take = if index < parked.others.len() {
                index
            } else {
                index - 1
            };
            self.panels[side] = parked.others.remove(take);
            parked.position = take;
            self.panels[side].reload();
        } else if index < parked.position {
            parked.others.remove(index);
            parked.position -= 1;
        } else {
            parked.others.remove(index - 1);
        }
        Ok(())
    }

    /// The file under the cursor, for view and edit: never `..`, never a directory.
    fn entry_to_open(&self) -> Result<PathBuf, EngineError> {
        let entry = self
            .active_panel()
            .current()
            .filter(|e| !is_parent_entry(e))
            .ok_or_else(|| EngineError("nic nie wskazano".into()))?;
        if entry.is_dir() {
            return Err(EngineError(format!("to katalog, nie plik: {}", entry.name)));
        }
        Ok(entry.path.clone())
    }

    /// F3: opens the built-in viewer on the file under the cursor.
    fn open_viewer(&mut self) -> Result<(), EngineError> {
        let path = self.entry_to_open()?;
        let viewer = Viewer::open(&path)?;
        self.status = format!(
            "Podgląd: {}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        self.viewer = Some(viewer);
        Ok(())
    }

    /// F4: opens the file in `BLUEPANE_EDITOR`, or the system's default application.
    fn edit_file(&mut self) -> Result<(), EngineError> {
        let path = self.entry_to_open()?;
        let editor = std::env::var("BLUEPANE_EDITOR").ok();
        bluepane_vfs::open_external(&path, editor.as_deref())
            .map_err(|e| EngineError(format!("nie udało się otworzyć edytora: {e}")))?;
        self.status = format!(
            "Otwarto w edytorze: {}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        Ok(())
    }

    fn viewer_action(&mut self, action: ViewerAction) -> Result<(), EngineError> {
        let Some(viewer) = self.viewer.as_mut() else {
            return Err(EngineError("podgląd nie jest otwarty (F3)".into()));
        };
        match action {
            ViewerAction::Scroll(rows) => viewer.scroll(rows),
            ViewerAction::Page(pages) => viewer.page(pages),
            ViewerAction::HScroll(columns) => viewer.scroll_columns(columns),
            ViewerAction::Top => viewer.top(),
            ViewerAction::Bottom => viewer.bottom(),
            ViewerAction::Mode => viewer.toggle_mode(),
            ViewerAction::Goto(spec) => viewer.goto(&spec)?,
            ViewerAction::Find(Some(text)) => {
                viewer.find(&text, true);
                self.status = viewer.message.clone();
            }
            ViewerAction::Find(None) => {
                let text = viewer.last_search().unwrap_or_default().to_string();
                self.status = "Szukaj w pliku (Enter = szukaj, Esc = anuluj)".into();
                self.pending = Some(Pending::Input {
                    kind: InputKind::ViewerFind,
                    text,
                });
            }
            ViewerAction::Next | ViewerAction::Prev => {
                viewer.find_again(action == ViewerAction::Next);
                self.status = viewer.message.clone();
            }
            ViewerAction::Close => self.viewer = None,
        }
        Ok(())
    }

    fn make_dir(&mut self, name: &str) -> Result<(), EngineError> {
        let cwd = self.active_panel().cwd.clone();
        let created = ops::mkdir(&cwd, name).map_err(|e| EngineError(e.to_string()))?;
        self.panels.iter_mut().for_each(Panel::reload);
        self.active_panel_mut().put_cursor_on(&created);
        self.status = format!("Utworzono katalog {}", name.trim());
        Ok(())
    }

    /// The entry a rename acts on: the one under the cursor, never `..`.
    fn entry_to_rename(&self) -> Result<PathBuf, EngineError> {
        self.active_panel()
            .current()
            .filter(|e| !is_parent_entry(e))
            .map(|e| e.path.clone())
            .ok_or_else(|| EngineError("nic nie wskazano".into()))
    }

    fn rename_entry(&mut self, path: &Path, new_name: &str) -> Result<(), EngineError> {
        let renamed = ops::rename(path, new_name).map_err(|e| EngineError(e.to_string()))?;
        self.panels.iter_mut().for_each(Panel::reload);
        self.active_panel_mut().put_cursor_on(&renamed);
        self.status = format!("Zmieniono nazwę na {}", new_name.trim());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_dirs() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir(&left).unwrap();
        std::fs::create_dir(&right).unwrap();
        std::fs::write(left.join("a.txt"), "a").unwrap();
        std::fs::write(left.join("b.txt"), "b").unwrap();
        let session = Session::new(left, right);
        (tmp, session)
    }

    #[test]
    fn copy_goes_to_the_other_panel() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("left/a.txt").exists());
        assert_eq!(s.panels[1].entries.len(), 2); // ".." and a.txt
        assert!(s.panels[0].selected.is_empty());
    }

    #[test]
    fn move_uses_cursor_when_nothing_is_selected() {
        let (tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("move").unwrap();
        assert!(s.wait_idle());
        assert!(!tmp.path().join("left/a.txt").exists());
        assert!(tmp.path().join("right/a.txt").exists());
    }

    /// Both panels on `left`, cursor on a.txt.
    fn same_dir() -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = two_dirs();
        s.run_line("switch").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        s.run_line("cursor 1").unwrap();
        (tmp, s)
    }

    #[test]
    fn move_in_the_same_directory_is_rejected() {
        let (_tmp, mut s) = same_dir();
        assert!(s.run_line("move").is_err());
        assert!(s.status.contains("ten sam katalog"));
        assert!(s.pending.is_none());
    }

    #[test]
    fn copy_in_the_same_directory_asks_before_duplicating() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_some());
        assert!(s.status.contains("a (kopia).txt"), "{}", s.status);
        // Nothing is created until the user answers.
        assert!(!tmp.path().join("left/a (kopia).txt").exists());

        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a (kopia).txt")).unwrap(),
            "a"
        );
        assert!(tmp.path().join("left/a.txt").exists());
        // Both panels were reloaded.
        assert!(s.panels[0]
            .entries
            .iter()
            .any(|e| e.name == "a (kopia).txt"));
        assert!(s.panels[1]
            .entries
            .iter()
            .any(|e| e.name == "a (kopia).txt"));
    }

    #[test]
    fn declining_creates_nothing() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert_eq!(s.status, "Anulowano");
        assert!(!tmp.path().join("left/a (kopia).txt").exists());
        assert!(s.run_line("confirm").is_err());
    }

    #[test]
    fn any_other_command_drops_the_question() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        s.run_line("cursor 1").unwrap();
        assert!(s.pending.is_none());
        assert!(s.run_line("confirm").is_err());
        assert!(!tmp.path().join("left/a (kopia).txt").exists());
    }

    #[test]
    fn duplicates_every_selected_entry_and_never_overwrites() {
        let (tmp, mut s) = same_dir();
        std::fs::write(tmp.path().join("left/a (kopia).txt"), "old").unwrap();
        s.run_line("cursor -1").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        s.run_line("confirm").unwrap();
        assert!(s.wait_idle());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a (kopia).txt")).unwrap(),
            "old"
        );
        assert!(tmp.path().join("left/a (kopia 2).txt").exists());
        assert!(tmp.path().join("left/b (kopia).txt").exists());
    }

    #[test]
    fn copy_with_nothing_to_copy_does_not_ask() {
        let (_tmp, mut s) = same_dir();
        s.run_line("cursor -1").unwrap(); // on ".."
        assert!(s.run_line("copy").is_err());
        assert!(s.pending.is_none());
    }

    #[test]
    fn mkdir_and_cd_work() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir new").unwrap();
        assert!(tmp.path().join("left/new").is_dir());
        s.run_line("cd new").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/new"));
        assert!(s.run_line("cd nope").is_err());
    }

    #[test]
    fn errors_land_in_status() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("frobnicate").is_err());
        assert!(s.status.contains("nieznana"));
    }

    #[test]
    fn parent_row_is_not_an_operation_target() {
        let (tmp, mut s) = two_dirs();
        // The cursor starts on "..": nothing to copy, and nothing must happen to the parent.
        assert!(s.run_line("copy").is_err());
        assert!(s.run_line("delete").is_err());
        assert!(s.run_line("select ..").is_err());
        assert!(tmp.path().join("left/a.txt").exists());
    }

    #[test]
    fn back_and_forward_follow_the_history() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("cd sub").unwrap();
        s.run_line("back").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("forward").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/sub"));
        assert!(s.run_line("forward").is_err());
        assert!(s.status.contains("historii"));
        // The other panel has its own history.
        s.run_line("switch").unwrap();
        assert!(s.run_line("back").is_err());
    }

    #[test]
    fn backspace_edits_the_filter_before_going_up() {
        let (tmp, mut s) = two_dirs();
        s.run_line("filter b").unwrap();
        assert_eq!(s.active_panel().entries.len(), 2); // ".." and b.txt
        s.run_line("up").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        assert!(s.active_panel().filter.is_empty());
        s.run_line("up").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path());
    }

    #[test]
    fn operations_ignore_entries_hidden_by_the_filter() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("filter b").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle()); // acts on b.txt under the cursor, not on hidden a.txt
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!tmp.path().join("right/a.txt").exists());
    }

    #[test]
    fn volume_jump_by_number_and_name() {
        let (tmp, mut s) = two_dirs();
        s.volumes = vec![
            Volume {
                name: "one".into(),
                path: tmp.path().join("left"),
            },
            Volume {
                name: "Two".into(),
                path: tmp.path().join("right"),
            },
        ];
        s.run_line("volume 2").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
        s.run_line("volume one").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("volume TWO").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
        s.run_line("back").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));

        assert!(s.run_line("volume 0").is_err());
        assert!(s.run_line("volume 3").is_err());
        let err = s.run_line("volume nope").unwrap_err();
        assert!(err.0.contains("dostępne: 1 one, 2 Two"), "{err}");
    }

    #[test]
    fn volume_of_picks_the_longest_prefix() {
        let volumes = vec![
            Volume {
                name: "root".into(),
                path: PathBuf::from("/a"),
            },
            Volume {
                name: "nested".into(),
                path: PathBuf::from("/a/b"),
            },
        ];
        assert_eq!(volume_index(&volumes, Path::new("/a/b/c")), Some(1));
        assert_eq!(volume_index(&volumes, Path::new("/a/x")), Some(0));
        assert_eq!(volume_index(&volumes, Path::new("/z")), None);
    }

    #[test]
    fn selection_commands_report_in_status() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("left/c.md"), "c").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("selmask *.txt").unwrap();
        assert!(s.status.contains("Zaznaczono: 2"), "{}", s.status);
        s.run_line("unselmask b*").unwrap();
        assert_eq!(s.active_panel().selected.len(), 1);
        s.run_line("invert").unwrap();
        assert_eq!(s.active_panel().selected.len(), 2);
        s.run_line("selectall").unwrap();
        assert!(s.status.contains("Zaznaczono: 3"));
        s.run_line("unselectall").unwrap();
        assert!(s.active_panel().selected.is_empty());
        assert!(s.run_line("selmask ;").is_err());
    }

    #[test]
    fn copy_acts_on_the_mask_selection() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("left/c.md"), "c").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("selmask *.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!tmp.path().join("right/c.md").exists());
    }

    #[test]
    fn sort_command_reorders_the_active_panel_only() {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("left/big.bin"), vec![0u8; 100]).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("sort size desc").unwrap();
        let names: Vec<_> = s.panels[0]
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["..", "big.bin", "a.txt", "b.txt"]);
        assert_eq!(s.panels[1].sort.key, crate::SortKey::Name);
        assert!(s.run_line("sort bogus").is_err());
    }

    fn read(path: std::path::PathBuf) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    /// Both dirs hold `a.txt`: "a" on the left, "old" on the right; a.txt is selected.
    fn clash() -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = two_dirs();
        std::fs::write(tmp.path().join("right/a.txt"), "old").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select a.txt").unwrap();
        (tmp, s)
    }

    fn set_mtime(path: std::path::PathBuf, secs: u64) {
        let time = std::time::UNIX_EPOCH + Duration::from_secs(secs);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn an_existing_target_asks_and_overwrite_replaces_it() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        assert!(!s.wait_idle(), "must stop at the conflict");
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        assert!(s.status.contains("a.txt"));
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        let job = s.job_status().unwrap();
        assert!(job.waiting_for_answer);

        s.run_line("overwrite").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "a");
        assert!(s.pending.is_none());
        assert!(!s.status_is_error);
    }

    #[test]
    fn skip_keeps_the_target_and_says_so() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("skip").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        assert!(s.status.contains("pominięto: 1"), "{}", s.status);
    }

    #[test]
    fn keep_both_writes_a_second_copy() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("keepboth").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        assert_eq!(read(tmp.path().join("right/a (kopia).txt")), "a");
    }

    #[test]
    fn newer_overwrites_only_older_targets_and_all_answers_once() {
        let (tmp, mut s) = two_dirs();
        for name in ["a.txt", "b.txt"] {
            std::fs::write(tmp.path().join("right").join(name), "old").unwrap();
        }
        set_mtime(tmp.path().join("left/a.txt"), 2_000); // newer than the target
        set_mtime(tmp.path().join("right/a.txt"), 1_000);
        set_mtime(tmp.path().join("left/b.txt"), 1_000); // older than the target
        set_mtime(tmp.path().join("right/b.txt"), 2_000);
        s.run_line("refresh").unwrap();
        s.run_line("selmask *.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(!s.wait_idle());
        s.run_line("newer-all").unwrap();
        assert!(s.wait_idle(), "one answer covers both conflicts");
        assert_eq!(read(tmp.path().join("right/a.txt")), "a");
        assert_eq!(read(tmp.path().join("right/b.txt")), "old");
        assert!(s.status.contains("pominięto: 1"), "{}", s.status);
    }

    #[test]
    fn other_commands_do_not_release_a_conflict_but_stop_does() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("cursor 1").unwrap();
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        assert!(
            s.run_line("confirm").is_err(),
            "Enter must not answer a conflict"
        );

        s.run_line("stop").unwrap();
        assert!(s.wait_idle());
        assert!(s.pending.is_none());
        assert!(s.status.starts_with("Anulowano"), "{}", s.status);
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
    }

    #[test]
    fn cancel_on_a_conflict_cancels_the_job_and_the_queue() {
        let (tmp, mut s) = clash();
        s.run_line("copy").unwrap();
        s.wait_idle();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap(); // queued behind the blocked job
        assert!(s.status.contains("W kolejce"));
        s.run_line("cancel").unwrap();
        assert!(s.wait_idle());
        assert!(
            !tmp.path().join("right/b.txt").exists(),
            "queue was dropped"
        );
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
    }

    #[test]
    fn answers_without_a_question_are_errors() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("overwrite").is_err());
        assert!(s.run_line("confirm").is_err());
        assert!(s.status_is_error);
        s.run_line("stop").unwrap();
        assert_eq!(s.status, "Nic się nie wykonuje");
    }

    #[test]
    fn auto_resolve_answers_conflicts_without_asking() {
        let (tmp, mut s) = clash();
        s.auto_resolve = Some(Resolution::Skip);
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert_eq!(read(tmp.path().join("right/a.txt")), "old");
        assert!(s.pending.is_none());
    }

    #[test]
    fn jobs_run_one_after_another() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!s.is_busy());
        assert!(s.job_status().is_none());
    }

    #[test]
    fn delete_asks_first_and_cancel_keeps_the_file() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("delete").unwrap();
        assert!(matches!(s.pending, Some(Pending::ConfirmDelete(_))));
        assert!(s.status.contains("kosza"));
        assert!(s.wait_idle(), "nothing was started");
        assert!(tmp.path().join("left/a.txt").exists());
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert!(tmp.path().join("left/a.txt").exists());
        // Cancelling keeps the selection; with nothing selected and the cursor on ".."
        // there is nothing to delete.
        s.run_line("unselectall").unwrap();
        s.run_line("cursor -5").unwrap();
        assert!(s.run_line("delete").is_err());
    }

    fn type_into_dialog(s: &mut Session, text: &str) {
        match &mut s.pending {
            Some(Pending::Input { text: field, .. }) => *field = text.to_string(),
            other => panic!("no name dialog open: {other:?}"),
        }
    }

    #[test]
    fn mkdir_without_a_name_opens_a_dialog() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir").unwrap();
        type_into_dialog(&mut s, "Reports");
        s.run_line("confirm").unwrap();
        assert!(tmp.path().join("left/Reports").is_dir());
        assert!(s.pending.is_none());
        assert_eq!(s.active_panel().current().unwrap().name, "Reports");
    }

    #[test]
    fn a_bad_name_keeps_the_dialog_open_for_correction() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir").unwrap();
        type_into_dialog(&mut s, "a/b");
        assert!(s.run_line("confirm").is_err());
        assert!(s.status_is_error);
        assert!(matches!(s.pending, Some(Pending::Input { .. })));
        type_into_dialog(&mut s, "fixed");
        s.run_line("confirm").unwrap();
        assert!(tmp.path().join("left/fixed").is_dir());
        s.run_line("mkdir").unwrap();
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
    }

    #[test]
    fn rename_dialog_is_prefilled_and_renames_the_entry_under_the_cursor() {
        let (tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("rename").unwrap();
        match &s.pending {
            Some(Pending::Input { text, kind }) => {
                assert_eq!(text, "a.txt");
                assert_eq!(*kind, InputKind::Rename(tmp.path().join("left/a.txt")));
            }
            other => panic!("{other:?}"),
        }
        type_into_dialog(&mut s, "renamed.txt");
        s.run_line("confirm").unwrap();
        assert!(!tmp.path().join("left/a.txt").exists());
        assert_eq!(read(tmp.path().join("left/renamed.txt")), "a");
        assert_eq!(s.active_panel().current().unwrap().name, "renamed.txt");
    }

    #[test]
    fn rename_refuses_existing_names_and_the_parent_row() {
        let (tmp, mut s) = two_dirs();
        assert!(s.run_line("rename x").is_err(), "cursor is on ..");
        s.run_line("cursor 1").unwrap();
        assert!(s.run_line("rename b.txt").is_err());
        assert_eq!(read(tmp.path().join("left/b.txt")), "b");
        s.run_line("rename c.txt").unwrap();
        assert!(tmp.path().join("left/c.txt").exists());
    }

    #[test]
    fn progress_fraction_prefers_bytes_then_items() {
        let mut status = JobStatus {
            label: "Kopiowanie",
            current: String::new(),
            bytes_done: 0,
            bytes_total: 0,
            items_done: 0,
            items_total: 0,
            queued: 0,
            waiting_for_answer: false,
        };
        assert_eq!(status.fraction(), None);
        status.items_total = 4;
        status.items_done = 1;
        assert_eq!(status.fraction(), Some(0.25));
        status.bytes_total = 200;
        status.bytes_done = 50;
        assert_eq!(status.fraction(), Some(0.25));
        status.bytes_done = 500;
        assert_eq!(status.fraction(), Some(1.0));
    }

    #[test]
    fn view_opens_the_file_under_the_cursor_and_close_leaves() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("view").is_err(), "cursor is on ..");
        s.run_line("cursor 1").unwrap();
        s.run_line("view").unwrap();
        let viewer = s.viewer.as_mut().expect("viewer is open");
        viewer.set_viewport(5, 80);
        assert_eq!(viewer.visible_rows()[0].text, "a");
        assert!(s.status.contains("a.txt"));
        s.run_line("viewer close").unwrap();
        assert!(s.viewer.is_none());
        assert!(
            s.run_line("viewer scroll 1").is_err(),
            "nothing is open any more"
        );
    }

    #[test]
    fn view_refuses_directories() {
        let (_tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("select sub").unwrap();
        let err = s.run_line("view").unwrap_err();
        assert!(err.0.contains("katalog"), "{err}");
        assert!(s.viewer.is_none());
    }

    #[test]
    fn viewer_commands_navigate_and_search() {
        let (tmp, mut s) = two_dirs();
        let lines: String = (0..50).map(|i| format!("row {i}\n")).collect();
        std::fs::write(tmp.path().join("left/long.txt"), lines).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select long.txt").unwrap();
        s.run_line("view").unwrap();
        s.viewer.as_mut().unwrap().set_viewport(5, 80);
        s.run_line("viewer scroll 10").unwrap();
        assert_eq!(s.viewer.as_mut().unwrap().visible_rows()[0].text, "row 10");
        s.run_line("viewer goto 0").unwrap();
        s.run_line("viewer find row 33").unwrap();
        assert!(s.status.contains("znaleziono"), "{}", s.status);
        assert_eq!(s.viewer.as_mut().unwrap().visible_rows()[0].text, "row 33");
        s.run_line("viewer mode").unwrap();
        assert_eq!(s.viewer.as_ref().unwrap().mode(), crate::ViewMode::Hex);
        assert!(s.run_line("viewer goto nonsense").is_err());
    }

    #[test]
    fn the_viewer_search_dialog_is_prefilled_with_the_last_search() {
        let (_tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("view").unwrap();
        s.run_line("viewer find").unwrap();
        type_into_dialog(&mut s, "a");
        s.run_line("confirm").unwrap();
        assert!(s.pending.is_none());
        s.run_line("viewer find").unwrap();
        match &s.pending {
            Some(Pending::Input { text, kind }) => {
                assert_eq!(text, "a");
                assert_eq!(*kind, InputKind::ViewerFind);
            }
            other => panic!("{other:?}"),
        }
        s.run_line("cancel").unwrap();
        assert!(
            s.viewer.is_some(),
            "cancelling the dialog keeps the viewer open"
        );
    }

    fn titles(s: &Session, side: usize) -> Vec<String> {
        s.tab_strip(side).titles
    }

    #[test]
    fn a_new_tab_copies_the_directory_and_becomes_active() {
        let (tmp, mut s) = two_dirs();
        let strip = s.tab_strip(0);
        assert_eq!(strip.titles, ["left"]);
        assert_eq!(strip.paths, [tmp.path().join("left")]);
        assert_eq!(strip.active, 0);
        s.run_line("tab new").unwrap();
        assert_eq!(titles(&s, 0), ["left", "left"]);
        assert_eq!(s.tab_strip(0).active, 1);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left"));
        assert!(s.status.contains("2 z 2"), "{}", s.status);
        // The other side is untouched.
        assert_eq!(titles(&s, 1), ["right"]);
    }

    #[test]
    fn tabs_keep_their_own_directory_history_and_sort() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd sub").unwrap();
        s.run_line("sort size desc").unwrap();
        assert_eq!(titles(&s, 0), ["left", "sub"]);
        s.run_line("tab 1").unwrap();
        assert_eq!(s.panels[0].cwd, tmp.path().join("left"));
        assert!(
            !s.panels[0].sort.descending,
            "the first tab was not re-sorted"
        );
        assert!(!s.panels[0].can_go_back());
        s.run_line("tab 2").unwrap();
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/sub"));
        assert!(s.panels[0].sort.descending);
        assert!(s.panels[0].can_go_back(), "history belongs to the tab");
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let (_tmp, mut s) = two_dirs();
        s.run_line("tab new").unwrap();
        s.run_line("tab new").unwrap();
        assert_eq!(s.tab_strip(0).active, 2);
        s.run_line("tab next").unwrap();
        assert_eq!(s.tab_strip(0).active, 0);
        s.run_line("tab prev").unwrap();
        assert_eq!(s.tab_strip(0).active, 2);
        s.run_line("tab prev").unwrap();
        assert_eq!(s.tab_strip(0).active, 1);
    }

    #[test]
    fn open_puts_the_directory_under_the_cursor_in_a_new_tab() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("cursor -5").unwrap(); // up to ".."
        assert!(s.run_line("tab open").is_err());
        s.run_line("select sub").unwrap();
        s.run_line("tab open").unwrap();
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/sub"));
        assert_eq!(titles(&s, 0), ["left", "sub"]);
        s.run_line("tab 1").unwrap();
        s.run_line("cursor 100").unwrap();
        assert!(s.run_line("tab open").is_err(), "a file is not a directory");
    }

    #[test]
    fn closing_tabs_activates_a_neighbour_and_keeps_the_last_one() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir a").unwrap();
        s.run_line("mkdir b").unwrap();
        s.run_line("tab new").unwrap(); // tabs: left, left*
        s.run_line("cd a").unwrap(); // left, a*
        s.run_line("tab new").unwrap(); // left, a, a*
        s.run_line("cd ..").unwrap();
        s.run_line("cd b").unwrap(); // left, a, b*
        assert_eq!(titles(&s, 0), ["left", "a", "b"]);

        // Closing the last tab activates the one on its left.
        s.run_line("tab close").unwrap();
        assert_eq!(titles(&s, 0), ["left", "a"]);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/a"));
        // Closing a background tab keeps the active one and fixes the index.
        s.run_line("tab close 1").unwrap();
        assert_eq!(titles(&s, 0), ["a"]);
        assert_eq!(s.tab_strip(0).active, 0);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/a"));
        assert!(s.run_line("tab close").is_err(), "the last tab stays");
    }

    #[test]
    fn closing_the_active_tab_in_the_middle_activates_the_one_to_its_right() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir a").unwrap();
        s.run_line("mkdir b").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd a").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd ..").unwrap();
        s.run_line("cd b").unwrap(); // left, a, b*
        s.run_line("tab 2").unwrap();
        s.run_line("tab close").unwrap();
        assert_eq!(titles(&s, 0), ["left", "b"]);
        assert_eq!(s.panels[0].cwd, tmp.path().join("left/b"));
        assert_eq!(s.tab_strip(0).active, 1);
    }

    #[test]
    fn bad_tab_numbers_are_errors() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("tab 2").is_err());
        assert!(s.run_line("tab close 3").is_err());
        assert!(s.run_line("tab 0").is_err());
        assert!(s.status_is_error);
    }

    #[test]
    fn a_background_tab_is_refreshed_when_it_is_shown_again() {
        let (tmp, mut s) = two_dirs();
        s.run_line("tab new").unwrap();
        s.run_line("tab 1").unwrap();
        std::fs::write(tmp.path().join("left/fresh.txt"), "x").unwrap();
        s.run_line("tab 2").unwrap();
        assert!(s.panels[0].entries.iter().any(|e| e.name == "fresh.txt"));
    }

    #[test]
    fn operations_use_the_active_tab_of_each_side() {
        let (tmp, mut s) = two_dirs();
        std::fs::create_dir(tmp.path().join("other")).unwrap();
        s.run_line("switch").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("other").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.wait_idle());
        assert!(
            tmp.path().join("other/a.txt").exists(),
            "copied into the active tab of the other side"
        );
        assert!(!tmp.path().join("right/a.txt").exists());
    }

    #[test]
    fn cd_normalises_dot_and_dotdot() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir a").unwrap();
        s.run_line("cd a").unwrap();
        s.run_line("cd ..").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("cd ./a/../a/.").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/a"));
        s.run_line(&format!("cd {}/left/a/../../right", tmp.path().display()))
            .unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
    }

    #[test]
    fn normalising_never_climbs_above_the_root() {
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        let beyond = Path::new(root).join("..").join("..");
        assert_eq!(normalize_path(&beyond), Path::new(root));
        assert_eq!(normalize_path(Path::new("a/b/../../..")), Path::new(""));
    }
}
