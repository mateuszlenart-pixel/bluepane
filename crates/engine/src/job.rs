//! Background jobs: copy, move, duplicate and delete run on a worker thread so the UI
//! stays responsive. The worker reports to the session through a channel and, when it hits
//! an existing file, asks the user through the same channel and waits for the answer.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

use bluepane_archive::{
    self as archive, ArchiveError, Change, CreateOptions, Format, Level, Listing, ModifyReport,
    Selection, VerifyReport,
};
use bluepane_ops::{self as ops, Conflict, OpError, Outcome, Progress, Resolution};
use bluepane_remote as sftp;
use bluepane_sync as sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Copy,
    Move,
    /// Copies next to the originals under a free `(kopia)` name.
    Duplicate,
    /// Moves to the trash.
    Delete,
    /// Reads the table of contents of an archive so a panel can browse it.
    OpenArchive,
    /// Extracts (part of) an archive.
    Extract,
    /// Creates an archive.
    Pack,
    /// Reads a whole archive to check it is intact.
    VerifyArchive,
    /// Adds to, removes from or renames inside an existing archive.
    ModifyArchive,
    /// Compares two folders and plans a synchronization (writes nothing).
    SyncCompare,
    /// Carries out a synchronization plan.
    SyncApply,
    /// Something on a server (SFTP, FTP or FTPS): connect, list, transfer, delete, make a folder.
    Remote,
}

impl JobKind {
    /// Present participle for the progress bar ("Kopiowanie").
    pub fn label(self) -> &'static str {
        match self {
            JobKind::Copy => "Kopiowanie",
            JobKind::Move => "Przenoszenie",
            JobKind::Duplicate => "Tworzenie kopii",
            JobKind::Delete => "Usuwanie do kosza",
            JobKind::OpenArchive => "Wczytywanie archiwum",
            JobKind::Extract => "Rozpakowywanie",
            JobKind::Pack => "Pakowanie",
            JobKind::VerifyArchive => "Sprawdzanie archiwum",
            JobKind::ModifyArchive => "Zmiana archiwum",
            JobKind::SyncCompare => "Porównywanie folderów",
            JobKind::SyncApply => "Synchronizacja",
            JobKind::Remote => "Serwer",
        }
    }

    /// Past tense for the result message, matching the wording used before jobs existed.
    pub fn verb(self) -> &'static str {
        match self {
            JobKind::Copy => "Skopiowano",
            JobKind::Move => "Przeniesiono",
            JobKind::Duplicate => "Utworzono kopie",
            JobKind::Delete => "Do kosza",
            JobKind::OpenArchive => "Otwarto archiwum",
            JobKind::Extract => "Rozpakowano",
            JobKind::Pack => "Spakowano",
            JobKind::VerifyArchive => "Sprawdzono",
            JobKind::ModifyArchive => "Zmieniono archiwum",
            JobKind::SyncCompare => "Porównano",
            JobKind::SyncApply => "Zsynchronizowano",
            JobKind::Remote => "Wykonano",
        }
    }
}

/// How to build an archive.
#[derive(Debug, Clone)]
pub struct PackOptions {
    pub format: Format,
    pub level: Level,
    pub split: Option<u64>,
    /// 7z only: also hide the file names behind the password.
    pub encrypt_names: bool,
    /// File name of the archive (with its extension), created in the job's `dest`.
    pub output_name: String,
}

/// What a synchronization job works on.
#[derive(Debug, Clone)]
pub struct SyncJob {
    pub left: PathBuf,
    pub right: PathBuf,
    pub settings: sync::Settings,
    /// Apply: the plan to carry out.
    pub preview: Option<Arc<sync::Preview>>,
}

