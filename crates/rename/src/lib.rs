//! Multi-rename: build new names from masks, search and replace and a counter, show the
//! result as a [`Plan`], and only then apply it.
//!
//! Nothing touches the disk until [`apply`]. The apply step never replaces an existing
//! file, goes through temporary names so that swaps and chains (`a -> b`, `b -> c`) work,
//! and rolls back when something fails halfway.

mod apply;
mod diff;
mod mask;
mod meta;
mod plan;

pub use apply::{apply, undo, Renamed};
pub use diff::{changed_spans, Span};
pub use meta::{read_meta, Meta};
pub use plan::{needs_meta, plan, CaseMode, Item, Plan, Row, Settings, Stamp, Status};
