//! Ids given to prerequisites a person picked while typing: the id is
//! written to the chosen task in the same write as the typed line.

use super::{
    dep::give_id,
    env::Env,
    landing::{Landing, list_changed},
    refs::Target,
    tx::Tx,
};
use crate::{
    errs::{Nr, Result, Source},
    list::{Snapshot, TaskId},
    todotxt::Id,
};

/// The id to give a prerequisite that has none, and the line the task
/// stands on in the reading last shown.
#[derive(Clone, Debug)]
pub struct Grant {
    id: Id,
    /// None once the line is not the one that was picked.
    nr: Option<Nr>,
}

impl Grant {
    /// A grant of `id` to `task` of the todo file in `snap`. `None` when
    /// the task is not in the todo file.
    pub fn new(snap: &Snapshot, task: TaskId, id: Id) -> Option<Self> {
        let task = snap.task(task).filter(|i| i.source() == Source::Todo)?;
        Some(Self {
            id,
            nr: Some(task.nr()),
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

    /// Whether the grant is of the task that stands on line `nr` now.
    pub fn is_of(&self, nr: Nr) -> bool {
        self.nr == Some(nr)
    }

    /// Takes the grant to a later reading.
    pub fn land(&mut self, landing: &Landing) {
        self.nr = self.nr.and_then(|nr| {
            let to = landing.line(Source::Todo, nr.index())?;
            Nr::from_index(to)
        });
    }
}

/// Gives every grant its id.
///
/// # Errors
///
/// `expect_mismatch` for a task whose line is not the one picked; and what
/// the claim rules refuse.
pub(super) fn grant_ids(
    tx: &mut Tx,
    env: &Env<'_>,
    grants: &[Grant],
) -> Result<()> {
    grants.iter().try_for_each(|g| {
        let nr = g.nr.ok_or_else(list_changed)?;
        let pre = tx.anchor(&Target::todo(i64::from(nr.get())))?;
        give_id(tx, env, pre, g.id.as_str())
    })
}
