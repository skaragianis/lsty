//! What commands do with a list. Every change goes through one run: lock,
//! read again, check everything, apply everything, write. An operation
//! over several items is all-or-nothing, and its [`Outcome`] says what
//! changed and, when asked, what an undo needs.

mod add;
mod archive;
mod cascade;
mod claim;
mod cycle;
mod del;
mod dep;
mod due;
mod edit;
mod env;
mod expect;
mod finish;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
mod grant;
mod id;
mod idrules;
mod ids;
mod ifabsent;
mod lifecycle;
mod merge;
mod moves;
mod newline;
mod next;
mod order;
mod paste;
mod pin;
mod pinned;
mod pri;
mod refs;
mod relink;
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
mod typed;
mod undo;
mod undo_apply;

pub use add::{AddParams, IdChoice, Place, add};
pub use archive::archive;
pub use del::del;
pub use dep::{dep, undep};
pub use due::due;
pub use edit::{EditPlan, edit, joined_lines, prepare_edit};
pub use env::{Actor, Env, Toggle};
pub use expect::squash;
pub use finish::finish;
pub use grant::Grant;
pub use id::id;
pub use ids::{IdSource, OsIds, fresh_id};
pub use lifecycle::{ClaimOpts, DoOpts, StartOpts};
pub use next::{NextParams, Reading, Started, next_read, next_start};
pub use order::{Placement, deduplicate, down, reorder, up};
pub use paste::{PasteLine, Relink, paste};
pub use pin::{Pin, Seen, Settle, Spot, Tracked};
pub use pinned::{Conflict, Edited, edit_pinned};
pub use pri::{PriPair, depri, parse_letter, pri};
pub use refs::{Target, parse_one, parse_refs, resolve};
pub use reopen::reopen;
pub use restore::restore;
pub use result::Outcome;
pub use staged::ChangeKind;
pub use start::start;
pub use stop::stop;
pub use tag::{tag, untag};
pub use text::{append, del_term, prepend, replace};
pub use undo::{Change, FileRecord, UndoRecord};
pub use undo_apply::{Followed, Tracking, redo, undo};

#[cfg(test)]
use {
    ids::Taken,
    run::run,
    staged::Path,
    tx::{Picked, Tx},
};

#[cfg(test)]
mod add_tests;
#[cfg(test)]
mod claims_tests;
#[cfg(test)]
mod dep_tests;
#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod expect_tests;
#[cfg(test)]
mod grant_tests;
#[cfg(test)]
mod id_tests;
#[cfg(test)]
mod ifabsent_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod moves_tests;
#[cfg(test)]
mod next_tests;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod paste_tests;
#[cfg(test)]
mod pin_tests;
#[cfg(test)]
mod pinned_tests;
#[cfg(test)]
mod pri_tests;
#[cfg(test)]
mod refs_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod spawn_tests;
#[cfg(test)]
mod staged_tests;
#[cfg(test)]
mod stamps_tests;
#[cfg(test)]
mod test_util;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod typed_tests;
#[cfg(test)]
mod undo_apply_tests;
