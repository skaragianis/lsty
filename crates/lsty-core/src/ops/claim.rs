//! The claim guard: who may act on a task that is underway, and what a
//! warning says when the actor is not its holder.

use std::fmt::Write as _;

use super::{
    env::Actor,
    lifecycle::ClaimOpts,
    refs::Target,
    tx::{Tx, gone},
};
use crate::{
    errs::{Code, Error, Field, Result, lossy},
    list::{Snapshot, State, Task, TaskId},
};

/// What the holder of a task that is not the actor's own is called when
/// it is the human.
const HUMAN_NAME: &[u8] = b"the human";

const CLAIMED_HINT: &str = "pick another task with lsty next; \
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

impl<'a> Guard<'a> {
    /// The guard of a metadata edit by `actor`.
    pub(super) fn edit(actor: &'a Actor) -> Self {
        Self {
            actor,
            mode: ClaimMode::Edit,
            opts: ClaimOpts::default(),
        }
    }
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

/// The `owner:` of a task, empty when it has none.
pub(super) fn owner_of<'s>(snap: &'s Snapshot, task: &Task) -> &'s [u8] {
    snap.tags(task).owner.unwrap_or_default()
}

/// Whether the task is underway and not the actor's own claim.
pub(super) fn held_by_other(
    snap: &Snapshot,
    actor: &Actor,
    task: &Task,
) -> bool {
    task.state() == State::Doing
        && owner_of(snap, task) != actor.name().as_bytes()
}

/// The holder of the task as a warning names it.
pub(super) fn holder_name<'s>(snap: &'s Snapshot, task: &Task) -> &'s [u8] {
    match owner_of(snap, task) {
        [] => HUMAN_NAME,
        owner => owner,
    }
}

/// Refuses an agent that acts on a task another actor holds, unless it
/// takes the claim over. `handle` names the task in the message.
pub(super) fn check_task_claim(
    snap: &Snapshot,
    actor: &Actor,
    task: &Task,
    handle: &str,
    opts: ClaimOpts,
) -> Result<()> {
    if actor.is_human() || opts.takeover || !held_by_other(snap, actor, task) {
        return Ok(());
    }
    Err(claimed_error(snap, task, handle))
}

impl Tx {
    /// Runs `act` on the task after the guard of `mode` has passed, then
    /// says whose claim it touched.
    pub(super) fn guarded<F>(
        &mut self,
        guard: Guard<'_>,
        id: TaskId,
        act: F,
    ) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        let Guard { actor, mode, opts } = guard;
        if mode != ClaimMode::Edit {
            let task = self.task(id).ok_or_else(gone)?;
            check_task_claim(&self.snap, actor, task, &self.handle(id), opts)?;
        }
        act(self)?;
        self.note_claim(actor, id, mode);
        Ok(())
    }

    /// [`Tx::each_task`] with every task passed through the guard.
    pub(super) fn each_guarded<F>(
        &mut self,
        targets: &[Target],
        guard: Guard<'_>,
        mut each: F,
    ) -> Result<()>
    where
        F: FnMut(&mut Self, TaskId) -> Result<()>,
    {
        self.each_task(targets, |tx, id| {
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
        F: FnMut(&mut Self, TaskId) -> Result<()>,
    {
        self.each_guarded(targets, Guard::edit(actor), each)
    }

    /// Says that the task was held by someone else.
    pub(super) fn note_claim(
        &mut self,
        actor: &Actor,
        id: TaskId,
        mode: ClaimMode,
    ) {
        let Some(task) = self
            .task(id)
            .filter(|task| held_by_other(&self.snap, actor, task))
        else {
            return;
        };
        let holder = holder_name(&self.snap, task).to_vec();
        let handle = self.handle(id);
        let phrase = claim_phrase(actor.is_human(), mode);
        self.say([handle.as_bytes(), phrase, &holder].concat());
    }
}

/// What a note says about the holder of a claim that `mode` touched.
pub(super) fn claim_phrase(human: bool, mode: ClaimMode) -> &'static [u8] {
    match (human, mode) {
        (false, ClaimMode::Edit)
        | (true, ClaimMode::Edit | ClaimMode::Guarded) => b" is claimed by ",
        (false, ClaimMode::Guarded | ClaimMode::Ends) => b" taken over from ",
        (true, ClaimMode::Ends) => b" was claimed by ",
    }
}

/// The `claimed` error for a task another actor holds.
fn claimed_error(snap: &Snapshot, task: &Task, handle: &str) -> Error {
    let (holder, owner) = match owner_of(snap, task) {
        [] => (HUMAN_NAME, Field::Null),
        name => (name, Field::Str(lossy(name))),
    };
    let mut message = format!("{handle} is claimed by {}", lossy(holder));
    let mut started = Field::Null;
    if let Some(at) = task.started() {
        write!(message, " since {}", at.strftime("%Y-%m-%dT%H:%MZ")).ok();
        started = Field::Str(at.to_string());
    }
    Error::new(Code::Claimed, message)
        .with_hint(CLAIMED_HINT)
        .with_field("owner", owner)
        .with_field("started", started)
        .with_task(snap.ref_of(task))
}
