//! The history of the session: what each write did, so that `u` and
//! `<C-r>` can put it back, and `.` can do the last change again.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use lsty_core::errs::{Code, Source};
use lsty_core::keys::Action;
use lsty_core::list::Snapshot;
use lsty_core::ops::{self, FileRecord, Followed, Tracking, UndoRecord};

use crate::live::{Loaded, Stamp};
use crate::messages;
use crate::model::Model;
use crate::prompt::Kind as PromptKind;
use crate::track::{Place, Remap, Wrote};

/// The place of each tracked line in a reading of a file.
type Lines = Vec<Option<usize>>;

/// A todo line and the done line it moved to or came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pair {
    todo: Vec<u8>,
    done: Vec<u8>,
}

/// One recorded action, and where its lines are in the files.
#[derive(Debug)]
pub(crate) struct Step {
    pub(crate) rec: UndoRecord,
    message: String,
    /// For each file the record wrote, where each of its lines is in the
    /// last reading, or `None` when the line is gone.
    at: Vec<(Source, Lines)>,
    pairs: Vec<Pair>,
}

impl Step {
    fn new(rec: UndoRecord, message: String, pairs: Vec<Pair>) -> Self {
        let at = rec
            .files
            .iter()
            .map(|f| (f.source, (0..f.post.len()).map(Some).collect()))
            .collect();
        let message = if message.is_empty() {
            rec.action.clone()
        } else {
            message
        };
        Self {
            rec,
            message,
            at,
            pairs,
        }
    }

    /// Moves every tracked line to its place in the next reading.
    fn follow(&mut self, moved: &Remap) {
        for (source, at) in &mut self.at {
            *at = moved.compose(*source, at);
        }
    }

    /// The lines of the record in the files as `snap` has them.
    fn tracking<'a>(&'a self, snap: &'a Snapshot) -> Tracking<'a> {
        let of = |source| {
            self.at.iter().find(|(s, _)| *s == source).map(|(_, at)| {
                Followed {
                    seen: snap.file(source),
                    at,
                }
            })
        };
        Tracking {
            todo: of(Source::Todo),
            done: of(Source::Done),
        }
    }
}

/// The undo and redo stacks of the session. Neither is stored.
#[derive(Debug, Default)]
pub(crate) struct History {
    pub(crate) undo: Vec<Step>,
    pub(crate) redo: Vec<Step>,
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

    /// Follows the lines of every record to the next reading.
    pub(crate) fn follow(&mut self, moved: &Remap) {
        for step in self.undo.iter_mut().chain(&mut self.redo) {
            step.follow(moved);
        }
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
    const fn verb(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Redo => "redo",
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
/// with what, never the items it was done to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Again {
    /// An action of the keys, with the count it had.
    Item { action: Action, count: usize },
    /// A move of a block of rows, as many as were selected.
    Block { action: Action, count: usize },
    /// A prompt's action, with the count and the text it had.
    Prompt {
        kind: PromptKind,
        count: usize,
        text: String,
    },
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
        let Some(top) = stack.last() else {
            self.message = Some(format!("nothing to {}", way.verb()));
            return;
        };
        let held = top.tracking(&self.snap);
        let result = self.attempt(|env| match way {
            Way::Undo => ops::undo(env, &top.rec, &held),
            Way::Redo => ops::redo(env, &top.rec, &held),
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
        let (touched, from) = moves_of(&record, &step.pairs);
        let snap = outcome.into_after();
        let stamp = Stamp::of(&snap);
        let wrote = Wrote {
            record: &record,
            touched: &touched,
            from: &from,
        };
        self.install(Loaded { snap, stamp }, clock, Some(&wrote));
        let sticky = self.keep_sticky(&touched);
        let next = Step::new(record, step.message, step.pairs);
        self.hist.stack(way.back()).push(next);
        if sticky && let Some(message) = &mut self.message {
            message.push_str("; ");
            message.push_str(messages::STICKY);
        }
    }

    /// `repeat`: the last change again, on the item under the cursor.
    /// A count replaces the one it had.
    pub(crate) fn repeat(&mut self, count: usize) {
        let Some(mut again) = self.again.clone() else {
            return;
        };
        let (Again::Item { count: stored, .. }
        | Again::Block { count: stored, .. }
        | Again::Prompt { count: stored, .. }) = &mut again;
        if count > 0 {
            *stored = count;
        }
        self.again = Some(again.clone());
        match again {
            Again::Item { action, count } => {
                self.run(action, count);
            }
            Again::Block { action, count } => self.repeat_block(action, count),
            Again::Prompt { kind, count, text } => {
                self.repeat_prompt(kind, count, &text);
            }
        }
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
                .map(|nr| (f.source, nr.get()))
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
    let index = usize::try_from(at.1).unwrap_or(usize::MAX);
    index
        .checked_sub(1)
        .and_then(|i| side(file).get(i))
        .map_or(&[], |line| &**line)
}

fn post_line(rec: &UndoRecord, at: Place) -> &[u8] {
    line_at(rec, at, |f| &f.post)
}

fn pre_line(rec: &UndoRecord, at: Place) -> &[u8] {
    line_at(rec, at, |f| &f.pre)
}

/// The lines a record moved between the files, from the places of the
/// lines it wrote and where each stood before.
pub(crate) fn pairs_of(
    rec: &UndoRecord,
    touched: &[Place],
    from: &[Place],
) -> Vec<Pair> {
    touched
        .iter()
        .zip(from)
        .filter(|(t, f)| t.0 != f.0)
        .map(|(&t, &f)| {
            let (now, was) = (post_line(rec, t), pre_line(rec, f));
            let (todo, done) = if t.0 == Source::Done {
                (was, now)
            } else {
                (now, was)
            };
            Pair {
                todo: todo.to_vec(),
                done: done.to_vec(),
            }
        })
        .collect()
}

/// The places of the lines a record wrote, and beside each the place it
/// stood before, when it moved from the todo file to the done file.
fn moves_of(rec: &UndoRecord, pairs: &[Pair]) -> (Vec<Place>, Vec<Place>) {
    let touched = touched_by(rec);
    let mut from = touched.clone();
    if pairs.is_empty() {
        return (touched, from);
    }
    let mut origins = Origins::new(rec, pairs);
    for (i, &at) in touched.iter().enumerate() {
        if at.0 != Source::Done {
            continue;
        }
        if let Some(origin) = origins.of(post_line(rec, at))
            && let Some(slot) = from.get_mut(i)
        {
            *slot = origin;
        }
    }
    (touched, from)
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
                    .push_back((Source::Todo, nr.get()));
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
