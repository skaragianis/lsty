//! Both files of a list read at one moment, with the state that no single
//! line says: whether an item is blocked and by what, dangling
//! prerequisites, dependency cycles, later, overdue and stale.
//!
//! [`load()`] reads both files under one shared lock and [`Snapshot::build`]
//! derives the state from what it is given. A [`Snapshot`] owns its files
//! and its items: an [`Item`] keeps its position and the values derived
//! for it, and asks the snapshot for its line and tags.

mod build;
mod counts;
mod cycle;
mod ids;
mod load;
mod names;
mod snapshot;
mod sort;

pub use counts::Count;
pub(crate) use ids::no_item;
pub use load::load;
pub use snapshot::{Claim, Item, ItemId, Params, Snapshot, State};
pub use sort::SortField;

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod counts_tests;
#[cfg(test)]
mod ids_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod names_tests;
#[cfg(test)]
mod sort_tests;
#[cfg(any(test, feature = "fuzzing"))]
#[expect(
    clippy::expect_used,
    reason = "the fixture fails by panicking, in the fuzz target too"
)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the fuzz target takes part")
)]
pub(crate) mod test_util;
