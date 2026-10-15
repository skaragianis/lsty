//! Ids given to prerequisites a person picked while typing: the id is
//! written to the chosen task in the same write as the typed line.

use super::{
    dep::give_id,
    env::Env,
    expect::squash_bytes,
    pin::Seen,
    refs::{Target, no_task},
    tx::{Tx, wrong_state},
};
use crate::{
    errs::{Error, Field, Nr, Result, Source, lossy},
    list::{Snapshot, TaskId},
    todotxt::Id,
};

/// The id to give a prerequisite that has none. The task is found
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
    /// A grant of `id` to `task` of the todo file in `snap`, whose lines
    /// `seen` holds. `None` when the task is not in the todo file, and
    /// when `seen` is not of the reading `snap`.
    pub fn new(
        snap: &Snapshot,
        seen: &Seen,
        task: TaskId,
        id: Id,
    ) -> Option<Self> {
        if !seen.is_of(snap) {
            return None;
        }
        let task = snap.task(task).filter(|i| i.source() == Source::Todo)?;
        let at = task.nr().index();
        seen.line(at)?;
        Some(Self {
            id,
            nr: task.nr(),
            at,
            seen: seen.clone(),
        })
    }

    /// The id the task is given.
    pub const fn id(&self) -> &Id {
        &self.id
    }

    /// Gives another id.
    pub fn set_id(&mut self, id: Id) {
        self.id = id;
    }

    /// Whether the grant is of the task that stands on line `nr` with
    /// the text `line` now: a line that changed or is gone is not.
    pub fn is_of(&self, nr: Nr, line: &[u8]) -> bool {
        self.nr == nr && self.base() == line
    }

    /// The line the task stands on, as the caller last saw it.
    pub const fn nr(&self) -> Nr {
        self.nr
    }

    /// Tells the grant where the caller sees the task now, which
    /// messages name.
    pub const fn moved_to(&mut self, nr: Nr) {
        self.nr = nr;
    }

    /// The line of the task as it was picked.
    pub(crate) fn base(&self) -> &[u8] {
        self.seen.line(self.at).unwrap_or_default()
    }
}

/// Gives every grant its id.
///
/// # Errors
///
/// `no_task` for a task that is gone and `wrong_state` for one that
/// has an id now, each with the id of the grant as `chosen`; and what
/// the claim rules refuse. A task that changed meanwhile gets its id
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

/// The task of the grant, where it is now.
fn prerequisite(tx: &mut Tx, g: &Grant) -> Result<TaskId> {
    let now = &tx.snapshot().todo().lines;
    let at = g.seen.find(now, g.at).ok_or_else(|| gone(g))?;
    tx.anchor(&Target::todo_index(at))
}

/// The refusal for a task that is not there.
fn gone(g: &Grant) -> Error {
    no_task(&Target::todo(i64::from(g.nr.get())))
}

/// Refuses a task that got an id meanwhile. Whether its line changed.
fn check_id(tx: &Tx, pre: TaskId, g: &Grant) -> Result<bool> {
    let snap = tx.snapshot();
    let Some(task) = tx.task(pre) else {
        return Err(gone(g));
    };
    if let Some(have) = snap.id_of(task) {
        let have = lossy(have);
        let message = format!("the prerequisite has id:{have} now");
        let e = wrong_state(snap, task, task.state(), message, String::new());
        return Err(e.with_field("id", Field::Str(have)));
    }
    Ok(squash_bytes(snap.line(task).raw()) != squash_bytes(g.base()))
}
