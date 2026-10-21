//! The changes of one operation, staged until it has passed every check.

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use super::{
    env::Env,
    refs::{Target, find, no_task},
    staged::{LineRef, Path, Row, StagedFile},
};
use crate::{
    discover::FileState,
    errs::{Code, Error, Field, Result, Source},
    list::{Snapshot, State, Task, TaskId},
    todotxt::{Line, share},
};

/// What a target names once it is found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Picked {
    /// A task.
    Task(TaskId),
    /// A blank line: the target is already deleted (todo file) or
    /// restored (done file). It holds the message that says so.
    Gone(String),
}

/// Stages the changes of one operation. Nothing is written until the
/// operation has planned and the rules have passed.
#[derive(Debug)]
pub(super) struct Tx {
    pub(super) snap: Snapshot,
    pub(super) todo: StagedFile,
    pub(super) done: StagedFile,
    /// The file the operation moves lines into; it is written first.
    pub(super) dest: Option<Source>,
    /// The lines the operation is about; they are reported as tasks.
    pub(super) primary: Vec<LineRef>,
    /// The lines of the operation's own changes, when it says so itself,
    /// as an undo does; otherwise those of its primary tasks.
    pub(super) own: Option<Vec<LineRef>>,
    /// Lines changed as a side effect.
    pub(super) side: Vec<LineRef>,
    /// Lines the operation created as a by-product.
    pub(super) spawned: Vec<LineRef>,
    pub(super) messages: Vec<Vec<u8>>,
    /// Messages about an id the operation may remove: the message's
    /// index and the id.
    pub(super) lost: Vec<(usize, Vec<u8>)>,
    /// Lines of the other file that a moved line has taken the place of.
    pub(super) paired: HashSet<TaskId>,
    /// A file written after the two lists, whole: its path and bytes.
    pub(super) log: Option<(PathBuf, Vec<u8>)>,
    /// The list as the staged bytes would leave it, once built. Only
    /// [`Tx::snapshot_after`] sets it, with the states it was built from.
    after: Option<Rebuilt>,
}

/// The list built from staged lines, kept with the states of both files
/// (see [`StagedFile::state`]) so that it is reused only while neither
/// has changed.
#[derive(Debug)]
struct Rebuilt {
    todo: u64,
    done: u64,
    snap: Arc<Snapshot>,
}

impl Tx {
    /// The list the staged lines leave now, built once and kept for as
    /// long as neither file has changed.
    pub(super) fn snapshot_after(&mut self, env: &Env<'_>) -> Arc<Snapshot> {
        let (todo, done) = (self.todo.state(), self.done.state());
        if let Some(kept) = self
            .after
            .as_ref()
            .filter(|kept| kept.todo == todo && kept.done == done)
        {
            return Arc::clone(&kept.snap);
        }
        let mut found = env.found.clone();
        if found.state == FileState::Missing {
            // Writing the list made the file.
            found.state = FileState::Exists;
        }
        let snap = Arc::new(Snapshot::build(
            found,
            self.todo.after_file(&self.snap),
            self.done.after_file(&self.snap),
            &env.params(),
        ));
        self.after = Some(Rebuilt {
            todo,
            done,
            snap: Arc::clone(&snap),
        });
        snap
    }

    pub(super) fn new(snap: Snapshot, create: bool) -> Self {
        Self {
            todo: StagedFile::new(Source::Todo, &snap, create),
            done: StagedFile::new(Source::Done, &snap, false),
            snap,
            dest: None,
            primary: Vec::new(),
            own: None,
            side: Vec::new(),
            spawned: Vec::new(),
            messages: Vec::new(),
            lost: Vec::new(),
            paired: HashSet::new(),
            log: None,
            after: None,
        }
    }

    pub(super) fn file(&self, source: Source) -> &StagedFile {
        source.pick(&self.todo, &self.done)
    }

    pub(super) fn file_mut(&mut self, source: Source) -> &mut StagedFile {
        source.pick(&mut self.todo, &mut self.done)
    }

    /// The lists as read under the lock.
    pub(super) const fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    /// The task behind `id`, which must come from this transaction.
    pub(super) fn task(&self, id: TaskId) -> Option<&Task> {
        self.snap.task(id)
    }

    /// The line of a task, as read.
    pub(super) fn line(&self, id: TaskId) -> Option<&Line> {
        self.snap.task(id).map(|task| self.snap.line(task))
    }

