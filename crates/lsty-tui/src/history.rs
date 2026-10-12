//! The history of the session: what each write did, so that `u` and
//! `<C-r>` can put it back, and `.` can do the last change again.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use lsty_core::{
    errs::{Code, Source},
    keys::Action,
    list::Snapshot,
    ops::{self, FileRecord, Followed, UndoRecord},
};

use crate::{
    model::Model,
    msg::Cmd,
    prompt::Kind as PromptKind,
    track::{Place, Remap, Touched, Wrote},
};

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
    at: Vec<Lines>,
    pairs: Vec<Pair>,
}

impl Step {
    fn new(rec: UndoRecord, message: String, pairs: Vec<Pair>) -> Self {
        let at = rec
            .files
            .iter()
            .map(|f| (0..f.post().len()).map(Some).collect())
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
        for (f, at) in self.rec.files.iter().zip(&mut self.at) {
            *at = moved.compose(f.source, at);
        }
    }

    /// The lines of the record in the files as `snap` has them.
    fn tracking<'a>(&'a self, snap: &'a Snapshot) -> Vec<Followed<'a>> {
        self.rec
            .files
            .iter()
            .zip(&self.at)
            .map(|(f, at)| Followed {
                seen: snap.file(f.source),
                at,
            })
            .collect()
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

    /// `repeat`: the last change again, on the item under the cursor.
    /// A count replaces the one it had.
    pub(crate) fn repeat(&mut self, count: usize) -> Vec<Cmd> {
        let Some(again) = &mut self.again else {
            return Vec::new();
        };
        let (Again::Item { count: stored, .. }
        | Again::Block { count: stored, .. }
        | Again::Prompt { count: stored, .. }) = again;
        if count > 0 {
            *stored = count;
        }
        match again.clone() {
            Again::Item { action, count } => return self.run(action, count),
            Again::Block { action, count } => self.repeat_block(action, count),
            Again::Prompt { kind, count, text } => {
                self.repeat_prompt(kind, count, &text);
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
