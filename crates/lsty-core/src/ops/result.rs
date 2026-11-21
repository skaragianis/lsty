//! What an operation reports.

use std::{collections::HashSet, sync::Arc};

use super::{
    env::Env,
    run::Sealed,
    staged::{LineRef, Row, SealedFile},
    tx::Tx,
    undo::UndoRecord,
};
use crate::{
    errs::{Msg, Nr, Part, Place, Source},
    list::{Snapshot, Task, TaskId},
};

/// The message of an operation that changed nothing and said nothing.
const NOTHING_CHANGED: &str = "nothing changed";

/// What an operation did. The ids in it belong to the list the
/// operation read (`before`) or to the list it left (all the others), so
/// a caller can render the tasks from either list.
#[derive(Debug)]
pub struct Outcome {
    tasks: Vec<TaskId>,
    before: Vec<TaskId>,
    spawned: Vec<TaskId>,
    changed: Vec<TaskId>,
    renumbered: bool,
    messages: Vec<Msg>,
    no_op: bool,
    undo: Option<UndoRecord>,
    read: Snapshot,
    after: Arc<Snapshot>,
}

impl Outcome {
    /// The lines the operation is about, as they are after it, in the
    /// order of the targets. They point into [`Outcome::after`].
    pub fn tasks(&self) -> &[TaskId] {
        &self.tasks
    }

    /// The primary lines as they were read, in the order of the
    /// targets; an added line has none. They point into
    /// [`Outcome::read`].
    pub fn before(&self) -> &[TaskId] {
        &self.before
    }

    /// Lines created as a by-product, in file order. They point into
    /// [`Outcome::after`].
    pub fn spawned(&self) -> &[TaskId] {
        &self.spawned
    }

    /// Lines changed as a side effect, in file order. They point into
    /// [`Outcome::after`].
    pub fn changed(&self) -> &[TaskId] {
        &self.changed
    }

    /// Whether a line the operation was not about has another number.
    pub const fn renumbered(&self) -> bool {
        self.renumbered
    }

    /// What the operation said, then the warnings about touched tasks,
    /// as the command line prints it.
    pub fn messages(&self) -> Vec<Vec<u8>> {
        self.messages.iter().map(Msg::cli).collect()
    }

    /// What the operation said as data: each message names the tasks it
    /// is about, by their line in the list the write left, or by their
    /// line as read for a task the write removed.
    pub fn said(&self) -> &[Msg] {
        &self.messages
    }

    /// Whether nothing was changed.
    pub const fn no_op(&self) -> bool {
        self.no_op
    }

    /// What an undo needs. `None` for a no-op, and for an operation
    /// that ran without [`Env::record_undo`](super::Env::record_undo).
    #[cfg(test)]
    pub(super) const fn undo(&self) -> Option<&UndoRecord> {
        self.undo.as_ref()
    }

    /// Takes the undo record out of the outcome.
    pub const fn take_undo(&mut self) -> Option<UndoRecord> {
        self.undo.take()
    }

    /// The lists as the operation left them, moved out of the outcome.
    pub fn into_after(self) -> Snapshot {
        Arc::unwrap_or_clone(self.after)
    }

    /// The lists as the operation read them, which `before` points
    /// into.
    pub const fn read(&self) -> &Snapshot {
        &self.read
    }

    /// The lists as the operation left them, which `tasks`, `spawned`
    /// and `changed` point into.
    pub fn after(&self) -> &Snapshot {
        &self.after
    }

    /// The tasks of `before`.
    #[cfg(test)]
    pub(super) fn tasks_before(&self) -> impl Iterator<Item = &Task> {
        self.before.iter().filter_map(|id| self.read.task(*id))
    }

    /// The tasks of `tasks`.
    pub fn tasks_after(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter_map(|id| self.after.task(*id))
    }
}

impl Sealed {
    /// Builds the result of the operation, which wrote `written`.
    pub(super) fn report(
        self,
        env: &Env<'_>,
        action: &str,
        written: &[Source],
    ) -> Outcome {
        let Self { mut tx, todo, done } = self;
        let after = tx.snapshot_after(env);
        let no_op = !todo.changed && !done.changed;
        let tasks = tx.tasks_after(&after, &tx.primary);
        let spawned = sorted(tx.tasks_after(&after, &tx.spawned));
        let changed = sorted(tx.tasks_after(&after, &tx.side));
        let touched = [&tasks, &spawned, &changed]
            .into_iter()
            .flatten()
            .filter_map(|id| after.task(*id));
        let mut messages = tx.messages.clone();
        messages
            .extend(after.warnings_for(touched).into_iter().map(Msg::from));
        if no_op && tx.messages.is_empty() {
            messages.push(NOTHING_CHANGED.into());
        }
        tx.place_names(&mut messages);
        let undo = (env.record_undo && !no_op)
            .then(|| tx.undo(action, written, (&todo, &done), &after));
        Outcome {
            before: tx.tasks_before(),
            renumbered: tx.renumbered(),
            tasks,
            spawned,
            changed,
            messages,
            no_op,
            undo,
            read: tx.snap,
            after,
        }
    }
}