    /// The line of a task that was resolved a moment ago.
    ///
    /// # Errors
    ///
    /// `no_task` when the task is not in the snapshot, which a resolved
    /// id never is.
    pub(super) fn line_of(&self, id: TaskId) -> Result<&Line> {
        self.line(id).ok_or_else(gone)
    }

    /// The reference to the line of a task.
    pub(super) fn line_ref(&self, id: TaskId) -> Result<LineRef> {
        let task = self.snap.task(id).ok_or_else(gone)?;
        Ok(self.file(task.source()).line_ref(task.nr().index()))
    }

    /// How messages name a task: the reference its target used, or its
    /// number.
    pub(super) fn handle(&self, id: TaskId) -> String {
        let Some(task) = self.snap.task(id) else {
            return String::new();
        };
        let row = self.row(task);
        match row.and_then(|r| r.handle.as_ref()) {
            Some(handle) => handle.clone(),
            None => task.handle(),
        }
    }

    /// The reference as the caller wrote it; the handle when it was not
    /// given.
    pub(super) fn given(&self, id: TaskId) -> String {
        let given = self
            .snap
            .task(id)
            .and_then(|task| self.row(task))
            .and_then(|row| row.given.clone());
        given.unwrap_or_else(|| self.handle(id))
    }

    /// The line of a task as staged now.
    pub(super) fn current(&self, id: TaskId) -> Option<Line> {
        let task = self.snap.task(id)?;
        let row = self.row(task)?;
        Some(Line::parse_shared(&row.raw))
    }

    fn row(&self, task: &Task) -> Option<&Row> {
        self.file(task.source()).rows().get(task.nr().index())
    }

    /// Finds the task `target` names. A blank line is gone; a line that
    /// is not in the file is `no_task`; a task of the done file is
    /// refused unless the target accepts it.
    pub(super) fn target(&mut self, target: &Target) -> Result<Picked> {
        let handle = target.handle();
        let Some(task) = find(&self.snap, target)? else {
            let word = if matches!(target.line(), Some((Source::Done, _))) {
                "restored"
            } else {
                "deleted"
            };
            return Ok(Picked::Gone(format!("{handle} already {word}")));
        };
        if task.source() == Source::Done && !target.archived {
            return Err(archived_error(&self.snap, task, &handle));
        }
        let (source, index, id) =
            (task.source(), task.nr().index(), task.task_id());
        if let Some(row) = self.file_mut(source).rows_mut().get_mut(index)
            && row.handle.is_none()
        {
            row.handle = Some(handle);
            row.given = target.given().map(str::to_owned);
        }
        Ok(Picked::Task(id))
    }

    /// Finds the task `target` names. A blank line is said to be gone and
    /// gives `None`.
    pub(super) fn pick(&mut self, target: &Target) -> Result<Option<TaskId>> {
        match self.target(target)? {
            Picked::Task(id) => Ok(Some(id)),
            Picked::Gone(message) => {
                self.say(message);
                Ok(None)
            }
        }
    }

    /// The task an anchor names: it must be a task of the todo file.
    ///
    /// # Errors
    ///
    /// `no_task` for a blank line or a line that is not there,
    /// `wrong_state` for a task of the done file.
    pub(super) fn anchor(&mut self, anchor: &Target) -> Result<TaskId> {
        let anchor = anchor.clone().current_only();
        match self.target(&anchor)? {
            Picked::Task(id) => Ok(id),
            Picked::Gone(_) => Err(no_task(&anchor)),
        }
    }

    /// Marks the line of a task as one the operation is about.
    pub(super) fn mark_primary(&mut self, id: TaskId) -> Result<()> {
        let line = self.line_ref(id)?;
        self.primary.push(line);
        Ok(())
    }

    /// Runs `each` on every task the targets name, once per task and in
    /// the order given, and marks each as primary. A blank line is said
    /// to be gone and skipped.
    ///
    /// # Errors
    ///
    /// The first error of a target or of `each`.
    pub(super) fn each_task<F>(
        &mut self,
        targets: &[Target],
        mut each: F,
    ) -> Result<()>
    where
        F: FnMut(&mut Self, TaskId) -> Result<()>,
    {
        let mut seen = HashSet::new();
        for target in targets {
            let Some(id) = self.pick(target)? else {
                continue;
            };
            if seen.insert(id) {
                self.mark_primary(id)?;
                each(self, id)?;
            }
        }
        Ok(())
    }

