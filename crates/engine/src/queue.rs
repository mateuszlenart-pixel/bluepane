//! The job queue as the user sees it (Ctrl+J): what runs, what waits and can be removed,
//! what finished and can be run again; the queue can be paused.

use crate::command::{QueueMove, SpeedLimit};
use crate::job::{JobKind, JobSpec, RemoteOp};
use crate::session::Session;
use crate::EngineError;

/// How many finished jobs are remembered.
const HISTORY: usize = 30;

/// A job that has ended.
#[derive(Debug, Clone)]
pub struct FinishedJob {
    /// What it did (`Pobieranie z serwera: a.txt (+2)`).
    pub title: String,
    /// How it went (the status line it produced).
    pub result: String,
    pub is_error: bool,
    pub cancelled: bool,
    pub(crate) spec: JobSpec,
}

/// A finished job to tell the user about (a toast in the corner of the window).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// What the job did (`Kopiowanie: a.txt (+2)`).
    pub title: String,
    /// How it went (the status line it produced).
    pub text: String,
    pub is_error: bool,
    pub cancelled: bool,
}

/// How many notices wait for the front end before the oldest are dropped.
const NOTICES_MAX: usize = 20;

/// One row of the queue window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueLine {
    pub title: String,
    pub running: bool,
}

/// Jobs that are only the interface asking (listings, logins) are not worth a row.
fn is_housekeeping(spec: &JobSpec) -> bool {
    spec.kind == JobKind::Remote
        && spec
            .archive
            .as_ref()
            .and_then(|a| a.remote.as_ref())
            .is_some_and(|r| {
                matches!(
                    r.op,
                    RemoteOp::List { .. } | RemoteOp::Connect { .. } | RemoteOp::SaveSecret { .. }
                )
            })
}

impl Session {
    /// The running job (first, if any) followed by the waiting ones.
    pub fn queue_lines(&self) -> Vec<QueueLine> {
        let mut lines = Vec::new();
        if let Some(job) = self.current_spec() {
            if !is_housekeeping(job) {
                lines.push(QueueLine {
                    title: job.describe(),
                    running: true,
                });
            }
        }
        lines.extend(
            self.queue_specs()
                .filter(|spec| !is_housekeeping(spec))
                .map(|spec| QueueLine {
                    title: spec.describe(),
                    running: false,
                }),
        );
        lines
    }

    pub(crate) fn record_finished(
        &mut self,
        spec: &JobSpec,
        text: &str,
        is_error: bool,
        cancelled: bool,
    ) {
        if is_housekeeping(spec) {
            return;
        }
        // Files were copied, moved or deleted: counted folder sizes may be wrong now.
        self.dir_sizes.clear();
        self.notices.push(Notice {
            title: spec.describe(),
            text: text.to_string(),
            is_error,
            cancelled,
        });
        if self.notices.len() > NOTICES_MAX {
            self.notices.remove(0);
        }
        self.finished.push_front(FinishedJob {
            title: spec.describe(),
            result: text.to_string(),
            is_error,
            cancelled,
            spec: spec.clone(),
        });
        self.finished.truncate(HISTORY);
    }

