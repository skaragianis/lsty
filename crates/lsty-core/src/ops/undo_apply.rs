//! Undo and redo: put back what a record says an operation wrote, but
//! only when every line it wrote still reads as it left it.
//!
//! The check is by content and covers both files: one line that has
//! changed since refuses the whole undo, and nothing is written. The
//! rule layer does not run, since an undo restores what was there.

use std::collections::{BTreeMap, HashMap};

use super::env::Env;
use super::result::Outcome;
use super::run::{Rules, run_with};
use super::staged::{ChangeKind, Path, Row, StagedFile};
use super::tx::Tx;
use super::undo::{Change, FileRecord, UndoRecord};
use crate::errs::{self, Code, Error, Nr, Result, Source};
use crate::lineid;
use crate::todotxt::File;

/// The lines a record wrote, followed to a later reading of one file.
#[derive(Clone, Copy, Debug)]
pub struct Followed<'a> {
    /// The file as it was last seen.
    pub seen: &'a File,
    /// For each line the record wrote (its `post` lines), the line of
    /// `seen` it is, or `None` when it is gone.
    pub at: &'a [Option<usize>],
}

/// Where the lines of a record are in both files.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tracking<'a> {
    /// The todo file; `None` when no line of it is followed.
    pub todo: Option<Followed<'a>>,
    /// The done file; `None` when no line of it is followed.
    pub done: Option<Followed<'a>>,
}

impl<'a> Tracking<'a> {
    const fn of(&self, source: Source) -> Option<Followed<'a>> {
        match source {
            Source::Todo => self.todo,
            Source::Done => self.done,
        }
    }
}

/// Which way a record is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Undo,
    Redo,
}

impl Verb {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Redo => "redo",
        }
    }
}

/// Reverts `rec`, or refuses when a line it wrote has changed.
///
/// # Errors
///
/// `expect_mismatch` naming the line when one has changed since, and
/// whatever locking, reading or writing the files fails with.
pub fn undo(
    env: &Env<'_>,
    rec: &UndoRecord,
    tracked: &Tracking<'_>,
) -> Result<Outcome> {
    apply(env, rec, tracked, Verb::Undo)
}

/// Reverts `rec`, the record of an undo, the same way.
///
/// # Errors
///
/// As for [`undo`], with `can't redo` in the message.
pub fn redo(
    env: &Env<'_>,
    rec: &UndoRecord,
    tracked: &Tracking<'_>,
) -> Result<Outcome> {
    apply(env, rec, tracked, Verb::Redo)
}

impl FileRecord {
    /// Where each line of `pre` is in `post`, or `None`. The lines the
    /// operation staged are followed by number; the others by a diff,
    /// unless the diff matched them with a staged line.
    #[must_use]
    pub fn pre_to_post(&self) -> Vec<Option<usize>> {
        let mut map = self.diff_pre_to_post().to_vec();
        let staged: Vec<usize> = self
            .changes
            .iter()
            .filter_map(|c| c.nr_after)
            .map(Nr::index)
            .collect();
        let mut is_staged = vec![false; self.post().len()];
        for at in staged {
            if let Some(slot) = is_staged.get_mut(at) {
                *slot = true;
            }
        }
        for at in &mut map {
            if at.is_some_and(|to| is_staged.get(to) == Some(&true)) {
                *at = None;
            }
        }
        for change in &self.changes {
            let Some(before) = change.nr_before else {
                continue;
            };
            if let Some(slot) = map.get_mut(before.index()) {
                *slot = change.nr_after.map(Nr::index);
            }
        }
        map
    }
}

fn apply(
    env: &Env<'_>,
    rec: &UndoRecord,
    tracked: &Tracking<'_>,
    verb: Verb,
) -> Result<Outcome> {
    run_with(env, &rec.action, true, Rules::Skip, |tx| {
        let plans: Vec<Plan<'_>> = rec
            .files
            .iter()
            .map(|f| Plan::new(tx, f, tracked.of(f.source)))
            .collect();
        for plan in &plans {
            if let Some(change) = plan.stale(tx.file(plan.rec.source)) {
                return Err(changed_since(verb, change));
            }
        }
        for plan in &plans {
            plan.revert(tx.file_mut(plan.rec.source));
        }
        tx.dest(returned_to(tx));
        Ok(())
    })
}