    /// Stages `raw` as the new text of a task. The same text as now
    /// stages nothing.
    pub(super) fn set(
        &mut self,
        id: TaskId,
        raw: impl AsRef<[u8]>,
        path: Path,
    ) {
        let Some(task) = self.snap.task(id) else {
            return;
        };
        let (source, index) = (task.source(), task.nr().index());
        if let Some(row) = self.file_mut(source).rows_mut().get_mut(index) {
            let raw = raw.as_ref();
            if *row.raw != *raw {
                row.raw = share(raw);
                row.path = row.path.max(path);
            }
        }
    }

    /// The staged row of a task.
    pub(super) fn row_mut(&mut self, id: TaskId) -> Option<&mut Row> {
        let task = self.snap.task(id)?;
        let (source, index) = (task.source(), task.nr().index());
        self.file_mut(source).rows_mut().get_mut(index)
    }

    /// Stages a task as an empty line that keeps its place.
    pub(super) fn blank(&mut self, id: TaskId) {
        let Some(task) = self.snap.task(id) else {
            return;
        };
        let (source, index) = (task.source(), task.nr().index());
        if let Some(row) = self.file_mut(source).rows_mut().get_mut(index) {
            row.raw = Arc::default();
            row.blanked = true;
        }
    }

    /// Empties the todo-file line `target` names when it holds only
    /// spaces and tabs. It stays a blank line, so it is not a task.
    pub(super) fn clear_spaces(&mut self, target: &Target) {
        let Some((Source::Todo, nr)) = target.line() else {
            return;
        };
        let row = usize::try_from(nr - 1)
            .ok()
            .and_then(|index| self.todo.rows_mut().get_mut(index));
        if let Some(row) = row
            && !row.raw.is_empty()
            && Line::is_blank_text(&row.raw)
        {
            row.raw = Arc::default();
            row.blanked = true;
        }
    }

    /// Stages a new line after line `after` of `source`; 0 is the top.
    pub(super) fn insert(
        &mut self,
        source: Source,
        after: usize,
        raw: impl Into<Arc<[u8]>>,
        path: Path,
    ) -> LineRef {
        let index = self.file_mut(source).insert(after, raw.into(), path);
        self.file(source).line_ref(index)
    }

    /// Stages a new line at the end of `source`.
    pub(super) fn append(
        &mut self,
        source: Source,
        raw: impl Into<Arc<[u8]>>,
        path: Path,
    ) -> LineRef {
        let index = self.file_mut(source).append(raw.into(), path);
        self.file(source).line_ref(index)
    }

    /// Stages tasks, all of one file, after line `after` of it; 0 is the
    /// top.
    pub(super) fn relocate(&mut self, ids: &[TaskId], after: usize) {
        let tasks: Vec<_> =
            ids.iter().filter_map(|&id| self.snap.task(id)).collect();
        let Some(first) = tasks.first() else {
            return;
        };
        let source = first.source();
        let rows: Vec<usize> = tasks.iter().map(|i| i.nr().index()).collect();
        self.file_mut(source).relocate(&rows, after);
    }

    /// Adds a message to the result. It is bytes, because it can carry
    /// the text of a line.
    pub(super) fn say(&mut self, message: impl Into<Vec<u8>>) {
        self.messages.push(message.into());
    }

    /// The error for an edit that would turn a word into a stamp.
    pub(super) fn stamp_word_refusal(&self, id: TaskId) -> Error {
        let handle = self.handle(id);
        stamped(
            &self.snap,
            id,
            &handle,
            "the text would start with a stamp-like word; reword it first",
        )
    }
}

/// The error for an id that was resolved a moment ago and is not there.
pub(super) fn gone() -> Error {
    Error::new(Code::NoTask, "no such task")
}

/// A `wrong_state` error about `task`, whose `state` field is `state`.
pub(super) fn wrong_state(
    snap: &Snapshot,
    task: &Task,
    state: State,
    message: String,
    hint: String,
) -> Error {
    Error::new(Code::WrongState, message)
        .with_hint(hint)
        .with_task(snap.ref_of(task))
        .with_field("state", Field::Str(state.as_str().to_owned()))
}

pub(super) fn archived_error(
    snap: &Snapshot,
    task: &Task,
    handle: &str,
) -> Error {
    wrong_state(
        snap,
        task,
        State::Done,
        format!("{handle} is read-only history"),
        format!("{handle} is archived; use restore"),
    )
}

/// The refusal of a text edit that changes a stamp.
pub(super) fn stamped(
    snap: &Snapshot,
    id: TaskId,
    handle: &str,
    hint: &str,
) -> Error {
    let Some(task) = snap.task(id) else {
        return gone();
    };
    let message = format!("{handle}: a text edit can't change a stamp");
    wrong_state(snap, task, task.state(), message, hint.to_owned())
}
