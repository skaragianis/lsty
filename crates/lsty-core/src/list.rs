//! Both files of a list read at one moment, with the state that no single
//! line says: whether an item is blocked and by what, dangling
//! prerequisites, dependency cycles, later, overdue and stale.
//!
//! [`load`] reads both files under one shared lock and [`Snapshot::build`]
//! derives the state from what it is given. A [`Snapshot`] owns its files
//! and its items: an [`Item`] keeps its position and the values derived
//! for it, and asks the snapshot for its line and tags.

mod build;
mod cycle;
mod ids;
mod load;
mod snapshot;

pub use load::load;
pub use snapshot::{Claim, Item, ItemId, Params, Snapshot, State};

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod ids_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod test_util;