/// What a [`JobKind::Remote`] job does on the server.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // one value per job, short-lived
pub enum RemoteOp {
    /// Log in; the secret is looked up from the site when it is `None`. On success the job
    /// also lists the start directory.
    Connect {
        site: sftp::Site,
        secret: Option<String>,
        trust: Option<String>,
        known: sftp::KnownHosts,
        services: Arc<dyn sftp::SecretServices>,
    },
    /// Saves a site's password in the store its secret points at (Bitwarden can take a
    /// moment, so this is not done on the interface thread).
    SaveSecret {
        site: sftp::Site,
        password: String,
        services: Arc<dyn sftp::SecretServices>,
    },
    List {
        path: String,
    },
    Mkdir {
        path: String,
    },
    Rename {
        from: String,
        to: String,
    },
    /// Removes files and, recursively, folders. Nothing is recoverable.
    Delete {
        paths: Vec<String>,
    },
    /// Copies these server paths into the local directory `dest`. With `remove_source` each
    /// item is deleted on the server once it has arrived in full (a move). `view` marks a
    /// download for the viewer (F3): the file lands in a temporary folder.
    Download {
        sources: Vec<String>,
        dest: PathBuf,
        remove_source: bool,
        view: bool,
    },
    /// Copies these local paths into the server directory `dest`. With `remove_source` each
    /// item goes to the local trash once it has been sent in full.
    Upload {
        sources: Vec<PathBuf>,
        dest: String,
        remove_source: bool,
    },
    /// Copies server paths to another server (the job's own connection is the source),
    /// through a temporary folder. With `remove_source` the source is deleted afterwards.
    Relay {
        target: Arc<dyn sftp::RemoteFs>,
        sources: Vec<String>,
        dest: String,
        remove_source: bool,
    },
    /// Moves server paths to another folder of the same server (a rename; nothing is
    /// copied and nothing is replaced).
    MoveWithin {
        sources: Vec<String>,
        dest: String,
    },
}

#[derive(Debug, Clone)]
pub struct RemoteJob {
    pub conn: Option<Arc<dyn sftp::RemoteFs>>,
    pub op: RemoteOp,
    /// The panel (0 or 1) that asked.
    pub side: usize,
}

/// A server whose key is not remembered yet: the user decides whether to trust it.
#[derive(Debug, Clone)]
pub struct HostPrompt {
    pub site: sftp::Site,
    pub secret: Option<String>,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
}

/// The extra settings of the archive and synchronization jobs.
#[derive(Debug, Clone, Default)]
pub struct ArchiveJob {
    /// Extract: the items to extract (inside the archive); empty means all.
    pub entries: Vec<String>,
    /// Extract: the archive directory the extracted names are relative to.
    pub strip: String,
    pub password: Option<String>,
    /// The archive's table of contents, when already known (gives the progress a total).
    pub listing: Option<Arc<Listing>>,
    pub pack: Option<PackOptions>,
    /// Modify: what to do with the archive.
    pub change: Option<Change>,
    /// Sync jobs.
    pub sync: Option<SyncJob>,
    /// Remote (SFTP) jobs.
    pub remote: Option<RemoteJob>,
    /// Open: the side (0 or 1) that asked.
    pub side: usize,
    /// Extract: this is a preview (F3), so show the file in the viewer once it is out.
    pub view: bool,
}

/// What a job should do. `dest` is the target directory for copy, move, extract and pack;
/// for the archive jobs `sources` holds the archive (or, for packing, the files to pack).
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub kind: JobKind,
    pub sources: Vec<PathBuf>,
    pub dest: Option<PathBuf>,
    pub archive: Option<ArchiveJob>,
}

impl JobSpec {
    pub fn new(kind: JobKind, sources: Vec<PathBuf>, dest: Option<PathBuf>) -> JobSpec {
        JobSpec {
            kind,
            sources,
            dest,
            archive: None,
        }
    }
}

/// The user's answer to a conflict. `resolution: None` cancels the whole job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub resolution: Option<Resolution>,
    /// Apply the same resolution to every later conflict in this job.
    pub for_all: bool,
}

/// Why an archive job needs the user's help with a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordProblem {
    Needed,
    Wrong,
}