fn changed_since(verb: Verb, change: &Change) -> Error {
    Error::new(
        Code::ExpectMismatch,
        format!(
            "can't {}: \"{}\" was changed since",
            verb.as_str(),
            errs::lossy(&change.after)
        ),
    )
}

/// The file that lines were put back into: the first with a line that
/// was empty and is not any more. The todo file when there is none.
fn returned_to(tx: &Tx) -> Source {
    [Source::Todo, Source::Done]
        .into_iter()
        .find(|&source| {
            tx.file(source).all.iter().any(|row| {
                row.orig.is_empty() && !row.raw.is_empty() && !row.removed
            })
        })
        .unwrap_or(Source::Todo)
}

/// A line to put back, and the line it is put back into, if any.
struct PutBack<'a> {
    change: &'a Change,
    row: Option<usize>,
}

/// The revert of one file of a record.
struct Plan<'a> {
    rec: &'a FileRecord,
    /// For each line the record wrote, its row now.
    at: Vec<Option<usize>>,
    /// For a row, the line of the record's `pre` it is.
    pre_of: HashMap<usize, usize>,
}

impl<'a> Plan<'a> {
    fn new(
        tx: &Tx,
        rec: &'a FileRecord,
        followed: Option<Followed<'_>>,
    ) -> Self {
        let at = followed
            .map_or_else(Vec::new, |f| place(f, tx.snap.file(rec.source)));
        let mut pre_of = HashMap::new();
        for (pre, post) in rec.pre_to_post().into_iter().enumerate() {
            let row = post.and_then(|p| at.get(p).copied().flatten());
            if let Some(row) = row {
                pre_of.insert(row, pre);
            }
        }
        Self { rec, at, pre_of }
    }

    /// The row the change wrote, or `None` when it is gone.
    fn row_index(&self, change: &Change) -> Option<usize> {
        let line = change.nr_after?.index();
        self.at.get(line).copied().flatten()
    }

    /// The first change whose line no longer reads as it left it.
    fn stale(&self, file: &StagedFile) -> Option<&'a Change> {
        self.rec.changes.iter().find(|c| {
            must_hold(c.kind)
                && self
                    .row_index(c)
                    .and_then(|i| file.all.get(i))
                    .is_none_or(|row| row.raw != c.after)
        })
    }

    fn revert(&self, file: &mut StagedFile) {
        let mut back = Vec::new();
        for change in &self.rec.changes {
            self.revert_change(file, change, &mut back);
        }
        back.sort_by_key(|b| b.change.nr_before.map(Nr::get));
        let mut places = Places::new(file, &self.pre_of);
        for put in back {
            if let Some(done) = Self::put_back(file, &put) {
                places.put(&done);
            }
        }
        file.order = places.order(&file.order);
        if self.rec.open_end()
            && let Some(last) = self.rec.pre().last()
        {
            file.end_open_after(last);
        }
    }

    /// Reverts one change, or notes the line it has to put back.
    fn revert_change<'c>(
        &self,
        file: &mut StagedFile,
        change: &'c Change,
        back: &mut Vec<PutBack<'c>>,
    ) {
        let row = self.row_index(change);
        match (change.kind, row) {
            (ChangeKind::Changed, Some(i)) => {
                set_text(&mut file.all[i], &change.before);
            }
            (ChangeKind::Added, Some(i)) => unadd(file, i),
            (ChangeKind::Moved, _) => {
                if let Some(i) = row {
                    file.order.retain(|&o| o != i);
                }
                back.push(PutBack { change, row });
            }
            (ChangeKind::Blanked | ChangeKind::Removed, _) => {
                let row = row.filter(|&i| file.all[i].raw.is_empty());
                if !change.before.is_empty() {
                    back.push(PutBack { change, row });
                }
            }
            (ChangeKind::Changed | ChangeKind::Added, None) => {}
        }
    }

    /// Puts the text of a line back. Says which row it is and which
    /// line of the record, and whether the row has to go into the order
    /// again (a blanked line is still in it).
    fn put_back(file: &mut StagedFile, put: &PutBack<'_>) -> Option<Back> {
        let line = put.change.nr_before.map(Nr::index)?;
        let before = &put.change.before;
        let (row, enter) = match (put.row, put.change.kind) {
            (Some(i), ChangeKind::Blanked) => {
                set_text(&mut file.all[i], before);
                (i, false)
            }
            (Some(i), _) => {
                file.all[i].moved = true;
                (i, true)
            }
            (None, _) => {
                let i = file.all.len();
                file.all
                    .push(Row::added(before.clone(), Path::Action, None));
                (i, true)
            }
        };
        Some(Back { row, line, enter })
    }
}

