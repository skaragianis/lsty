//! The history of the session: what each write did, so that `u` and
//! `<C-r>` can put it back, and `.` can do the last change again.

use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use jiff::Timestamp;
use lsty_core::{
    errs::{Code, Error, Source},
    keys::Action,
    list::Snapshot,
    ops::{self, FileRecord, UndoRecord},
};

use crate::{
    messages,
    model::{MESSAGE_LIFE, Model},
    msg::Cmd,
    prompt::Kind as PromptKind,
    rows::Row,
    track::{Place, Remap, Touched, Wrote},
};

/// A todo line and the done line it moved to or came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pair {
    todo: Vec<u8>,
    done: Vec<u8>,
}

/// One recorded action.
#[derive(Debug)]
pub(crate) struct Step {
    pub(crate) rec: UndoRecord,
    message: String,
    pairs: Vec<Pair>,
    /// Whether `rec` lands on the reading of this serial, kept so that a
    /// hint drawn every frame checks the file once per reading.
    landed: Cell<Option<(u32, bool)>>,
}

impl Step {
    fn new(rec: UndoRecord, message: String, pairs: Vec<Pair>) -> Self {
        let message = if message.is_empty() {
            rec.action.clone()
        } else {
            message
        };
        Self {
            rec,
            message,
            pairs,
            landed: Cell::new(None),
        }
    }

    /// Whether putting this step back from `snap` would land: see
    /// [`ops::lands`].
    pub(crate) fn lands(&self, snap: &Snapshot) -> bool {
        match self.landed.get() {
            Some((serial, lands)) if serial == snap.serial() => lands,
            _ => {
                let lands = ops::lands(&self.rec, snap);
                self.landed.set(Some((snap.serial(), lands)));
                lands
            }
        }
    }
}

/// The window in which a second `t` puts back what the step on top of the
/// undo stack stopped.
#[derive(Debug)]
pub(crate) struct Stopped {
    until: Timestamp,
    /// Where the tasks it stopped stand in the last reading.
    at: Vec<Place>,
}

/// The undo and redo stacks of the session. Neither is stored.
#[derive(Debug, Default)]
pub(crate) struct History {
    pub(crate) undo: Vec<Step>,
    pub(crate) redo: Vec<Step>,
    /// Any other key, or a mouse press, ends it; a message neither ends nor
    /// extends it.
    pub(crate) stopped: Option<Stopped>,
}

impl History {
    /// Puts a new action on the undo stack and empties the redo stack.
    pub(crate) fn record(
        &mut self,
        rec: UndoRecord,
        message: String,
        pairs: Vec<Pair>,
    ) {
        self.undo.push(Step::new(rec, message, pairs));
        self.redo.clear();
    }

    /// Follows the tasks of the window to the next reading.
    pub(crate) fn follow(&mut self, moved: &Remap) {
        let at = self.stopped.iter_mut().flat_map(|s| &mut s.at);
        at.for_each(|at| *at = moved.place(*at));
    }

    const fn stack(&mut self, way: Way) -> &mut Vec<Step> {
        match way {
            Way::Undo => &mut self.undo,
            Way::Redo => &mut self.redo,
        }
    }
}

/// Which way a record is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Way {
    Undo,
    Redo,
}

impl Way {
    const fn action(self) -> Action {
        match self {
            Self::Undo => Action::Undo,
            Self::Redo => Action::Redo,
        }
    }

    const fn past(self) -> &'static str {
        match self {
            Self::Undo => "undid",
            Self::Redo => "redid",
        }
    }

    const fn back(self) -> Self {
        match self {
            Self::Undo => Self::Redo,
            Self::Redo => Self::Undo,
        }
    }
}

/// The last change, as `.` runs it again. It is data: what to do and
/// with what, never the tasks it was done to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Again {
    /// An action of the keys, with the count it had.
    Task { action: Action, count: usize },
    /// A move of a block of rows, as many as were selected.
    Block { action: Action, count: usize },
    /// A prompt's action, with the count and the text it had.
    Prompt {
        kind: PromptKind,
        count: usize,
        text: String,
    },
}

impl Again {
    /// The action it runs again.
    pub(crate) const fn action(&self) -> Action {
        match self {
            Self::Task { action, .. } | Self::Block { action, .. } => *action,
            Self::Prompt { kind, .. } => kind.action(),
        }
    }

