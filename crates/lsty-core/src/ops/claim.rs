//! The claim guard: who may act on an item that is underway, and what a
//! warning says when the actor is not its holder.

use std::fmt::Write as _;

use super::{env::Actor, lifecycle::ClaimOpts, refs::Target, tx::Tx};
use crate::{
    errs::{Code, Error, Field, Result},
    list::{Item, ItemId, Snapshot, State},
};

/// What the holder of an item that is not the actor's own is called when
/// it is the human.
const HUMAN_NAME: &[u8] = b"the human";

const CLAIMED_HINT: &str = "pick another item with lsty next; \
                            pass --takeover only when told to";

/// Who acts, how the claim of another is treated and whether the actor
/// takes it over.
#[derive(Clone, Copy, Debug)]
pub(super) struct Guard<'a> {
    /// Who acts.
    pub(super) actor: &'a Actor,
    /// How a claim of another is treated.
    pub(super) mode: ClaimMode,
    /// The claim flags.
    pub(super) opts: ClaimOpts,
}

/// How an operation treats a claim held by another actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClaimMode {
    /// A metadata edit: allowed for everyone, with a warning.
    Edit,
    /// Refuses an agent without `takeover`; the human is warned as for an
    /// edit.
    Guarded,
    /// Ends or takes the claim: refuses an agent without `takeover`; the
    /// human is told the claim was theirs to end.
    Ends,
}

/// The `owner:` of an item, empty when it has none.
fn owner_of(snap: &Snapshot, item: &Item) -> Vec<u8> {
    snap.tags(item).owner.unwrap_or_default().to_vec()
}

/// Whether the item is underway and not the actor's own claim.
fn held_by_other(snap: &Snapshot, actor: &Actor, item: &Item) -> bool {
    item.state() == State::Doing
        && owner_of(snap, item) != actor.name().as_bytes()
}

/// The holder of the item as a warning names it.
fn holder_name(snap: &Snapshot, item: &Item) -> Vec<u8> {
    let owner = owner_of(snap, item);
    if owner.is_empty() {
        HUMAN_NAME.to_vec()
    } else {
        owner
    }
}

/// Refuses an agent that acts on an item another actor holds, unless it
/// takes the claim over. `handle` names the item in the message.
pub(super) fn check_item_claim(
    snap: &Snapshot,
    actor: &Actor,
    item: &Item,
    handle: &str,
    opts: ClaimOpts,
) -> Result<()> {
    if actor.is_human() || opts.takeover || !held_by_other(snap, actor, item) {
        return Ok(());
    }
    Err(claimed_error(snap, item, handle))
}

impl Tx {
    /// Whether the item is underway and not the actor's own claim.
    pub(super) fn held_by_other(&self, actor: &Actor, id: ItemId) -> bool {
        self.item(id)
            .is_some_and(|item| held_by_other(&self.snap, actor, item))
    }

    /// The holder of the item as a warning names it.
    pub(super) fn holder_name(&self, id: ItemId) -> Vec<u8> {
        let Some(item) = self.item(id) else {
            return HUMAN_NAME.to_vec();
        };
        holder_name(&self.snap, item)
    }

    /// Refuses an agent that acts on an item another actor holds, unless
    /// it takes the claim over.
    ///
    /// # Errors
    ///
    /// `claimed`, with the holder and the time of the claim.
    pub(super) fn check_claim(
        &self,
        actor: &Actor,
        id: ItemId,
        opts: ClaimOpts,
    ) -> Result<()> {
        let Some(item) = self.item(id) else {
            return Ok(());
        };
        check_item_claim(&self.snap, actor, item, &self.handle(id), opts)
    }

    /// Runs `act` on the item after the guard of `mode` has passed, then
    /// says whose claim it touched.
    pub(super) fn guarded<F>(
        &mut self,
        guard: Guard<'_>,
        id: ItemId,
        act: F,
    ) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        let Guard { actor, mode, opts } = guard;
        if mode != ClaimMode::Edit {
            self.check_claim(actor, id, opts)?;
        }
        act(self)?;
        self.note_claim(actor, id, mode);
        Ok(())
    }

    /// [`Tx::each_item`] with every item passed through the guard.
    pub(super) fn each_guarded<F>(
        &mut self,
        targets: &[Target],
        guard: Guard<'_>,
        mut each: F,
    ) -> Result<()>
    where
        F: FnMut(&mut Self, ItemId) -> Result<()>,
    {
        self.each_item(targets, |tx, id| {
            tx.guarded(guard, id, |tx| each(tx, id))
        })
    }

    /// [`Tx::each_guarded`] for a metadata edit.
    pub(super) fn each_edit<F>(
        &mut self,
        targets: &[Target],
        actor: &Actor,
        each: F,
    ) -> Result<()>
    where
        F: FnMut(&mut Self, ItemId) -> Result<()>,
    {
        let guard = Guard {
            actor,
            mode: ClaimMode::Edit,
            opts: ClaimOpts::default(),
        };
        self.each_guarded(targets, guard, each)
    }

    /// Says that the item was held by someone else.
    pub(super) fn note_claim(
        &mut self,
        actor: &Actor,
        id: ItemId,
        mode: ClaimMode,
    ) {
        if !self.held_by_other(actor, id) {
            return;
        }
        let (handle, holder) = (self.handle(id), self.holder_name(id));
        let phrase: &[u8] = match (actor.is_human(), mode) {
            (false, ClaimMode::Edit)
            | (true, ClaimMode::Edit | ClaimMode::Guarded) => {
                b" is claimed by "
            }
            (false, ClaimMode::Guarded | ClaimMode::Ends) => {
                b" taken over from "
            }
            (true, ClaimMode::Ends) => b" was claimed by ",
        };
        self.say([handle.as_bytes(), phrase, &holder].concat());
    }
}

/// The `claimed` error for an item another actor holds.
fn claimed_error(snap: &Snapshot, item: &Item, handle: &str) -> Error {
    let holder =
        String::from_utf8_lossy(&holder_name(snap, item)).into_owned();
    let mut message = format!("{handle} is claimed by {holder}");
    let owner = owner_of(snap, item);
    let owner = if owner.is_empty() {
        Field::Null
    } else {
        Field::Str(String::from_utf8_lossy(&owner).into_owned())
    };
    let mut started = Field::Null;
    if let Some(at) = item.started() {
        write!(message, " since {}", at.strftime("%Y-%m-%dT%H:%MZ")).ok();
        started = Field::Str(at.to_string());
    }
    Error::new(Code::Claimed, message)
        .with_hint(CLAIMED_HINT)
        .with_field("owner", owner)
        .with_field("started", started)
        .with_item(snap.ref_of(item))
}
