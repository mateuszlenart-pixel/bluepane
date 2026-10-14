//! The job queue as the user sees it (Ctrl+J): what runs, what waits and can be removed,
//! what finished and can be run again; the queue can be paused.

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
        self.finished.push_front(FinishedJob {
            title: spec.describe(),
            result: text.to_string(),
            is_error,
            cancelled,
            spec: spec.clone(),
        });
        self.finished.truncate(HISTORY);
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
}
