//! What an operation reports.

use std::{collections::HashSet, sync::Arc};

use super::{
    env::Env,
    run::Sealed,
    staged::{LineRef, SealedFile},
    tx::Tx,
    undo::UndoRecord,
};
use crate::{
    errs::{Nr, Source},
    list::{Snapshot, Task, TaskId},
};

/// What encloses a line's name in a message of an operation that was given
/// `Env::by_place`, as the files are written, for [`name_lines`] to name.
pub(super) const MARK: char = '\u{1f}';

/// What encloses the name of a line as it was read: the report turns it
/// into a mark of the line as written, and leaves it for a task with none
/// (see [`name_read`]).
pub(super) const READ: char = '\u{1e}';

/// `message` with each marked line named by `name`, which gets its file
/// and its number as the files are written. The marks stand where a
/// message names a line the operation read, made, moved or freed; a caller
/// that gave `Env::by_place` names them as its list does. A mark that
/// holds no line, as one in the text of a task, stays as it is.
pub fn name_lines(
    message: &[u8],
    name: impl FnMut(Source, Nr) -> String,
) -> Vec<u8> {
    marked(message, MARK, name)
}

/// `message` with each line it marks as read, of a task the operation left
/// no line, named by `name` from the list the operation read.
pub fn name_read(
    message: &[u8],
    name: impl FnMut(Source, Nr) -> String,
) -> Vec<u8> {
    marked(message, READ, name)
}

/// [`name_lines`] for the marks `mark`.
pub(super) fn marked(
    message: &[u8],
    mark: char,
    mut name: impl FnMut(Source, Nr) -> String,
) -> Vec<u8> {
    let mark = mark as u8;
    let (mut out, mut rest) = (Vec::new(), message);
    while let Some(open) = rest.iter().position(|b| *b == mark) {
        let inner = &rest[open + 1..];
        let line = inner.iter().position(|b| *b == mark).and_then(|close| {
            let line = std::str::from_utf8(&inner[..close]).ok()?;
            let (source, digits) = match line.strip_prefix("done:") {
                Some(digits) => (Source::Done, digits),
                None => (Source::Todo, line),
            };
            Some((close, name(source, Nr::new(digits.parse().ok()?)?)))
        });
        let (kept, name, left) = match line {
            Some((close, name)) => (open, name, &inner[close + 1..]),
            None => (open + 1, String::new(), inner),
        };
        out.extend_from_slice(&rest[..kept]);
        out.extend(name.into_bytes());
        rest = left;
    }
    out.extend_from_slice(rest);
    out
}

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
    messages: Vec<Vec<u8>>,
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

    /// What the operation said, then the warnings about touched tasks.
    pub fn messages(&self) -> &[Vec<u8>] {
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
        let mut messages: Vec<_> = (tx.messages.iter())
            .map(|m| marked(m, READ, |source, nr| tx.written_name(source, nr)))
            .collect();
        messages.extend(
            after
                .warnings_for(touched)
                .into_iter()
                .map(String::into_bytes),
        );
        if no_op && tx.messages.is_empty() {
            messages.push(NOTHING_CHANGED.as_bytes().to_vec());
        }
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
    /// How a message names the line `nr` of `source` as it was read, once
    /// the files are written: the line its task has then, else it stays
    /// as read.
    fn written_name(&self, source: Source, nr: Nr) -> String {
        let task = self.snap.at(source, nr);
        match task.and_then(|t| self.written_line(t.task_id())) {
            Some((source, at)) => self.line_name(source, at + 1),
            None => self.read_name(source, nr),
        }
    }

    /// The file and the position the task `id` has once written, in
    /// whichever file it is, unless it is gone or blank there.
    fn written_line(&self, id: TaskId) -> Option<(Source, usize)> {
        [Source::Todo, Source::Done].into_iter().find_map(|source| {
            let file = self.file(source);
            let rows = file.rows().iter().enumerate();
            rows.filter(|(_, row)| {
                row.was() == Some(id) && !row.removed && !row.blanked
            })
            .find_map(|(index, _)| Some((source, file.position(index)?)))
        })
    }

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
