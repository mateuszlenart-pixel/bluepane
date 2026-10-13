//! Folder comparison and synchronization (the engine behind the FreeFileSync-style dialog).
//!
//! The work is split in steps the user can inspect: [`scan`] both folders, [`compare`] them
//! into a list of differences, build a plan from a sync variant, and only then execute it.
//! Nothing is written before the plan is shown and confirmed.

mod compare;
mod filter;
mod plan;
mod scan;
mod state;

pub use compare::{compare, Category, CompareBy, Diff};
pub use filter::Filter;
pub use plan::{plan, Action, Direction, Op, Plan, Rules, Variant};
pub use scan::{scan, Kind, Node, Tree};
pub use state::{SyncState, STATE_FILE};
