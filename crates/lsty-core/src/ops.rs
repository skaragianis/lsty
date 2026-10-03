//! What commands do with a list. Every change goes through [`run`]: lock,
//! read again, check everything, apply everything, write. An operation
//! over several items is all-or-nothing, and its [`Outcome`] says what
//! changed and what an undo needs.

mod add;
mod archive;
mod cascade;
mod claim;
mod cycle;
mod del;
mod dep;
mod due;
mod env;
mod expect;
mod finish;
mod id;
mod idrules;
mod ids;
mod ifabsent;
mod lifecycle;
mod moves;
mod newline;
mod order;
mod pri;
mod refs;
mod reopen;
mod restore;
mod result;
mod rules;
mod run;
mod spawn;
mod staged;
mod stamps;
mod start;
mod stop;
mod tag;
mod text;
mod tokens;
mod tx;
mod undo;

pub use add::{AddParams, IdChoice, Place, add};
pub use archive::archive;
pub use del::del;
pub use dep::{dep, undep};
pub use due::due;
pub use env::{Actor, Env};
pub use expect::{Expect, squash};
pub use finish::finish;
pub use id::id;
pub use ids::{IdSource, OsIds, Taken, new_id};
pub use lifecycle::{ClaimOpts, DoOpts, StartOpts};
pub use order::{Placement, deduplicate, down, reorder, up};
pub use pri::{PriPair, depri, pri, reprioritize};
pub use refs::{Target, parse_one, parse_refs, resolve};
pub use reopen::reopen;
pub use restore::restore;
pub use result::Outcome;
pub use run::run;
pub use spawn::spawn;
pub use staged::{ChangeKind, LineRef, Path};
pub use start::start;
pub use stop::stop;
pub use tag::{tag, untag};
pub use text::{append, del_term, prepend, replace};
pub use tx::{Picked, Tx};
pub use undo::{Change, FileRecord, UndoRecord};

#[cfg(test)]
mod add_tests;
#[cfg(test)]
mod claims_tests;
#[cfg(test)]
mod dep_tests;
#[cfg(test)]
mod expect_tests;
#[cfg(test)]
mod id_tests;
#[cfg(test)]
mod ifabsent_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod moves_tests;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod pri_tests;
#[cfg(test)]
mod refs_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod spawn_tests;
#[cfg(test)]
mod stamps_tests;
#[cfg(test)]
mod test_util;
#[cfg(test)]
mod text_tests;
