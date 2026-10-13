//! Whole synchronizations on real directories (temporary ones only).

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use bluepane_ops::{FailOnConflict, OpError, Progress};
use bluepane_sync::{
    apply, preview, Category, CompareBy, Deletion, Direction, Filter, Op, Rules, Settings, Variant,
    STATE_FILE,
};

fn put(root: &Path, rel: &str, text: &str, secs: u64) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
}

fn read(root: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(root.join(rel)).ok()
}

struct Env {
    _tmp: tempfile::TempDir,
    l: std::path::PathBuf,
    r: std::path::PathBuf,
    versions: std::path::PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let (l, r, versions) = (
        tmp.path().join("l"),
        tmp.path().join("r"),
        tmp.path().join("versions"),
    );
    fs::create_dir_all(&l).unwrap();
    fs::create_dir_all(&r).unwrap();
    Env {
        _tmp: tmp,
        l,
        r,
        versions,
    }
}

fn settings(e: &Env, variant: Variant) -> Settings {
    Settings {
        variant,
        // Never the real trash in tests.
        deletion: Deletion::Versioning(e.versions.clone()),
        ..Settings::default()
    }
}

fn sync(e: &Env, s: &Settings) -> bluepane_sync::ExecReport {
    let p = preview(&e.l, &e.r, s, &Progress::new()).unwrap();
    apply(&p, &e.l, &e.r, s, &Progress::new(), &mut FailOnConflict).unwrap()
}

fn is_in_sync(e: &Env, s: &Settings) -> bool {
    let p = preview(&e.l, &e.r, s, &Progress::new()).unwrap();
    let bad: Vec<_> = p
        .diffs
        .iter()
        .filter(|d| d.category != Category::Equal)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    true
}

#[test]
fn mirror_copies_overwrites_and_removes_extras_keeping_old_versions() {
    let e = env();
    put(&e.l, "keep.txt", "left version", 100);
    put(&e.r, "keep.txt", "old right", 0);
    put(&e.l, "sub/new.txt", "new", 0);
    put(&e.r, "extra.txt", "only right", 0);
    put(&e.r, "extra_dir/x.txt", "x", 0);
    let s = settings(&e, Variant::Mirror);

    let p = preview(&e.l, &e.r, &s, &Progress::new()).unwrap();
    // Nothing is written by a preview.
    assert_eq!(read(&e.r, "keep.txt").as_deref(), Some("old right"));
    assert_eq!(
        p.plan.deletions(),
        3,
        "extra.txt, extra_dir/x.txt, extra_dir"
    );
    assert_eq!(p.plan.conflicts(), 0);

    let report = apply(&p, &e.l, &e.r, &s, &Progress::new(), &mut FailOnConflict).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(read(&e.r, "keep.txt").as_deref(), Some("left version"));
    assert_eq!(read(&e.r, "sub/new.txt").as_deref(), Some("new"));
    assert!(!e.r.join("extra.txt").exists());
    assert!(!e.r.join("extra_dir").exists());
    // The replaced and the deleted files were set aside, not destroyed.
    assert_eq!(read(&e.versions, "keep.txt").as_deref(), Some("old right"));
    assert_eq!(
        read(&e.versions, "extra.txt").as_deref(),
        Some("only right")
    );
    assert_eq!(read(&e.versions, "extra_dir/x.txt").as_deref(), Some("x"));
    // Dates are copied along, so the next comparison finds nothing to do.
    assert!(is_in_sync(&e, &s));
    assert!(preview(&e.l, &e.r, &s, &Progress::new())
        .unwrap()
        .plan
        .is_noop());
}

