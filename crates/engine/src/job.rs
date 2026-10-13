//! Background jobs: copy, move, duplicate and delete run on a worker thread so the UI
//! stays responsive. The worker reports to the session through a channel and, when it hits
//! an existing file, asks the user through the same channel and waits for the answer.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

use bluepane_ops::{self as ops, Conflict, OpError, Outcome, Progress, Resolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Copy,
    Move,
    /// Copies next to the originals under a free `(kopia)` name.
    Duplicate,
    /// Moves to the trash.
    Delete,
}

impl JobKind {
    /// Present participle for the progress bar ("Kopiowanie").
    pub fn label(self) -> &'static str {
        match self {
            JobKind::Copy => "Kopiowanie",
            JobKind::Move => "Przenoszenie",
            JobKind::Duplicate => "Tworzenie kopii",
            JobKind::Delete => "Usuwanie do kosza",
        }
    }

    /// Past tense for the result message, matching the wording used before jobs existed.
    pub fn verb(self) -> &'static str {
        match self {
            JobKind::Copy => "Skopiowano",
            JobKind::Move => "Przeniesiono",
            JobKind::Duplicate => "Utworzono kopie",
            JobKind::Delete => "Do kosza",
        }
    }
}

/// What a job should do. `dest` is the target directory for copy and move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    pub kind: JobKind,
    pub sources: Vec<PathBuf>,
    pub dest: Option<PathBuf>,
}

/// The user's answer to a conflict. `resolution: None` cancels the whole job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub resolution: Option<Resolution>,
    /// Apply the same resolution to every later conflict in this job.
    pub for_all: bool,
}

/// How a finished job went.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Top-level items fully processed.
    pub done: usize,
    /// Files left alone because of a "skip" decision.
    pub skipped: u64,
    /// One message per top-level item that failed.
    pub failed: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug)]
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
struct WorkerHost {
    tx: Sender<Event>,
    progress: Progress,
    /// Set by an "all" answer.
    sticky: Option<Resolution>,
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
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn run(spec: &JobSpec, progress: &Progress, tx: &Sender<Event>) -> Report {
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
        };
        let (report, _) = finish(dup, None, false);
        assert_eq!(report.done, 1);
        assert!(tmp.path().join("src/a (kopia).txt").exists());

        let mv = JobSpec {
            kind: JobKind::Move,
            sources: vec![tmp.path().join("src/b.txt")],
            dest: Some(tmp.path().join("dst")),
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
