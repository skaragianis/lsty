//! What commands do with a list. Every change goes through [`run`]: lock,
//! read again, check everything, apply everything, write. An operation
//! over several items is all-or-nothing, and its [`Outcome`] says what
//! changed and what an undo needs.

mod env;
mod id;
mod idrules;
mod ids;
mod pri;
mod refs;
mod result;
mod run;
mod staged;
mod stamps;
mod tx;
mod undo;

pub use env::{Actor, Env};
pub use id::id;
pub use ids::{IdSource, OsIds, Taken, new_id};
pub use pri::{PriPair, depri, pri, reprioritize};
pub use refs::{Target, parse_one, parse_refs, resolve};
pub use result::Outcome;
pub use run::run;
pub use staged::{ChangeKind, LineRef, Path};
pub use tx::{Picked, Tx};
pub use undo::{Change, FileRecord, UndoRecord};

#[cfg(test)]
mod id_tests;
#[cfg(test)]
mod pri_tests;
#[cfg(test)]
mod refs_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod stamps_tests;
#[cfg(test)]
mod test_util;
