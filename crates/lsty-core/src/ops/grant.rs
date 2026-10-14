//! Ids given to prerequisites a person picked while typing: the id is
//! written to the chosen item in the same write as the typed line.

use super::{
    dep::give_id,
    env::Env,
    expect::squash_bytes,
    pin::Seen,
    refs::{Target, no_item},
    tx::{Tx, wrong_state},
};
use crate::{
    errs::{Error, Field, Nr, Result, Source, lossy},
    list::{ItemId, Snapshot},
    todotxt::Id,
};

/// The id to give a prerequisite that has none. The item is found
/// wherever it stands now when its line did not change, and between its
/// neighbours when its line changed; it is gone when they cannot say.
#[derive(Clone, Debug)]
pub struct Grant {
    id: Id,
    nr: Nr,
    at: usize,
    seen: Seen,
}

impl Grant {
    /// A grant of `id` to `item` of the todo file in `snap`, whose lines
    /// `seen` holds. `None` when the item is not in the todo file, and
    /// when `seen` is not of the reading `snap`.
    pub fn new(
        snap: &Snapshot,
        seen: &Seen,
        item: ItemId,
        id: Id,
    ) -> Option<Self> {
        if !seen.is_of(snap) {
            return None;
        }
        let item = snap.item(item).filter(|i| i.source() == Source::Todo)?;
        let at = item.nr().index();
        seen.line(at)?;
        Some(Self {
            id,
            nr: item.nr(),
            at,
            seen: seen.clone(),
        })
    }

    /// The id the item is given.
    pub const fn id(&self) -> &Id {
        &self.id
    }

    /// Gives another id.
    pub fn set_id(&mut self, id: Id) {
        self.id = id;
    }

    /// Whether the grant is of the item that stands on line `nr` with
    /// the text `line` now: a line that changed or is gone is not.
    pub fn is_of(&self, nr: Nr, line: &[u8]) -> bool {
        self.nr == nr && self.base() == line
    }

    /// The line the item stands on, as the caller last saw it.
    pub const fn nr(&self) -> Nr {
        self.nr
    }

    /// Tells the grant where the caller sees the item now, which
    /// messages name.
    pub const fn moved_to(&mut self, nr: Nr) {
        self.nr = nr;
    }

    /// The line of the item as it was picked.
    pub(crate) fn base(&self) -> &[u8] {
        self.seen.line(self.at).unwrap_or_default()
    }
}

/// Gives every grant its id.
///
/// # Errors
///
/// `no_item` for an item that is gone and `wrong_state` for one that
/// has an id now, each with the id of the grant as `chosen`; and what
/// the claim rules refuse. An item that changed meanwhile gets its id
/// all the same, and the write says so.
pub(super) fn grant_ids(
    tx: &mut Tx,
    env: &Env<'_>,
    grants: &[Grant],
) -> Result<()> {
    grants.iter().try_for_each(|g| grant_id(tx, env, g))
}

fn grant_id(tx: &mut Tx, env: &Env<'_>, g: &Grant) -> Result<()> {
    let (pre, changed) = prerequisite(tx, g)
        .and_then(|pre| check_id(tx, pre, g).map(|c| (pre, c)))
        .map_err(|e| e.with_field("chosen", Field::Str(g.id.to_string())))?;
    let handle = tx.handle(pre);
    let first = tx.messages.len();
    give_id(tx, env, pre, g.id.as_str())?;
    if changed {
        // One clause in place of "is id:", the first thing `give_id` says;
        // what follows it, a claim note, stays.
        tx.messages[first] =
            format!("{handle} changed meanwhile and got id:{}", g.id).into();
    }
    Ok(())
}

/// The item of the grant, where it is now.
fn prerequisite(tx: &mut Tx, g: &Grant) -> Result<ItemId> {
    let now = &tx.snapshot().todo().lines;
    let at = g.seen.find(now, g.at).ok_or_else(|| gone(g))?;
    tx.anchor(&Target::todo_index(at))
}

/// The refusal for an item that is not there.
fn gone(g: &Grant) -> Error {
    no_item(&Target::todo(i64::from(g.nr.get())))
}

/// Refuses an item that got an id meanwhile. Whether its line changed.
fn check_id(tx: &Tx, pre: ItemId, g: &Grant) -> Result<bool> {
    let snap = tx.snapshot();
    let Some(item) = tx.item(pre) else {
        return Err(gone(g));
    };
    if let Some(have) = snap.id_of(item) {
        let have = lossy(have);
        let message = format!("the prerequisite has id:{have} now");
        let e = wrong_state(snap, item, item.state(), message, String::new());
        return Err(e.with_field("id", Field::Str(have)));
    }
    Ok(squash_bytes(snap.line(item).raw()) != squash_bytes(g.base()))
}