/// A line put back.
struct Back {
    row: usize,
    /// The line of the record it is.
    line: usize,
    /// Whether the row goes into the order again.
    enter: bool,
}

/// Where the lines put back go: each after the nearest line that stood
/// before it, or at the top.
struct Places {
    /// The rows in the order, by the line of the record they are.
    known: BTreeMap<usize, usize>,
    /// The rows put after a row, in the order they were put.
    after: HashMap<usize, Vec<usize>>,
    /// The rows put at the top, in the order they were put.
    top: Vec<usize>,
}

impl Places {
    fn new(file: &StagedFile, pre_of: &HashMap<usize, usize>) -> Self {
        let known = file
            .order
            .iter()
            .filter_map(|row| Some((*pre_of.get(row)?, *row)))
            .collect();
        Self {
            known,
            after: HashMap::new(),
            top: Vec::new(),
        }
    }

    fn put(&mut self, back: &Back) {
        if back.enter {
            match self.known.range(..back.line).next_back() {
                Some((_, &anchor)) => {
                    self.after.entry(anchor).or_default().push(back.row);
                }
                None => self.top.push(back.row),
            }
        }
        self.known.insert(back.line, back.row);
    }

    /// The order with every row put back in its place. A row put later
    /// stands before one put earlier after the same row.
    fn order(&self, rows: &[usize]) -> Vec<usize> {
        let mut out = Vec::with_capacity(rows.len() + self.top.len());
        for &unit in self.top.iter().rev().chain(rows) {
            self.emit(unit, &mut out);
        }
        out
    }

    /// Writes `row`, then the rows put after it, each with its own.
    fn emit(&self, row: usize, out: &mut Vec<usize>) {
        let mut stack = vec![row];
        while let Some(row) = stack.pop() {
            out.push(row);
            stack.extend(self.after.get(&row).into_iter().flatten());
        }
    }
}

const fn must_hold(kind: ChangeKind) -> bool {
    matches!(
        kind,
        ChangeKind::Changed | ChangeKind::Added | ChangeKind::Moved
    )
}

/// Where each followed line is in the lines read now.
fn place(followed: Followed<'_>, now: &File) -> Vec<Option<usize>> {
    if followed.seen == now {
        return followed.at.to_vec();
    }
    let to = lineid::remap(&followed.seen.lines, &now.lines);
    followed
        .at
        .iter()
        .map(|seen| seen.and_then(|s| to.get(s).copied().flatten()))
        .collect()
}

fn set_text(row: &mut Row, text: &[u8]) {
    row.raw = text.to_vec();
    row.blanked = text.is_empty() && !row.orig.is_empty();
}

/// Takes out a line the operation added: the done file keeps a blank
/// line in its place, the todo file loses the line.
fn unadd(file: &mut StagedFile, index: usize) {
    if file.source == Source::Done {
        set_text(&mut file.all[index], &[]);
        return;
    }
    file.order.retain(|&o| o != index);
    file.all[index].removed = true;
}