    /// The jobs that ended since the last call, oldest first.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    pub(crate) fn pause_queue(&mut self, pause: bool) {
        self.paused = pause;
        if pause {
            self.status =
                "Kolejka wstrzymana: bieżące zadanie dokończy się, następne poczekają".into();
        } else {
            self.status = "Kolejka wznowiona".into();
            // Whatever was waiting starts now.
            if self.current_spec().is_none() {
                self.start_waiting();
            }
        }
    }

    /// Removes waiting job `n` (1-based among the rows without the running one).
    pub(crate) fn queue_delete(&mut self, n: usize) -> Result<(), EngineError> {
        let running = usize::from(self.current_spec().is_some_and(|s| !is_housekeeping(s)));
        let target = n
            .checked_sub(1 + running)
            .ok_or_else(|| EngineError("to zadanie już trwa: zatrzymaj je komendą stop".into()))?;
        let index = self
            .queue_specs()
            .enumerate()
            .filter(|(_, spec)| !is_housekeeping(spec))
            .nth(target)
            .map(|(index, _)| index)
            .ok_or_else(|| EngineError(format!("nie ma zadania {n} w kolejce")))?;
        let removed = self.remove_queued(index);
        self.status = format!("Usunięto z kolejki: {}", removed.describe());
        Ok(())
    }

    /// The speed limit in bytes per second, if one is set.
    pub fn speed_limit(&self) -> Option<u64> {
        match self.speed_limit.load(std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            limit => Some(limit),
        }
    }

    /// `limit`: applies to the running job at once and to every later one.
    pub(crate) fn set_speed_limit(&mut self, limit: SpeedLimit) {
        match limit {
            SpeedLimit::Show => {}
            SpeedLimit::Off => self
                .speed_limit
                .store(0, std::sync::atomic::Ordering::Relaxed),
            SpeedLimit::BytesPerSecond(bytes) => self
                .speed_limit
                .store(bytes, std::sync::atomic::Ordering::Relaxed),
        }
        self.status = match self.speed_limit() {
            Some(bytes) => format!(
                "Limit prędkości: {}/s (kopiowanie i transfery; do końca sesji)",
                crate::human_size(bytes)
            ),
            None => "Bez limitu prędkości".into(),
        };
    }

    /// Moves waiting job `n` (numbered like `queue_delete`) up, down or to the front of the
    /// waiting jobs. The running job stays where it is.
    pub(crate) fn queue_move(&mut self, n: usize, to: QueueMove) -> Result<(), EngineError> {
        let running = usize::from(self.current_spec().is_some_and(|s| !is_housekeeping(s)));
        let row = n
            .checked_sub(1 + running)
            .ok_or_else(|| EngineError("to zadanie już trwa: nie da się go przesunąć".into()))?;
        // Positions in the real queue of the rows the user sees (listings and logins that
        // wait in between are not shown, and keep their place relative to the neighbours).
        let visible: Vec<usize> = self
            .queue_specs()
            .enumerate()
            .filter(|(_, spec)| !is_housekeeping(spec))
            .map(|(index, _)| index)
            .collect();
        let from = *visible
            .get(row)
            .ok_or_else(|| EngineError(format!("nie ma zadania {n} w kolejce")))?;
        let target = match to {
            QueueMove::Top if row == 0 => None,
            QueueMove::Up if row == 0 => None,
            QueueMove::Down if row + 1 == visible.len() => None,
            QueueMove::Top => Some(visible[0]),
            QueueMove::Up => Some(visible[row - 1]),
            QueueMove::Down => Some(visible[row + 1]),
        };
        let Some(target) = target else {
            return Err(EngineError(match to {
                QueueMove::Down => "to zadanie jest już ostatnie".into(),
                _ => "to zadanie jest już pierwsze".into(),
            }));
        };
        let title = self
            .queue_specs()
            .nth(from)
            .map(JobSpec::describe)
            .unwrap_or_default();
        self.move_queued(from, target);
        self.status = format!("Przesunięto w kolejce: {title}");
        Ok(())
    }

    /// Queues finished job `n` (1 = the newest) again.
    pub(crate) fn queue_retry(&mut self, n: usize) -> Result<(), EngineError> {
        let spec = self
            .finished
            .get(n - 1)
            .map(|job| job.spec.clone())
            .ok_or_else(|| EngineError(format!("nie ma zakończonego zadania {n}")))?;
        self.status = format!("Ponowione: {}", spec.describe());
        self.enqueue_quiet(spec);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(left.join(name), name).unwrap();
        }
        (tmp, Session::new(left, right))
    }

    fn settle(s: &mut Session) {
        for _ in 0..1000 {
            s.poll();
            if !s.is_busy() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("jobs did not finish: {}", s.status);
    }

    #[test]
    fn a_paused_queue_holds_jobs_until_resumed() {
        let (tmp, mut s) = session();
        s.run_line("queuepause").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(s.status.contains("wstrzymana"), "{}", s.status);
        assert_eq!(
            s.queue_lines(),
            [QueueLine {
                title: "Kopiowanie: a.txt".into(),
                running: false
            }]
        );
        std::thread::sleep(Duration::from_millis(50));
        s.poll();
        assert!(
            !tmp.path().join("right/a.txt").exists(),
            "nothing runs while paused"
        );
        s.run_line("queueresume").unwrap();
        settle(&mut s);
        assert_eq!(
            fs::read_to_string(tmp.path().join("right/a.txt")).unwrap(),
            "a.txt"
        );
        assert_eq!(s.finished.len(), 1);
        assert_eq!(s.finished[0].title, "Kopiowanie: a.txt");
        assert!(!s.finished[0].is_error);
        assert!(s.queue_lines().is_empty());
    }

    #[test]
    fn a_waiting_job_can_be_removed() {
        let (tmp, mut s) = session();
        s.run_line("queuepause").unwrap();
        for name in ["a.txt", "b.txt"] {
            s.run_line(&format!("select {name}")).unwrap();
            s.run_line("copy").unwrap();
        }
        assert_eq!(s.queue_lines().len(), 2);
        assert!(s.run_line("queuedel 9").is_err());
        s.run_line("queuedel 1").unwrap();
        assert_eq!(s.queue_lines()[0].title, "Kopiowanie: b.txt");
        s.run_line("queueresume").unwrap();
        settle(&mut s);
        assert!(
            !tmp.path().join("right/a.txt").exists(),
            "the removed job never ran"
        );
        assert!(tmp.path().join("right/b.txt").exists());
    }

    #[test]
    fn waiting_jobs_can_be_reordered_and_run_in_the_new_order() {
        let (tmp, mut s) = session();
        s.run_line("queuepause").unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            s.run_line(&format!("select {name}")).unwrap();
            s.run_line("copy").unwrap();
        }
        let titles =
            |s: &Session| -> Vec<String> { s.queue_lines().into_iter().map(|l| l.title).collect() };
        assert_eq!(
            titles(&s),
            [
                "Kopiowanie: a.txt",
                "Kopiowanie: b.txt",
                "Kopiowanie: c.txt"
            ]
        );
        s.run_line("queuemove 3 top").unwrap();
        assert_eq!(
            titles(&s),
            [
                "Kopiowanie: c.txt",
                "Kopiowanie: a.txt",
                "Kopiowanie: b.txt"
            ]
        );
        s.run_line("queuemove 1 down").unwrap();
        s.run_line("queuemove 3 up").unwrap();
        assert_eq!(
            titles(&s),
            [
                "Kopiowanie: a.txt",
                "Kopiowanie: b.txt",
                "Kopiowanie: c.txt"
            ]
        );
        // The ends and bad rows are explained, not silently ignored.
        assert!(s.run_line("queuemove 1 up").is_err());
        assert!(s.run_line("queuemove 1 top").is_err());
        assert!(s.run_line("queuemove 3 down").is_err());
        assert!(s.run_line("queuemove 9 up").is_err());
        assert!(s.run_line("queuemove 1").is_err());
        assert!(s.run_line("queuemove 1 sideways").is_err());
        s.run_line("queuemove 2 top").unwrap();
        s.run_line("queueresume").unwrap();
        settle(&mut s);
        let order: Vec<String> = s
            .finished
            .iter()
            .rev()
            .map(|job| job.title.clone())
            .collect();
        assert_eq!(
            order,
            [
                "Kopiowanie: b.txt",
                "Kopiowanie: a.txt",
                "Kopiowanie: c.txt"
            ]
        );
        assert!(tmp.path().join("right/c.txt").exists());
    }

    #[test]
    fn the_speed_limit_is_set_shown_and_removed() {
        let (_tmp, mut s) = session();
        assert_eq!(s.speed_limit(), None);
        s.run_line("limit 2m").unwrap();
        assert_eq!(s.speed_limit(), Some(2 * 1024 * 1024));
        assert!(s.status.contains("2.0 MB/s"), "{}", s.status);
        s.run_line("limit 500k/s").unwrap();
        assert_eq!(s.speed_limit(), Some(500 * 1024));
        s.run_line("limit").unwrap();
        assert_eq!(s.speed_limit(), Some(500 * 1024), "showing changes nothing");
        assert!(s.run_line("limit fast").is_err());
        s.run_line("limit off").unwrap();
        assert_eq!(s.speed_limit(), None);
        assert!(s.status.contains("Bez limitu"), "{}", s.status);
    }

    #[test]
    fn a_limited_copy_takes_time() {
        let (tmp, mut s) = session();
        fs::write(tmp.path().join("left/big.bin"), vec![7u8; 300_000]).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("limit 400k").unwrap();
        s.run_line("select big.bin").unwrap();
        let start = std::time::Instant::now();
        s.run_line("copy").unwrap();
        settle(&mut s);
        assert!(
            start.elapsed() >= Duration::from_millis(500),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(
            fs::metadata(tmp.path().join("right/big.bin"))
                .unwrap()
                .len(),
            300_000
        );
    }

    #[test]
    fn a_failed_job_can_be_run_again() {
        let (tmp, mut s) = session();
        s.run_line("queuepause").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        fs::remove_file(tmp.path().join("left/a.txt")).unwrap();
        s.run_line("queueresume").unwrap();
        settle(&mut s);
        assert!(s.finished[0].is_error, "{}", s.finished[0].result);
        fs::write(tmp.path().join("left/a.txt"), "back").unwrap();
        s.run_line("queueretry 1").unwrap();
        settle(&mut s);
        assert_eq!(
            fs::read_to_string(tmp.path().join("right/a.txt")).unwrap(),
            "back"
        );
        assert_eq!(s.finished.len(), 2, "both attempts are remembered");
        assert!(!s.finished[0].is_error);
        assert!(s.run_line("queueretry 9").is_err());
        s.run_line("queueclear").unwrap();
        assert!(s.finished.is_empty());
    }

    #[test]
    fn the_running_job_cannot_be_removed_and_commands_parse() {
        let (_tmp, mut s) = session();
        s.run_line("queue").unwrap();
        assert!(s.queue_open);
        s.run_line("queueclose").unwrap();
        assert!(!s.queue_open);
        assert!(s.run_line("queuedel").is_err());
        assert!(s.run_line("queuedel 0").is_err());
        assert!(s.run_line("queueretry x").is_err());
        assert_eq!(s.keymap.lookup("Ctrl+J"), Some("queue"));
    }

    #[test]
    fn jobs_are_described_in_words() {
        let paths: Vec<std::path::PathBuf> = ["/x/a.txt", "/x/b.txt", "/x/c.txt"]
            .iter()
            .map(Into::into)
            .collect();
        let spec = JobSpec::new(JobKind::Copy, paths, None);
        assert_eq!(spec.describe(), "Kopiowanie: a.txt (+2)");
        assert_eq!(
            JobSpec::new(JobKind::Delete, vec![], None).describe(),
            "Usuwanie do kosza"
        );
    }

    #[test]
    fn every_finished_job_leaves_one_notice_that_is_taken_once() {
        let (_tmp, mut s) = session();
        assert!(s.take_notices().is_empty());
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        settle(&mut s);
        let notices = s.take_notices();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].title.starts_with("Kopiowanie"), "{notices:?}");
        assert!(!notices[0].is_error && !notices[0].cancelled);
        assert!(s.take_notices().is_empty(), "taken once");

        // The same file again: the conflict is not answered, the job is stopped.
        s.run_line("copy").unwrap();
        s.run_line("stop").unwrap();
        settle(&mut s);
        let again = s.take_notices();
        assert_eq!(again.len(), 1, "{again:?}");
        assert!(again[0].cancelled, "{again:?}");
    }

    #[test]
    fn unseen_notices_are_capped() {
        let (_tmp, mut s) = session();
        for i in 0..(NOTICES_MAX + 5) {
            s.record_finished(
                &JobSpec::new(JobKind::Copy, vec![format!("/x/{i}.txt").into()], None),
                "ok",
                false,
                false,
            );
        }
        let notices = s.take_notices();
        assert_eq!(notices.len(), NOTICES_MAX);
        assert!(
            notices[0].title.contains("5.txt"),
            "oldest dropped: {}",
            notices[0].title
        );
    }
}