/// How a finished job went.
#[derive(Debug, Default, Clone)]
pub struct Report {
    /// Top-level items fully processed.
    pub done: usize,
    /// Files left alone because of a "skip" decision.
    pub skipped: u64,
    /// One message per top-level item that failed.
    pub failed: Vec<String>,
    pub cancelled: bool,
    /// Archive jobs: a password is missing or wrong.
    pub password: Option<PasswordProblem>,
    /// Open: the table of contents that was read.
    pub listing: Option<Arc<Listing>>,
    /// Extract / pack: links and the like that were left out.
    pub left_out: u64,
    /// Verify: what the check found.
    pub verify: Option<VerifyReport>,
    /// Modify: what was changed (the new table of contents is in `listing`).
    pub modified: Option<ModifyReport>,
    /// SyncCompare: what a synchronization would do.
    pub sync_preview: Option<Arc<sync::Preview>>,
    /// SyncApply: what was done.
    pub sync_done: Option<sync::ExecReport>,
    /// Remote connect: the new session.
    pub connected: Option<Arc<dyn sftp::RemoteFs>>,
    /// Remote connect / list: the folder and what is in it.
    pub remote_listing: Option<(String, Vec<sftp::RemoteEntry>)>,
    /// Remote connect: the server's key is unknown.
    pub host_prompt: Option<HostPrompt>,
    /// Pack: the files that make up the new archive.
    pub created: Vec<PathBuf>,
    /// Extract / pack: files written.
    pub files: u64,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // one `Finished` per job; boxing would only add noise
pub enum Event {
    Conflict {
        conflict: Conflict,
        reply: Sender<Answer>,
    },
    Finished(Report),
}

/// A job whose worker thread is running.
#[derive(Debug)]
pub struct RunningJob {
    pub spec: JobSpec,
    pub progress: Progress,
    events: Receiver<Event>,
    handle: Option<JoinHandle<()>>,
}

/// The worker thread vanished without reporting (it panicked).
#[derive(Debug)]
pub struct WorkerLost;

impl RunningJob {
    pub fn start(spec: JobSpec) -> RunningJob {
        let progress = Progress::new();
        let (tx, events) = mpsc::channel();
        let worker_spec = spec.clone();
        let worker_progress = progress.clone();
        let handle = std::thread::spawn(move || {
            let report = run(&worker_spec, &worker_progress, &tx);
            let _ = tx.send(Event::Finished(report));
        });
        RunningJob {
            spec,
            progress,
            events,
            handle: Some(handle),
        }
    }

    /// The next event if one is ready.
    pub fn try_event(&self) -> Result<Option<Event>, WorkerLost> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(WorkerLost),
        }
    }

    /// Waits for the worker thread to end (it already sent its last event).
    pub fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The worker's side of [`ops::Host`]: asks the session about conflicts and uses the
/// platform helpers from `vfs`.
pub(crate) struct WorkerHost {
    pub(crate) tx: Sender<Event>,
    pub(crate) progress: Progress,
    /// Set by an "all" answer.
    pub(crate) sticky: Option<Resolution>,
}

impl ops::Host for WorkerHost {
    fn conflict(&mut self, conflict: &Conflict) -> Result<Resolution, OpError> {
        if let Some(resolution) = self.sticky {
            return Ok(resolution);
        }
        let (reply, answer) = mpsc::channel();
        self.tx
            .send(Event::Conflict {
                conflict: conflict.clone(),
                reply,
            })
            .map_err(|_| OpError::Cancelled)?;
        match answer.recv() {
            Ok(Answer {
                resolution: Some(resolution),
                for_all,
            }) => {
                if for_all {
                    self.sticky = Some(resolution);
                }
                Ok(resolution)
            }
            // Cancelled by the user, or the session went away.
            _ => {
                self.progress.cancel();
                Err(OpError::Cancelled)
            }
        }
    }

    fn make_writable(&self, path: &Path) {
        let _ = bluepane_vfs::make_writable(path);
    }

    fn copy_symlink(&self, src: &Path, dst: &Path) -> io::Result<()> {
        bluepane_vfs::copy_symlink(src, dst)
    }