#[test]
fn update_never_deletes_and_keeps_newer_files_on_the_right() {
    let e = env();
    put(&e.l, "a.txt", "left newer", 100);
    put(&e.r, "a.txt", "old", 0);
    put(&e.l, "b.txt", "left older", 0);
    put(&e.r, "b.txt", "right newer", 100);
    put(&e.r, "only_right.txt", "stay", 0);
    let s = settings(&e, Variant::Update);
    sync(&e, &s);
    assert_eq!(read(&e.r, "a.txt").as_deref(), Some("left newer"));
    assert_eq!(read(&e.r, "b.txt").as_deref(), Some("right newer"));
    assert_eq!(read(&e.r, "only_right.txt").as_deref(), Some("stay"));
    assert_eq!(read(&e.l, "b.txt").as_deref(), Some("left older"));
    // Only the file that was really replaced was set aside; nothing was deleted.
    assert_eq!(read(&e.versions, "a.txt").as_deref(), Some("old"));
    assert!(!e.versions.join("only_right.txt").exists());
    assert!(!e.versions.join("b.txt").exists());
}

#[test]
fn two_way_runs_remember_each_other_so_deletions_travel() {
    let e = env();
    put(&e.l, "from_left.txt", "L", 0);
    put(&e.r, "from_right.txt", "R", 0);
    put(&e.l, "shared.txt", "v1", 0);
    put(&e.r, "shared.txt", "v1", 0);
    put(&e.l, "will_vanish.txt", "bye", 0);
    put(&e.r, "will_vanish.txt", "bye", 0);
    put(&e.l, "will_change.txt", "before", 0);
    put(&e.r, "will_change.txt", "before", 0);
    let s = settings(&e, Variant::TwoWay);

    // First run: no memory, so everything is copied, nothing deleted.
    let p = preview(&e.l, &e.r, &s, &Progress::new()).unwrap();
    assert!(!p.used_snapshot);
    assert_eq!(p.plan.deletions(), 0);
    sync(&e, &s);
    assert_eq!(read(&e.l, "from_right.txt").as_deref(), Some("R"));
    assert_eq!(read(&e.r, "from_left.txt").as_deref(), Some("L"));
    assert!(e.l.join(STATE_FILE).exists() && e.r.join(STATE_FILE).exists());

    // Later: a deletion on the left and an edit on the right.
    fs::remove_file(e.l.join("will_vanish.txt")).unwrap();
    put(&e.r, "will_change.txt", "after, longer", 500);
    let p = preview(&e.l, &e.r, &s, &Progress::new()).unwrap();
    assert!(p.used_snapshot);
    let op = |name: &str| &p.plan.actions.iter().find(|a| a.path == name).unwrap().op;
    assert_eq!(op("will_vanish.txt"), &Op::DeleteRight);
    assert_eq!(op("will_change.txt"), &Op::CopyToLeft);
    apply(&p, &e.l, &e.r, &s, &Progress::new(), &mut FailOnConflict).unwrap();
    assert!(!e.r.join("will_vanish.txt").exists());
    assert_eq!(read(&e.versions, "will_vanish.txt").as_deref(), Some("bye"));
    assert_eq!(
        read(&e.l, "will_change.txt").as_deref(),
        Some("after, longer")
    );
    assert!(is_in_sync(&e, &s));
    // The snapshot file itself is never treated as content.
    assert!(!read(&e.r, STATE_FILE).is_none());
    assert!(preview(&e.l, &e.r, &s, &Progress::new())
        .unwrap()
        .diffs
        .iter()
        .all(|d| d.path != STATE_FILE));
}

#[test]
fn edits_on_both_sides_stay_a_conflict_until_resolved() {
    let e = env();
    put(&e.l, "doc.txt", "v1", 0);
    put(&e.r, "doc.txt", "v1", 0);
    let s = settings(&e, Variant::TwoWay);
    sync(&e, &s);

    put(&e.l, "doc.txt", "left edit", 100);
    put(&e.r, "doc.txt", "right edit!", 200);
    for _ in 0..2 {
        // Running again and again must not forget the conflict.
        let p = preview(&e.l, &e.r, &s, &Progress::new()).unwrap();
        assert_eq!(p.plan.conflicts(), 1);
        let report = apply(&p, &e.l, &e.r, &s, &Progress::new(), &mut FailOnConflict).unwrap();
        assert_eq!(report.left_alone, 1);
        assert_eq!(read(&e.l, "doc.txt").as_deref(), Some("left edit"));
        assert_eq!(read(&e.r, "doc.txt").as_deref(), Some("right edit!"));
    }
    // Resolved by hand (the user copies one over the other): back in sync.
    put(&e.l, "doc.txt", "right edit!", 200);
    assert!(is_in_sync(&e, &s));
}

