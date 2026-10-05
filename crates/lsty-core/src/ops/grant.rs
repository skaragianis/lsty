//! Ids given to prerequisites a person picked while typing: the id is
//! written to the chosen item in the same write as the typed line.

use super::dep::give_id;
use super::env::Env;
use super::expect::squash_bytes;
use super::pin::Seen;
use super::refs::{Target, no_item};
use super::tx::{Picked, Tx, wrong_state};
use crate::errs::{Code, Error, Field, Nr, Result, lossy};
use crate::list::{ItemId, Snapshot};
use crate::todotxt::Id;

/// The id to give a prerequisite that has none, when the item still
/// reads as it did when it was picked. The item is found wherever it
/// stands now.
#[derive(Clone, Debug)]
pub struct Grant {
    id: Id,
    nr: Nr,
    base: Vec<u8>,
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
        let item = snap.item(item)?;
        let at = item.nr().index();
        let base = seen.line(at)?.to_vec();
        (item.source() == crate::errs::Source::Todo).then_some(Self {
            id,
            nr: item.nr(),
            base,
            at,
            seen: seen.clone(),
        })
    }

    /// The id the item is given.
    pub const fn id(&self) -> &Id {
        &self.id
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
    pub fn base(&self) -> &[u8] {
        &self.base
    }
}

/// Gives every grant its id.
///
/// # Errors
///
/// `no_item` for an item that is gone, `wrong_state` for one that has
/// an id now, `expect_mismatch` for one that changed, and what the
/// claim rules refuse. Each carries the id of the grant as `chosen`.
pub(super) fn grant_ids(
    tx: &mut Tx,
    env: &Env<'_>,
    grants: &[Grant],
) -> Result<()> {
    grants.iter().try_for_each(|g| grant_id(tx, env, g))
}

fn grant_id(tx: &mut Tx, env: &Env<'_>, g: &Grant) -> Result<()> {
    let pre = prerequisite(tx, g).map_err(|e| for_grant(e, g))?;
    check_state(tx, pre, g)?;
    give_id(tx, env, pre, g.id.as_str())
}

/// The item of the grant, where it is now.
fn prerequisite(tx: &mut Tx, g: &Grant) -> Result<ItemId> {
    let now = &tx.snapshot().todo().lines;
    let at = g.seen.follow(now, g.at).ok_or_else(|| gone(g))?;
    let target = Target::todo(i64::try_from(at + 1).unwrap_or(i64::MAX));
    match tx.target(&target) {
        Ok(Picked::Item(id)) => Ok(id),
        Ok(Picked::Gone(_)) => Err(no_item(&target)),
        Err(e) => Err(e),
    }
}

/// The refusal for an item that is not there.
fn gone(g: &Grant) -> Error {
    no_item(&Target::todo(i64::from(g.nr.get())))
}

/// Refuses an item that got an id meanwhile or that changed.
fn check_state(tx: &Tx, pre: ItemId, g: &Grant) -> Result<()> {
    let snap = tx.snapshot();
    let Some(item) = tx.item(pre) else {
        return Err(for_grant(gone(g), g));
    };
    if let Some(have) = snap.id_of(item).filter(|id| !id.is_empty()) {
        let have = lossy(have);
        let message = format!("the prerequisite has id:{have} now");
        let e = wrong_state(snap, item, item.state(), message, String::new());
        return Err(e
            .with_field("id", Field::Str(have))
            .with_field("chosen", Field::Str(g.id.to_string())));
    }
    let raw = snap.line(item).raw();
    if squash_bytes(raw) == squash_bytes(&g.base) {
        return Ok(());
    }
    Err(for_grant(mismatch(snap.ref_of(item), raw, g), g))
}

fn mismatch(item: crate::errs::Ref, actual: &[u8], g: &Grant) -> Error {
    let (actual, expected) = (lossy(actual), lossy(&g.base));
    let message =
        format!("item {} is \"{actual}\", expected \"{expected}\"", g.nr);
    Error::new(Code::ExpectMismatch, message)
        .with_item(item)
        .with_field("expected", Field::Str(expected))
        .with_field("actual", Field::Str(actual))
}

/// Names the id a refusal is about.
fn for_grant(e: Error, g: &Grant) -> Error {
    e.with_field("chosen", Field::Str(g.id.to_string()))
}