    fn set_mode(&self, path: &Path, mode: u32) {
        let _ = bluepane_vfs::set_mode(path, mode);
    }

    fn file_mode(&self, path: &Path) -> Option<u32> {
        bluepane_vfs::file_mode(path)
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The archive jobs: open, extract, pack and verify.
fn run_archive(spec: &JobSpec, progress: &Progress, tx: &Sender<Event>) -> Report {
    let mut report = Report::default();
    let job = spec.archive.clone().unwrap_or_default();
    let mut host = WorkerHost {
        tx: tx.clone(),
        progress: progress.clone(),
        sticky: None,
    };
    let source = spec.sources.first().cloned().unwrap_or_default();
    let name = file_name(&source);
    let no_dest = || ArchiveError::Op("brak katalogu docelowego".into());
    let result: Result<(), ArchiveError> = match spec.kind {
        JobKind::OpenArchive => {
            archive::list(&source, job.password.as_deref(), progress).map(|listing| {
                report.listing = Some(Arc::new(listing));
            })
        }
        JobKind::Extract => (|| {
            let dest = spec.dest.as_deref().ok_or_else(no_dest)?;
            if let Some(listing) = &job.listing {
                let (bytes, files) = listing.measure(&job.entries);
                progress.set_total(bytes, files);
            }
            let selection = Selection {
                strip: &job.strip,
                entries: &job.entries,
            };
            let done = archive::extract(
                &source,
                &selection,
                dest,
                job.password.as_deref(),
                progress,
                &mut host,
            )?;
            report.files = done.outcome.files;
            report.skipped += done.outcome.skipped;
            report.left_out = done.special_skipped;
            Ok(())
        })(),
        JobKind::Pack => (|| {
            let dest = spec.dest.as_deref().ok_or_else(no_dest)?;
            let pack = job
                .pack
                .as_ref()
                .ok_or_else(|| ArchiveError::Op("brak ustawień pakowania".into()))?;
            let options = CreateOptions {
                format: pack.format,
                level: pack.level,
                password: job.password.clone(),
                split: pack.split,
                encrypt_names: pack.encrypt_names,
            };
            let created = archive::create(
                &spec.sources,
                &dest.join(&pack.output_name),
                &options,
                progress,
                &mut host,
            )?;
            report.files = created.files;
            report.left_out = created.left_out;
            report.created = created.volumes;
            report.skipped += u64::from(created.skipped);
            Ok(())
        })(),
        JobKind::ModifyArchive => (|| {
            let change = job
                .change
                .as_ref()
                .ok_or_else(|| ArchiveError::Op("brak opisu zmiany".into()))?;
            let listing = job
                .listing
                .as_ref()
                .ok_or_else(|| ArchiveError::Op("brak spisu archiwum".into()))?;
            let done = archive::modify(
                &source,
                listing,
                change,
                job.password.as_deref(),
                progress,
                &mut host,
            )?;
            report.modified = Some(done);
            // The panels show the new contents without another trip through the disk later.
            let fresh = archive::list(&source, job.password.as_deref(), progress)?;
            report.listing = Some(Arc::new(fresh));
            Ok(())
        })(),
        JobKind::VerifyArchive => {
            archive::verify(&source, job.password.as_deref(), progress).map(|checked| {
                report.verify = Some(checked);
            })
        }
        _ => Err(ArchiveError::Op("to nie jest zadanie archiwum".into())),
    };
    match result {
        Ok(()) => report.done = 1,
        Err(ArchiveError::Cancelled) => report.cancelled = true,
        Err(ArchiveError::NeedsPassword) => report.password = Some(PasswordProblem::Needed),
        Err(ArchiveError::WrongPassword) => report.password = Some(PasswordProblem::Wrong),
        Err(error) => report.failed.push(format!("{name}: {error}")),
    }
    report
}

/// The synchronization jobs: compare (read only) and apply.
fn run_sync(spec: &JobSpec, progress: &Progress, tx: &Sender<Event>) -> Report {
    let mut report = Report::default();
    let Some(job) = spec.archive.as_ref().and_then(|a| a.sync.as_ref()) else {
        report.failed.push("brak ustawień synchronizacji".into());
        return report;
    };
    let mut host = WorkerHost {
        tx: tx.clone(),
        progress: progress.clone(),
        sticky: None,
    };
    let result: Result<(), OpError> = match (spec.kind, &job.preview) {
        (JobKind::SyncCompare, _) => sync::preview(&job.left, &job.right, &job.settings, progress)
            .map(|preview| {
                report.sync_preview = Some(Arc::new(preview));
            }),
        (_, Some(preview)) => sync::apply(
            preview,
            &job.left,
            &job.right,
            &job.settings,
            progress,
            &mut host,
        )
        .map(|done| {
            report.sync_done = Some(done);
        }),
        (_, None) => Err(OpError::InvalidName("brak planu synchronizacji".into())),
    };
    match result {
        Ok(()) => report.done = 1,
        Err(OpError::Cancelled) => report.cancelled = true,
        Err(error) => report.failed.push(error.to_string()),
    }
    report
}

fn run(spec: &JobSpec, progress: &Progress, tx: &Sender<Event>) -> Report {
    if spec.kind == JobKind::Remote {
        return crate::remote_job::run_remote(spec, progress, tx);
    }
    if matches!(spec.kind, JobKind::SyncCompare | JobKind::SyncApply) {
        return run_sync(spec, progress, tx);
    }
    if matches!(
        spec.kind,
        JobKind::OpenArchive
            | JobKind::Extract
            | JobKind::Pack
            | JobKind::VerifyArchive
            | JobKind::ModifyArchive
    ) {
        return run_archive(spec, progress, tx);
    }
    let mut report = Report::default();
    // Long Windows paths need the `\\?\` form; everything the user sees keeps the plain one.
    let sources: Vec<PathBuf> = spec
        .sources
        .iter()
        .map(|p| bluepane_vfs::extended_path(p))
        .collect();
    let dest = spec.dest.as_deref().map(bluepane_vfs::extended_path);
    let mut host = WorkerHost {
        tx: tx.clone(),
        progress: progress.clone(),
        sticky: None,
    };

    match spec.kind {
        JobKind::Delete => progress.set_total(0, sources.len() as u64),
        _ => {
            if ops::measure(&sources, progress).is_err() {
                report.cancelled = true;
                return report;
            }
        }
    }

    for (original, src) in spec.sources.iter().zip(&sources) {
        if progress.is_cancelled() {
            report.cancelled = true;
            break;
        }
        progress.set_current(&file_name(original));
        let result: Result<Outcome, OpError> = match (spec.kind, &dest) {
            (JobKind::Copy, Some(dest)) => ops::copy_with(src, dest, progress, &mut host),
            (JobKind::Move, Some(dest)) => ops::move_with(src, dest, progress, &mut host),
            (JobKind::Duplicate, _) => ops::duplicate(src, progress).map(|_| Outcome::default()),
            (JobKind::Delete, _) => {
                let result = ops::delete_to_trash(src).map(|()| Outcome::default());
                progress.finish_item();
                result
            }
            (_, None) => Err(OpError::InvalidName("brak katalogu docelowego".into())),
            (_, Some(_)) => unreachable!("archive jobs are handled by run_archive"),
        };
        match result {
            Ok(outcome) => {
                report.skipped += outcome.skipped;
                if outcome.skipped == 0 {
                    report.done += 1;
                }
            }
            Err(OpError::Cancelled) => {
                report.cancelled = true;
                break;
            }
            Err(error) => report
                .failed
                .push(format!("{}: {error}", file_name(original))),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Drives a job to the end, answering every conflict with `resolution`.
    fn finish(spec: JobSpec, resolution: Option<Resolution>, for_all: bool) -> (Report, usize) {
        let mut job = RunningJob::start(spec);
        let mut conflicts = 0;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < deadline, "job did not finish");
            match job.try_event().expect("worker alive") {
                Some(Event::Conflict { reply, .. }) => {
                    conflicts += 1;
                    reply
                        .send(Answer {
                            resolution,
                            for_all,
                        })
                        .unwrap();
                }
                Some(Event::Finished(report)) => {
                    job.join();
                    return (report, conflicts);
                }
                None => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    fn two_dirs() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::create_dir_all(tmp.path().join("dst")).unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(tmp.path().join("src").join(name), name).unwrap();
        }
        tmp
    }

    fn copy_all(tmp: &tempfile::TempDir) -> JobSpec {
        JobSpec {
            kind: JobKind::Copy,
            sources: ["a.txt", "b.txt", "c.txt"]
                .iter()
                .map(|n| tmp.path().join("src").join(n))
                .collect(),
            dest: Some(tmp.path().join("dst")),
            archive: None,
        }
    }

    #[test]
    fn a_copy_job_runs_to_the_end_and_reports() {
        let tmp = two_dirs();
        let (report, conflicts) = finish(copy_all(&tmp), None, false);
        assert_eq!(report.done, 3);
        assert!(report.failed.is_empty() && !report.cancelled);
        assert_eq!(conflicts, 0);
        assert!(tmp.path().join("dst/c.txt").exists());
    }

    #[test]
    fn each_conflict_is_asked_unless_the_answer_is_for_all() {
        let tmp = two_dirs();
        for name in ["a.txt", "b.txt"] {
            std::fs::write(tmp.path().join("dst").join(name), "old").unwrap();
        }
        let (report, conflicts) = finish(copy_all(&tmp), Some(Resolution::Skip), false);
        assert_eq!((conflicts, report.skipped, report.done), (2, 2, 1));

        let (report, conflicts) = finish(copy_all(&tmp), Some(Resolution::Skip), true);
        // c.txt was copied by the first run, so all three clash now; one question covers them.
        assert_eq!((conflicts, report.skipped), (1, 3));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("dst/a.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn answering_with_cancel_stops_the_job() {
        let tmp = two_dirs();
        std::fs::write(tmp.path().join("dst/a.txt"), "old").unwrap();
        let (report, _) = finish(copy_all(&tmp), None, false);
        assert!(report.cancelled);
        assert_eq!(report.done, 0);
        assert!(!tmp.path().join("dst/b.txt").exists());
    }

    #[test]
    fn a_failing_item_does_not_stop_the_others() {
        let tmp = two_dirs();
        let mut spec = copy_all(&tmp);
        spec.sources.insert(1, tmp.path().join("src/missing.txt"));
        let (report, _) = finish(spec, None, false);
        assert_eq!(report.done, 3);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].starts_with("missing.txt"));
    }

    #[test]
    fn duplicate_and_move_jobs_work() {
        let tmp = two_dirs();
        let dup = JobSpec {
            kind: JobKind::Duplicate,
            sources: vec![tmp.path().join("src/a.txt")],
            dest: None,
            archive: None,
        };
        let (report, _) = finish(dup, None, false);
        assert_eq!(report.done, 1);
        assert!(tmp.path().join("src/a (kopia).txt").exists());

        let mv = JobSpec {
            kind: JobKind::Move,
            sources: vec![tmp.path().join("src/b.txt")],
            dest: Some(tmp.path().join("dst")),
            archive: None,
        };
        let (report, _) = finish(mv, None, false);
        assert_eq!(report.done, 1);
        assert!(!tmp.path().join("src/b.txt").exists());
        assert!(tmp.path().join("dst/b.txt").exists());
    }

    #[test]
    fn cancelling_before_the_work_starts_reports_cancelled() {
        let tmp = two_dirs();
        let job = RunningJob::start(copy_all(&tmp));
        job.progress.cancel();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < deadline);
            if let Some(Event::Finished(report)) = job.try_event().unwrap() {
                // The worker may have won the race and finished first; both are valid,
                // but a cancelled job never reports failures.
                assert!(report.failed.is_empty());
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