#[test]
fn a_deleted_file_that_was_edited_on_the_other_side_is_not_deleted() {
    let e = env();
    put(&e.l, "x.txt", "v1", 0);
    put(&e.r, "x.txt", "v1", 0);
    let s = settings(&e, Variant::TwoWay);
    sync(&e, &s);
    fs::remove_file(e.l.join("x.txt")).unwrap();
    put(&e.r, "x.txt", "edited meanwhile", 300);
    sync(&e, &s);
    assert_eq!(read(&e.r, "x.txt").as_deref(), Some("edited meanwhile"));
    assert!(!e.l.join("x.txt").exists());
}

#[test]
fn custom_rules_and_filters_apply() {
    let e = env();
    put(&e.l, "a.txt", "a", 0);
    put(&e.l, "skip.tmp", "t", 0);
    put(&e.l, "cache/c.txt", "c", 0);
    put(&e.r, "r.txt", "r", 0);
    let s = Settings {
        filter: Filter::new("", "*.tmp;cache/"),
        variant: Variant::Custom(Rules {
            left_only: Direction::ToRight,
            right_only: Direction::Skip,
            left_newer: Direction::ToRight,
            right_newer: Direction::Skip,
            different: Direction::Skip,
        }),
        ..settings(&e, Variant::Mirror)
    };
    sync(&e, &s);
    assert_eq!(read(&e.r, "a.txt").as_deref(), Some("a"));
    assert!(!e.r.join("skip.tmp").exists());
    assert!(!e.r.join("cache").exists());
    assert_eq!(read(&e.r, "r.txt").as_deref(), Some("r"));
}

#[test]
fn content_comparison_finds_changes_that_dates_hide() {
    let e = env();
    put(&e.l, "f.bin", "abcd", 0);
    put(&e.r, "f.bin", "abce", 0);
    let mut s = settings(&e, Variant::Mirror);
    assert!(preview(&e.l, &e.r, &s, &Progress::new())
        .unwrap()
        .plan
        .is_noop());
    s.compare = CompareBy::Content;
    sync(&e, &s);
    assert_eq!(read(&e.r, "f.bin").as_deref(), Some("abcd"));
}

#[test]
fn cancelling_leaves_everything_untouched() {
    let e = env();
    put(&e.l, "a.txt", "a", 0);
    let s = settings(&e, Variant::Mirror);
    let p = preview(&e.l, &e.r, &s, &Progress::new()).unwrap();
    let progress = Progress::new();
    progress.cancel();
    let result = apply(&p, &e.l, &e.r, &s, &progress, &mut FailOnConflict);
    assert!(matches!(result, Err(OpError::Cancelled)));
    assert!(!e.r.join("a.txt").exists());
}

#[test]
fn a_missing_folder_is_a_clear_error_and_nothing_is_created() {
    let e = env();
    let s = settings(&e, Variant::Mirror);
    let missing = e.l.join("nope");
    let err = preview(&missing, &e.r, &s, &Progress::new()).unwrap_err();
    assert!(err.to_string().contains("nie katalog"), "{err}");
    assert!(!missing.exists());
}

#[test]
fn permanent_deletion_really_removes() {
    let e = env();
    put(&e.r, "gone.txt", "x", 0);
    let s = Settings {
        deletion: Deletion::Permanent,
        ..settings(&e, Variant::Mirror)
    };
    sync(&e, &s);
    assert!(!e.r.join("gone.txt").exists());
    assert!(!e.versions.exists());
}
