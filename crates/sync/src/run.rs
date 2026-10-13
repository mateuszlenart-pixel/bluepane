//! The two calls a front end needs: [`preview`] (nothing is written) and [`apply`].

use std::path::Path;
use std::time::Duration;

use bluepane_ops::{Host, OpError, Progress};

use crate::compare::{compare, CompareBy, Diff};
use crate::execute::{execute, Deletion, ExecOptions, ExecReport};
use crate::filter::Filter;
use crate::plan::{plan, Plan, Variant};
use crate::scan::scan;
use crate::state::SyncState;

#[derive(Debug, Clone)]
pub struct Settings {
    pub filter: Filter,
    pub compare: CompareBy,
    pub variant: Variant,
    pub deletion: Deletion,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            filter: Filter::default(),
            compare: CompareBy::default(),
            variant: Variant::TwoWay,
            deletion: Deletion::Trash,
        }
    }
}

impl Settings {
    fn tolerance(&self) -> Duration {
        match self.compare {
            CompareBy::TimeAndSize { tolerance } => tolerance,
            _ => Duration::from_secs(2),
        }
    }
}

/// What a synchronization would do. Reading only.
#[derive(Debug, Clone)]
pub struct Preview {
    pub diffs: Vec<Diff>,
    pub plan: Plan,
    /// Two-way: whether a snapshot of the previous sync was found and used.
    pub used_snapshot: bool,
    /// Symbolic links and the like, on both sides, that take no part.
    pub skipped_links: u64,
    /// Entries that could not be read (permissions, vanished files).
    pub unreadable: Vec<String>,
}

pub fn preview(
    left: &Path,
    right: &Path,
    settings: &Settings,
    progress: &Progress,
) -> Result<Preview, OpError> {
    for root in [left, right] {
        if !root.is_dir() {
            return Err(OpError::InvalidName(format!(
                "to nie katalog: {}",
                root.display()
            )));
        }
    }
    let l = scan(left, &settings.filter, progress)?;
    let r = scan(right, &settings.filter, progress)?;
    let diffs = compare(left, right, &l, &r, settings.compare, progress)?;
    let snapshot = (settings.variant == Variant::TwoWay)
        .then(|| SyncState::load(left, right))
        .flatten();
    let plan = plan(
        &diffs,
        settings.variant,
        snapshot.as_ref(),
        settings.tolerance(),
    );
    let mut unreadable = l.unreadable;
    unreadable.extend(r.unreadable);
    Ok(Preview {
        diffs,
        plan,
        used_snapshot: snapshot.is_some(),
        skipped_links: l.skipped_links + r.skipped_links,
        unreadable,
    })
}

/// Executes the plan of `preview`, then re-reads both folders and remembers what is equal
/// now (the snapshot two-way needs next time). The folders must not have changed since the
/// preview was made: anything that did is simply copied or deleted as planned, so show a
/// fresh preview when in doubt.
pub fn apply(
    preview: &Preview,
    left: &Path,
    right: &Path,
    settings: &Settings,
    progress: &Progress,
    host: &mut dyn Host,
) -> Result<ExecReport, OpError> {
    let options = ExecOptions {
        deletion: settings.deletion.clone(),
    };
    let report = execute(&preview.plan, left, right, &options, progress, host)?;
    if settings.variant == Variant::TwoWay || report.failed.is_empty() {
        let l = scan(left, &settings.filter, progress)?;
        let r = scan(right, &settings.filter, progress)?;
        let after = compare(left, right, &l, &r, settings.compare, progress)?;
        // A snapshot that cannot be written only makes the next two-way run more careful.
        let _ = SyncState::from_diffs(&after).save(left, right);
    }
    Ok(report)
}