impl Tx {
    /// The tasks the lines of `refs` are in `after`.
    pub(super) fn tasks_after(
        &self,
        after: &Snapshot,
        refs: &[LineRef],
    ) -> Vec<TaskId> {
        let todo = self.todo.positions();
        let done = self.done.positions();
        refs.iter()
            .filter_map(|r| {
                let at = r.source.pick(&todo, &done);
                let at = at.get(r.index).copied().flatten()?;
                after.at(r.source, Nr::from_index(at)?).map(Task::task_id)
            })
            .collect()
    }

    /// Gives the names in `messages` the line their row has in the list
    /// the write leaves, from one lookup of the positions of each file; a
    /// row the write removed or blanked keeps the line it was read at.
    fn place_names(&self, messages: &mut [Msg]) {
        let (todo, done) = (self.todo.positions(), self.done.positions());
        let names =
            messages
                .iter_mut()
                .flat_map(Msg::parts_mut)
                .filter_map(|part| match part {
                    Part::Name(name) => Some(name),
                    Part::Text(_) => None,
                });
        for name in names {
            let Some((source, index)) = name.row.take() else {
                continue;
            };
            let gone =
                self.file(source).rows().get(index).is_none_or(Row::is_gone);
            let at = source.pick(&todo, &done).get(index).copied().flatten();
            if let (false, Some(nr)) = (gone, at.and_then(Nr::from_index)) {
                name.place = Some(Place::Written(nr));
            }
        }
    }

    /// The tasks the primary lines were when read.
    fn tasks_before(&self) -> Vec<TaskId> {
        self.primary
            .iter()
            .filter_map(|r| self.file(r.source).rows().get(r.index)?.was())
            .collect()
    }

    /// Whether a line that was read, and is not primary, has another
    /// number.
    fn renumbered(&self) -> bool {
        [Source::Todo, Source::Done].into_iter().any(|source| {
            let skip: HashSet<usize> = self
                .primary
                .iter()
                .filter(|r| r.source == source)
                .map(|r| r.index)
                .collect();
            self.file(source).shifted(&skip)
        })
    }

    /// The lines of the changes the operation made itself: its primary
    /// lines and every line of the tasks they are, in either file, so an
    /// task sent to the other file is one.
    fn own_lines(&self) -> Vec<LineRef> {
        if let Some(own) = &self.own {
            return own.clone();
        }
        let tasks: HashSet<_> = self.tasks_before().into_iter().collect();
        let of_tasks =
            [Source::Todo, Source::Done].into_iter().flat_map(|source| {
                let rows = self.file(source).rows().iter().enumerate();
                rows.filter(|(_, row)| {
                    row.was().is_some_and(|id| tasks.contains(&id))
                })
                .map(move |(index, _)| LineRef { source, index })
            });
        self.primary.iter().copied().chain(of_tasks).collect()
    }

    fn undo(
        &self,
        action: &str,
        written: &[Source],
        (todo, done): (&SealedFile, &SealedFile),
        after: &Snapshot,
    ) -> UndoRecord {
        let own = self.own_lines();
        let mut files: Vec<_> = written
            .iter()
            .map(|&s| {
                let (before, now) = (self.snap.file(s), after.file(s));
                let rows = own.iter().filter(|r| r.source == s);
                self.file(s).record(
                    before,
                    now,
                    s.pick(&todo.bytes, &done.bytes),
                    &rows.map(|r| r.index).collect(),
                )
            })
            .collect();
        // An operation that made no change itself (it is all side
        // effects) is about every change it made.
        if !files.iter().flat_map(|f| &f.changes).any(|c| c.own) {
            for change in files.iter_mut().flat_map(|f| &mut f.changes) {
                change.own = true;
            }
        }
        UndoRecord {
            action: action.to_owned(),
            files,
        }
    }
}

/// `ids` in file order, the todo file first.
fn sorted(mut ids: Vec<TaskId>) -> Vec<TaskId> {
    ids.sort();
    ids
}
