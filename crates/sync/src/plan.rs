//! Turning differences into a list of actions, according to the chosen sync variant.

use std::collections::HashMap;
use std::time::Duration;

use crate::compare::{Category, Diff};
use crate::scan::{Kind, Node};
use crate::state::SyncState;

/// Which way a difference is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Make the right side match the left one (copy, or delete when the left has no such item).
    ToRight,
    /// Make the left side match the right one.
    ToLeft,
    /// Leave it as it is.
    Skip,
}

/// Direction per kind of difference, for [`Variant::Custom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    pub left_only: Direction,
    pub right_only: Direction,
    pub left_newer: Direction,
    pub right_newer: Direction,
    pub different: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Changes travel both ways; deletions too when a snapshot of the last sync exists.
    TwoWay,
    /// The right side becomes an exact copy of the left one (extra files are deleted).
    Mirror,
    /// New and newer left files go to the right; nothing is ever deleted or overwritten
    /// with an older file.
    Update,
    Custom(Rules),
}

impl Variant {
    /// The directions a variant uses when there is no snapshot to consult.
    pub fn rules(self) -> Rules {
        use Direction::*;
        match self {
            Variant::TwoWay => Rules {
                left_only: ToRight,
                right_only: ToLeft,
                left_newer: ToRight,
                right_newer: ToLeft,
                different: Skip,
            },
            Variant::Mirror => Rules {
                left_only: ToRight,
                right_only: ToRight,
                left_newer: ToRight,
                right_newer: ToRight,
                different: ToRight,
            },
            Variant::Update => Rules {
                left_only: ToRight,
                right_only: Skip,
                left_newer: ToRight,
                right_newer: Skip,
                different: ToRight,
            },
            Variant::Custom(rules) => rules,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    CopyToRight,
    CopyToLeft,
    DeleteRight,
    DeleteLeft,
    /// Differs but is left alone on purpose.
    Skip,
    /// Needs the user's decision; never executed.
    Conflict(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub path: String,
    pub op: Op,
    pub category: Category,
    /// Bytes that will be copied (0 for deletions, directories, skipped items).
    pub bytes: u64,
    /// Is this a directory (on whichever side exists)?
    pub is_dir: bool,
    /// What each side holds, for showing sizes and dates.
    pub left: Option<Node>,
    pub right: Option<Node>,
}

impl Action {
    /// The user overrides the direction of one row (FreeFileSync's per-item arrows).
    /// Rows that cannot be resolved by copying (a file against a directory) and the deletion
    /// of whole directories are refused: change the files inside instead.
    pub fn redirect(&mut self, direction: Direction) -> Result<(), String> {
        if matches!(self.category, Category::Conflict(_)) {
            return Err(
                "plik i katalog o tej samej nazwie: usuń lub zmień nazwę jednego z nich".into(),
            );
        }
        let op = op_for(direction, self.left.is_some(), self.right.is_some());
        if self.is_dir && matches!(op, Op::DeleteLeft | Op::DeleteRight) {
            return Err("usunięcie katalogu: zmień kierunek plików w środku".into());
        }
        self.bytes = copy_bytes(&op, &self.left, &self.right);
        self.op = op;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub actions: Vec<Action>,
}

impl Plan {
    /// Overrides the direction of the action with this index.
    pub fn redirect(&mut self, index: usize, direction: Direction) -> Result<(), String> {
        self.actions
            .get_mut(index)
            .ok_or_else(|| format!("nie ma pozycji {}", index + 1))?
            .redirect(direction)
    }

    pub fn count(&self, pred: impl Fn(&Op) -> bool) -> usize {
        self.actions.iter().filter(|a| pred(&a.op)).count()
    }

    pub fn copies(&self) -> usize {
        self.count(|op| matches!(op, Op::CopyToRight | Op::CopyToLeft))
    }

    pub fn deletions(&self) -> usize {
        self.count(|op| matches!(op, Op::DeleteRight | Op::DeleteLeft))
    }

    pub fn conflicts(&self) -> usize {
        self.count(|op| matches!(op, Op::Conflict(_)))
    }

    pub fn bytes(&self) -> u64 {
        self.actions.iter().map(|a| a.bytes).sum()
    }

    /// True when running the plan would change nothing.
    pub fn is_noop(&self) -> bool {
        self.copies() + self.deletions() == 0
    }
}

/// Builds the plan. `state` is the snapshot of the last sync (used by two-way only) and
/// `tolerance` the time slack used to decide whether an item changed since then.
pub fn plan(
    diffs: &[Diff],
    variant: Variant,
    state: Option<&SyncState>,
    tolerance: Duration,
) -> Plan {
    let mut actions = Vec::new();
    for diff in diffs {
        if diff.category == Category::Equal {
            continue;
        }
        let op = match (&diff.category, variant, state) {
            (Category::Conflict(why), _, _) => Op::Conflict(why.clone()),
            (_, Variant::TwoWay, Some(state)) => two_way_with_state(diff, state, tolerance),
            (category, variant, _) => {
                let rules = variant.rules();
                let direction = match category {
                    Category::LeftOnly => rules.left_only,
                    Category::RightOnly => rules.right_only,
                    Category::LeftNewer => rules.left_newer,
                    Category::RightNewer => rules.right_newer,
                    _ => rules.different,
                };
                if variant == Variant::TwoWay && category == &Category::Different {
                    Op::Conflict("ta sama data, inna zawartość".into())
                } else {
                    apply(direction, diff)
                }
            }
        };
        let bytes = copy_bytes(&op, &diff.left, &diff.right);
        actions.push(Action {
            path: diff.path.clone(),
            op,
            category: diff.category.clone(),
            bytes,
            is_dir: [diff.left, diff.right]
                .iter()
                .flatten()
                .any(|n| n.kind == Kind::Dir),
            left: diff.left,
            right: diff.right,
        });
    }
    protect_directories(&mut actions);
    Plan { actions }
}

fn apply(direction: Direction, diff: &Diff) -> Op {
    op_for(direction, diff.left.is_some(), diff.right.is_some())
}

fn op_for(direction: Direction, has_left: bool, has_right: bool) -> Op {
    match direction {
        Direction::Skip => Op::Skip,
        Direction::ToRight if has_left => Op::CopyToRight,
        Direction::ToRight => Op::DeleteRight,
        Direction::ToLeft if has_right => Op::CopyToLeft,
        Direction::ToLeft => Op::DeleteLeft,
    }
}

fn copy_bytes(op: &Op, left: &Option<Node>, right: &Option<Node>) -> u64 {
    match (op, left, right) {
        (Op::CopyToRight, Some(l), _) if l.kind == Kind::File => l.size,
        (Op::CopyToLeft, _, Some(r)) if r.kind == Kind::File => r.size,
        _ => 0,
    }
}

fn unchanged(now: &Option<Node>, then: Option<&Node>, tolerance: Duration) -> bool {
    match (now, then) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.kind == b.kind
                && (a.kind == Kind::Dir
                    || (a.size == b.size
                        && match (a.modified, b.modified) {
                            (Some(x), Some(y)) => {
                                let gap = x.duration_since(y).unwrap_or_else(|e| e.duration());
                                gap <= tolerance
                            }
                            (None, None) => true,
                            _ => false,
                        }))
        }
        _ => false,
    }
}

/// Two-way with a snapshot: an item changed on one side only travels to the other side,
/// a deletion travels when the other side did not touch the item, and changes on both
/// sides are conflicts.
fn two_way_with_state(diff: &Diff, state: &SyncState, tolerance: Duration) -> Op {
    let was_left = state.left.get(&diff.path);
    let was_right = state.right.get(&diff.path);
    let left_same = unchanged(&diff.left, was_left, tolerance);
    let right_same = unchanged(&diff.right, was_right, tolerance);
    match (&diff.left, &diff.right) {
        (Some(_), None) => {
            if was_right.is_some() && left_same {
                Op::DeleteLeft
            } else if was_right.is_some() {
                Op::Conflict("usunięte po prawej, zmienione po lewej".into())
            } else {
                Op::CopyToRight
            }
        }
        (None, Some(_)) => {
            if was_left.is_some() && right_same {
                Op::DeleteRight
            } else if was_left.is_some() {
                Op::Conflict("usunięte po lewej, zmienione po prawej".into())
            } else {
                Op::CopyToLeft
            }
        }
        (Some(_), Some(_)) => match (left_same, right_same) {
            (false, true) => Op::CopyToRight,
            (true, false) => Op::CopyToLeft,
            (true, true) => Op::Skip,
            // Both changed since the last sync: one of the edits would be lost, so the
            // user decides, even when one side is clearly later.
            (false, false) => {
                Op::Conflict("zmienione po obu stronach od ostatniej synchronizacji".into())
            }
        },
        (None, None) => Op::Skip,
    }
}

/// A directory is only deleted when everything below it is deleted on that same side;
/// otherwise the deletion would take files that are being kept or are in conflict.
fn protect_directories(actions: &mut [Action]) {
    let ops: HashMap<String, Op> = actions
        .iter()
        .map(|a| (a.path.clone(), a.op.clone()))
        .collect();
    for action in actions.iter_mut() {
        let side = match action.op {
            Op::DeleteRight | Op::DeleteLeft if action.is_dir => action.op.clone(),
            _ => continue,
        };
        let prefix = format!("{}/", action.path);
        let blocked = ops
            .iter()
            .any(|(path, op)| path.starts_with(&prefix) && *op != side);
        if blocked {
            action.op = Op::Conflict("katalog zawiera pliki, które zostają".into());
            action.bytes = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    const TOL: Duration = Duration::from_secs(2);

    fn file(size: u64, secs: u64) -> Option<Node> {
        Some(Node {
            kind: Kind::File,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)),
        })
    }

    fn dir() -> Option<Node> {
        Some(Node {
            kind: Kind::Dir,
            size: 0,
            modified: None,
        })
    }

    fn diff(path: &str, left: Option<Node>, right: Option<Node>, category: Category) -> Diff {
        Diff {
            path: path.into(),
            left,
            right,
            category,
        }
    }

    fn ops(plan: &Plan) -> Vec<(&str, &Op)> {
        plan.actions
            .iter()
            .map(|a| (a.path.as_str(), &a.op))
            .collect()
    }

    fn sample() -> Vec<Diff> {
        vec![
            diff("eq", file(1, 0), file(1, 0), Category::Equal),
            diff("lo", file(10, 0), None, Category::LeftOnly),
            diff("ro", None, file(20, 0), Category::RightOnly),
            diff("ln", file(1, 100), file(1, 0), Category::LeftNewer),
            diff("rn", file(1, 0), file(1, 100), Category::RightNewer),
            diff("df", file(1, 0), file(1, 0), Category::Different),
        ]
    }

    #[test]
    fn mirror_makes_the_right_side_a_copy() {
        let p = plan(&sample(), Variant::Mirror, None, TOL);
        assert_eq!(
            ops(&p),
            [
                ("lo", &Op::CopyToRight),
                ("ro", &Op::DeleteRight),
                ("ln", &Op::CopyToRight),
                ("rn", &Op::CopyToRight),
                ("df", &Op::CopyToRight),
            ]
        );
        assert_eq!(p.bytes(), 10 + 1 + 1 + 1);
        assert_eq!((p.copies(), p.deletions()), (4, 1));
    }

    #[test]
    fn update_never_deletes_and_never_overwrites_with_older() {
        let p = plan(&sample(), Variant::Update, None, TOL);
        assert_eq!(
            ops(&p),
            [
                ("lo", &Op::CopyToRight),
                ("ro", &Op::Skip),
                ("ln", &Op::CopyToRight),
                ("rn", &Op::Skip),
                ("df", &Op::CopyToRight),
            ]
        );
        assert_eq!(p.deletions(), 0);
    }

    #[test]
    fn two_way_without_a_snapshot_copies_and_asks_about_the_unclear() {
        let p = plan(&sample(), Variant::TwoWay, None, TOL);
        assert_eq!(
            ops(&p),
            [
                ("lo", &Op::CopyToRight),
                ("ro", &Op::CopyToLeft),
                ("ln", &Op::CopyToRight),
                ("rn", &Op::CopyToLeft),
                ("df", &Op::Conflict("ta sama data, inna zawartość".into())),
            ]
        );
        assert_eq!(p.deletions(), 0, "without memory nothing is ever deleted");
    }

    #[test]
    fn custom_rules_decide_each_category() {
        let rules = Rules {
            left_only: Direction::Skip,
            right_only: Direction::ToLeft,
            left_newer: Direction::ToLeft,
            right_newer: Direction::Skip,
            different: Direction::ToRight,
        };
        let p = plan(&sample(), Variant::Custom(rules), None, TOL);
        assert_eq!(
            ops(&p),
            [
                ("lo", &Op::Skip),
                ("ro", &Op::CopyToLeft),
                ("ln", &Op::CopyToLeft),
                ("rn", &Op::Skip),
                ("df", &Op::CopyToRight),
            ]
        );
        // ToLeft on a left-only item would delete it from the left.
        let rules = Rules {
            left_only: Direction::ToLeft,
            ..rules
        };
        let p = plan(&sample(), Variant::Custom(rules), None, TOL);
        assert_eq!(ops(&p)[0], ("lo", &Op::DeleteLeft));
    }

    #[test]
    fn kind_clashes_are_always_conflicts() {
        let d = vec![diff(
            "x",
            file(1, 0),
            dir(),
            Category::Conflict("plik i katalog".into()),
        )];
        for variant in [Variant::Mirror, Variant::Update, Variant::TwoWay] {
            assert_eq!(plan(&d, variant, None, TOL).conflicts(), 1);
        }
    }

    fn state(left: &[(&str, Option<Node>)], right: &[(&str, Option<Node>)]) -> SyncState {
        let mut s = SyncState::default();
        for (p, n) in left {
            s.left.insert(p.to_string(), n.unwrap());
        }
        for (p, n) in right {
            s.right.insert(p.to_string(), n.unwrap());
        }
        s
    }

    #[test]
    fn two_way_with_a_snapshot_propagates_deletions_and_edits() {
        let s = state(
            &[
                ("gone_left", file(1, 0)),
                ("gone_right", file(1, 0)),
                ("edit_left", file(1, 0)),
                ("edit_right", file(1, 0)),
            ],
            &[
                ("gone_left", file(1, 0)),
                ("gone_right", file(1, 0)),
                ("edit_left", file(1, 0)),
                ("edit_right", file(1, 0)),
            ],
        );
        let d = vec![
            // Deleted on the left, untouched on the right.
            diff("gone_left", None, file(1, 0), Category::RightOnly),
            // Deleted on the right, untouched on the left.
            diff("gone_right", file(1, 0), None, Category::LeftOnly),
            diff("edit_left", file(5, 500), file(1, 0), Category::LeftNewer),
            diff("edit_right", file(1, 0), file(5, 500), Category::RightNewer),
            // Never seen before.
            diff("fresh", file(1, 0), None, Category::LeftOnly),
        ];
        let p = plan(&d, Variant::TwoWay, Some(&s), TOL);
        assert_eq!(
            ops(&p),
            [
                ("gone_left", &Op::DeleteRight),
                ("gone_right", &Op::DeleteLeft),
                ("edit_left", &Op::CopyToRight),
                ("edit_right", &Op::CopyToLeft),
                ("fresh", &Op::CopyToRight),
            ]
        );
    }

    #[test]
    fn two_way_with_a_snapshot_flags_edits_on_both_sides_and_edit_versus_delete() {
        let s = state(
            &[("both", file(1, 0)), ("vs_delete", file(1, 0))],
            &[("both", file(1, 0)), ("vs_delete", file(1, 0))],
        );
        let d = vec![
            diff("both", file(2, 100), file(3, 200), Category::RightNewer),
            diff("vs_delete", None, file(9, 300), Category::RightOnly),
        ];
        let p = plan(&d, Variant::TwoWay, Some(&s), TOL);
        assert_eq!(p.conflicts(), 2, "{:?}", ops(&p));
        assert!(p.is_noop());
    }

    #[test]
    fn a_deleted_directory_is_only_removed_when_all_of_it_goes() {
        // Mirror: the right side has a directory the left lacks, all of it going away.
        let d = vec![
            diff("old", None, dir(), Category::RightOnly),
            diff("old/a", None, file(1, 0), Category::RightOnly),
        ];
        let p = plan(&d, Variant::Mirror, None, TOL);
        assert_eq!(p.deletions(), 2);

        // Custom: the directory would be deleted but one child is kept.
        let rules = Rules {
            right_only: Direction::ToRight,
            ..Variant::Update.rules()
        };
        let d = vec![
            diff("old", None, dir(), Category::RightOnly),
            diff("old/a", None, file(1, 0), Category::RightOnly),
            diff("old/b", file(2, 0), file(1, 0), Category::RightNewer),
        ];
        let p = plan(&d, Variant::Custom(rules), None, TOL);
        assert_eq!(
            p.actions[0].op,
            Op::Conflict("katalog zawiera pliki, które zostają".into())
        );
    }

    #[test]
    fn a_row_can_be_redirected_by_hand() {
        let mut p = plan(&sample(), Variant::Update, None, TOL);
        // `ro` (right only) is skipped by Update; copy it to the left instead.
        let ro = p.actions.iter().position(|a| a.path == "ro").unwrap();
        p.redirect(ro, Direction::ToLeft).unwrap();
        assert_eq!(p.actions[ro].op, Op::CopyToLeft);
        assert_eq!(p.actions[ro].bytes, 20);
        p.redirect(ro, Direction::ToRight).unwrap();
        assert_eq!(p.actions[ro].op, Op::DeleteRight);
        assert_eq!(p.actions[ro].bytes, 0);
        p.redirect(ro, Direction::Skip).unwrap();
        assert_eq!(p.actions[ro].op, Op::Skip);
        assert!(p.redirect(99, Direction::Skip).is_err());
    }

    #[test]
    fn directories_and_kind_clashes_refuse_risky_overrides() {
        let d = vec![
            diff("clash", file(1, 0), dir(), Category::Conflict("x".into())),
            diff("old", None, dir(), Category::RightOnly),
        ];
        let mut p = plan(&d, Variant::Update, None, TOL);
        assert!(p.redirect(0, Direction::ToRight).is_err());
        assert!(
            p.redirect(1, Direction::ToRight).is_err(),
            "no whole-directory delete"
        );
        assert!(
            p.redirect(1, Direction::ToLeft).is_ok(),
            "copying it over is fine"
        );
    }
}