    /// The count it runs again with: the one typed, else the one it had.
    pub(crate) const fn count(&self, typed: usize) -> usize {
        let (Self::Task { count, .. }
        | Self::Block { count, .. }
        | Self::Prompt { count, .. }) = self;
        if typed > 0 { typed } else { *count }
    }
}

impl Model {
    /// `undo`: puts back the last action of the session.
    pub(crate) fn undo_last(&mut self) {
        if self.kept.is_some() {
            self.restore_draft();
            return;
        }
        self.rewind(Way::Undo);
    }

    /// `redo`: does the last undone action again.
    pub(crate) fn redo_last(&mut self) {
        self.rewind(Way::Redo);
    }

    /// Applies the top step of one stack and moves it to the other. A
    /// step that is refused is dropped; one that met a busy lock stays.
    fn rewind(&mut self, way: Way) {
        let stack = match way {
            Way::Undo => &self.hist.undo,
            Way::Redo => &self.hist.redo,
        };
        // A step whose lines changed is refused by the write, which drops it.
        let Some(top) = stack.last() else {
            self.refused(way.action(), 1);
            return;
        };
        let result = self.attempt(|env| match way {
            Way::Undo => ops::undo(env, &top.rec),
            Way::Redo => ops::redo(env, &top.rec),
        });
        match result {
            Ok((outcome, clock)) => {
                let Some(step) = self.hist.stack(way).pop() else {
                    return;
                };
                self.message =
                    Some(format!("{}: {}", way.past(), step.message));
                self.rewound(outcome, clock, way, step);
            }
            Err(e) => {
                if e.code() == Code::ExpectMismatch {
                    self.hist.stack(way).pop();
                }
                self.fail(&e);
            }
        }
    }

    /// Shows the files an undo or redo wrote and keeps its record on
    /// the other stack.
    fn rewound(
        &mut self,
        mut outcome: ops::Outcome,
        clock: lsty_core::dates::Clock,
        way: Way,
        step: Step,
    ) {
        let Some(record) = outcome.take_undo() else {
            return;
        };
        self.accept(outcome.read().clone(), clock.clone());
        let touched = moves_of(&record, &step.pairs);
        let snap = outcome.into_after();
        let wrote = Wrote {
            record: &record,
            touched: &touched,
            undone: true,
        };
        self.install(snap, clock, Some(&wrote));
        self.keep_sticky(&touched);
        let next = Step::new(record, step.message, step.pairs);
        self.hist.stack(way.back()).push(next);
    }

    /// Opens the window of the `t` that just stopped tasks.
    pub(crate) fn remember_stop(&mut self) {
        let now = self.clock_now().map(|c| c.now());
        let until = now.ok().and_then(|t| t.checked_add(MESSAGE_LIFE).ok());
        let at = self.hist.undo.last().map(|top| touched_by(&top.rec));
        self.hist.stopped =
            until.zip(at).map(|(until, at)| Stopped { until, at });
    }

    /// Whether the window is still open, when `rows` are exactly the tasks
    /// it is about.
    pub(crate) fn stopped_over(&self, rows: &[Row]) -> Option<bool> {
        let stopped = self.hist.stopped.as_ref()?;
        let mut asked: Vec<Place> = rows.iter().map(Row::place).collect();
        let mut own = stopped.at.clone();
        asked.sort_unstable();
        own.sort_unstable();
        (asked == own)
            .then(|| self.clock_now().is_ok_and(|c| c.now() < stopped.until))
    }

    /// Tells whether the window is open over the tasks of `count`, and ends
    /// it if they are the tasks.
    pub(crate) fn just_stopped(&mut self, count: usize) -> bool {
        let over = self.stopped_over(self.rows_of(count));
        if over.is_some() {
            self.hist.stopped = None;
        }
        over == Some(true)
    }

    /// Puts back the lines of the step on top of the undo stack, as a
    /// write of its own, or refuses when one no longer stands: the press
    /// was aimed at the line as it was, and the next is a new one. Tells
    /// whether it ran.
    pub(crate) fn put_back(&mut self) -> bool {
        let Some(top) = self.hist.undo.last() else {
            return false;
        };
        let result = self.attempt(|env| ops::undo(env, &top.rec));
        if result
            .as_ref()
            .is_err_and(|e| e.code() == Code::ExpectMismatch)
        {
            self.fail(&Error::new(Code::ExpectMismatch, ops::LIST_CHANGED));
            return false;
        }
        if !self.settle(result) {
            return false;
        }
        self.message = Some(messages::PUT_BACK.to_owned());
        if let Some(step) = self.hist.undo.last_mut() {
            messages::PUT_BACK.clone_into(&mut step.message);
        }
        true
    }

    /// `repeat`: the last change again, on the task under the cursor.
    /// A count replaces the one it had, once the press writes.
    pub(crate) fn repeat(&mut self, typed: usize) -> Vec<Cmd> {
        let Some(again) = self.again.clone() else {
            return Vec::new();
        };
        let count = again.count(typed);
        match again {
            Again::Task { action, .. } => return self.run(action, count),
            Again::Block { action, .. } => self.move_by(action, true, count),
            Again::Prompt { kind, text, .. } => {
                self.repeat_prompt(kind, count, text);
            }
        }
        Vec::new()
    }
}

/// The places of the lines a record wrote, in file order.
fn touched_by(rec: &UndoRecord) -> Vec<Place> {
    rec.files
        .iter()
        .flat_map(|f| {
            f.changes
                .iter()
                .filter_map(|c| c.nr_after)
                .map(|nr| (f.source, nr))
        })
        .collect()
}

/// The text of the line at `at`, from the side of the record `side`
/// picks; empty when there is none.
fn line_at<'a>(
    rec: &'a UndoRecord,
    at: Place,
    side: impl Fn(&'a FileRecord) -> &'a [Arc<[u8]>],
) -> &'a [u8] {
    let Some(file) = rec.files.iter().find(|f| f.source == at.0) else {
        return &[];
    };
    side(file).get(at.1.index()).map_or(&[], |line| &**line)
}

/// The lines a record moved between the files, from the lines it wrote
/// and where each stood before.
pub(crate) fn pairs_of(rec: &UndoRecord, touched: &[Touched]) -> Vec<Pair> {
    touched
        .iter()
        .filter_map(|t| {
            let was = t.was.filter(|w| w.0 != t.now.0)?;
            let (now, before) = (
                line_at(rec, t.now, FileRecord::post),
                line_at(rec, was, FileRecord::pre),
            );
            let (todo, done) = t.now.0.pick((now, before), (before, now));
            Some(Pair {
                todo: todo.to_vec(),
                done: done.to_vec(),
            })
        })
        .collect()
}

/// The lines a record wrote, each done line with the todo place it came
/// from when it moved there.
fn moves_of(rec: &UndoRecord, pairs: &[Pair]) -> Vec<Touched> {
    let mut origins = (!pairs.is_empty()).then(|| Origins::new(rec, pairs));
    touched_by(rec)
        .into_iter()
        .map(|now| Touched {
            now,
            was: origins
                .as_mut()
                .filter(|_| now.0 == Source::Done)
                .and_then(|o| o.of(line_at(rec, now, FileRecord::post))),
        })
        .collect()
}

/// Lines of text.
type Texts<'a> = Vec<&'a [u8]>;

/// Where the todo lines that moved to the done file stood.
struct Origins<'a> {
    /// The todo text paired with a done text, in order.
    todo_of: HashMap<&'a [u8], Texts<'a>>,
    /// The places of the todo lines the record emptied, by their text,
    /// in file order. A place is taken once.
    emptied: HashMap<&'a [u8], VecDeque<Place>>,
}

impl<'a> Origins<'a> {
    fn new(rec: &'a UndoRecord, pairs: &'a [Pair]) -> Self {
        let mut todo_of: HashMap<&[u8], Texts<'_>> = HashMap::new();
        for pair in pairs {
            todo_of.entry(&pair.done).or_default().push(&pair.todo);
        }
        let mut emptied: HashMap<&[u8], VecDeque<Place>> = HashMap::new();
        let todo = rec.files.iter().find(|f| f.source == Source::Todo);
        for change in todo.into_iter().flat_map(|f| &f.changes) {
            if let Some(nr) = change.nr_before
                && change.after.is_empty()
            {
                emptied
                    .entry(&change.before)
                    .or_default()
                    .push_back((Source::Todo, nr));
            }
        }
        Self { todo_of, emptied }
    }

    /// The place of the first todo line the record emptied that was
    /// paired with the done line `text`, if no line took it before.
    fn of(&mut self, text: &[u8]) -> Option<Place> {
        let todos = self.todo_of.get(text)?;
        todos
            .iter()
            .find_map(|todo| self.emptied.get_mut(todo)?.pop_front())
    }
}
