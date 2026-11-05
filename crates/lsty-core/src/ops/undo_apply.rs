//! Undo and redo: put back what a record says an operation wrote, but
//! only when every line it wrote still stands as it left it.
//!
//! The check takes the file as the operation left it for the reading an
//! action is aimed from, and covers both files: one line that changed,
//! moved or went refuses the whole undo, and nothing is written. A line
//! that was taken away is put back whatever became of its neighbours. The
//! rule layer does not run, since an undo restores what was there.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::RangeInclusive,
};

use super::{
    env::Env,
    landing::{LIST_CHANGED, kept_in},
    result::Outcome,
    run::{Rules, run_with},
    staged::{ChangeKind, LineRef, Path, Row, StagedFile},
    tx::Tx,
    undo::{Change, FileRecord, UndoRecord},
};
use crate::{
    errs::{Code, Error, Nr, Result, Source},
    lineid,
    todotxt::{Line, share},
};

/// Reverts `rec`, or refuses when a line it wrote no longer stands.
///
/// # Errors
///
/// `expect_mismatch` when one no longer stands, and whatever locking,
/// reading or writing the files fails with.
pub fn undo(env: &Env<'_>, rec: &UndoRecord) -> Result<Outcome> {
    apply(env, rec, "undo")
}

/// Reverts `rec`, the record of an undo, the same way.
///
/// # Errors
///
/// As for [`undo`], with `can't redo` in the message.
pub fn redo(env: &Env<'_>, rec: &UndoRecord) -> Result<Outcome> {
    apply(env, rec, "redo")
}

impl FileRecord {
    /// Where each line of `pre` is in `post`, or `None`. The lines the
    /// operation staged are followed by number; the others by a diff.
    #[must_use]
    pub fn pre_to_post(&self) -> Vec<Option<usize>> {
        let mut map = self.diff_pre_to_post().to_vec();
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
    verb: &'static str,
) -> Result<Outcome> {
    run_with(env, &rec.action, true, Rules::Skip, |tx| {
        let plans: Vec<Plan<'_>> =
            rec.files.iter().map(|f| Plan::new(tx, f)).collect();
        if plans.iter().any(|plan| plan.stale(tx)) {
            return Err(Error::new(
                Code::ExpectMismatch,
                format!("can't {verb}: {LIST_CHANGED}"),
            ));
        }
        let mut own = Vec::new();
        for plan in &plans {
            let source = plan.rec.source;
            let rows = plan.revert(tx.file_mut(source));
            own.extend(
                rows.into_iter().map(|index| LineRef { source, index }),
            );
        }
        tx.own = Some(own);
        tx.dest = Some(returned_to(tx));
        Ok(())
    })
}

/// The file that lines were put back into: the first with a line that
/// was empty and is not any more. The todo file when there is none.
fn returned_to(tx: &Tx) -> Source {
    [Source::Todo, Source::Done]
        .into_iter()
        .find(|&source| {
            tx.file(source).rows().iter().any(|row| {
                row.orig.is_empty() && !row.raw.is_empty() && !row.removed
            })
        })
        .unwrap_or(Source::Todo)
}

/// The revert of one file of a record.
struct Plan<'a> {
    rec: &'a FileRecord,
    /// For each line the record wrote, its row now, for a line that is
    /// only a position: the row of a line that moved, too.
    at: Vec<Option<usize>>,
    /// For each line the record wrote, its row now for an action aimed at
    /// it. `None` when the file is as the record left it.
    kept: Option<Vec<Option<usize>>>,
    /// For a row, the line of the record's `pre` it is.
    pre_of: HashMap<usize, usize>,
}

impl<'a> Plan<'a> {
    fn new(tx: &Tx, rec: &'a FileRecord) -> Self {
        let now = &tx.snap.file(rec.source).lines;
        let kept = kept_in(rec.post(), now);
        let at = if kept.is_some() {
            lineid::remap(rec.post(), now)
        } else {
            (0..now.len()).map(Some).collect()
        };
        let mut pre_of = HashMap::new();
        for (pre, post) in rec.pre_to_post().into_iter().enumerate() {
            let row = post.and_then(|p| at.get(p).copied().flatten());
            if let Some(row) = row {
                pre_of.insert(row, pre);
            }
        }
        Self {
            rec,
            at,
            kept,
            pre_of,
        }
    }

    /// The row the change wrote, or `None` when it is gone.
    fn row_index(&self, change: &Change) -> Option<usize> {
        let line = change.nr_after?.index();
        self.at.get(line).copied().flatten()
    }

    /// Whether a line this undo writes on no longer stands as the record
    /// left it, or the lines a move crossed no longer stand side by side.
    fn stale(&self, tx: &Tx) -> bool {
        let Some(kept) = &self.kept else {
            return false;
        };
        let held = |line: usize| kept.get(line).copied().flatten();
        let gone = self.rec.changes.iter().any(|c| {
            must_hold(c.kind)
                && c.nr_after.and_then(|nr| held(nr.index())).is_none()
        });
        let Some(lines) = self.crossed().filter(|_| !gone) else {
            return gone;
        };
        let post = self.rec.post();
        let rows: Option<Vec<usize>> = lines
            .filter(|&i| post.get(i).is_some_and(|l| !Line::is_blank_text(l)))
            .map(held)
            .collect();
        let Some(rows) = rows else {
            return true;
        };
        let Some((&from, &to)) = rows.first().zip(rows.last()) else {
            return false;
        };
        let now = &tx.snap.file(self.rec.source).lines;
        let tasks = now[from..=to].iter();
        tasks.filter(|l| !Line::is_blank_text(l.raw())).count() != rows.len()
    }

    /// The lines of the record from the first to the last that a move left
    /// or took. They must all go back, so none can have gone.
    fn crossed(&self) -> Option<RangeInclusive<usize>> {
        let moved = self.rec.changes.iter();
        let ends = moved
            .filter(|c| c.kind == ChangeKind::Moved)
            .flat_map(|c| [c.nr_before, c.nr_after])
            .flatten()
            .map(Nr::index);
        Some(ends.clone().min()?..=ends.max()?)
    }

    /// Puts the lines a move crossed back in their old order, in the places
    /// they stand in. A blank line put among them from outside stays.
    fn order_moved(&self, order: &mut [usize]) {
        let Some(lines) = self.crossed() else {
            return;
        };
        let rows: HashSet<usize> = lines
            .filter_map(|line| self.at.get(line).copied().flatten())
            .collect();
        let slots: Vec<usize> = (0..order.len())
            .filter(|&s| rows.contains(&order[s]))
            .collect();
        let mut back: Vec<usize> = slots.iter().map(|&s| order[s]).collect();
        back.sort_by_key(|row| self.pre_of.get(row));
        for (slot, row) in slots.into_iter().zip(back) {
            order[slot] = row;
        }
    }

    /// Reverts the file. Gives the rows of the changes that were the
    /// operation's own.
    fn revert(&self, file: &mut StagedFile) -> Vec<usize> {
        let (mut back, mut own) = (Vec::new(), Vec::new());
        for change in &self.rec.changes {
            let row = self.revert_change(file, change, &mut back);
            own.extend(row.filter(|_| change.own));
        }
        back.sort_by_key(|c| c.nr_before.map(Nr::get));
        // The rows the changes took out leave the order in one pass.
        let mut kept: Vec<usize> = file
            .order()
            .iter()
            .copied()
            .filter(|&i| !file.rows()[i].removed)
            .collect();
        self.order_moved(&mut kept);
        let mut places = Places::new(&kept, &self.pre_of);
        for change in back {
            let Some(line) = change.nr_before.map(Nr::index) else {
                continue;
            };
            let text = share(&change.before);
            let row = file.push_row(Row::added(text, Path::Action, None));
            places.put(row, line);
            own.extend(change.own.then_some(row));
        }
        let mut order = places.order(&kept);
        let last = self.rec.pre().len().checked_sub(1);
        let last_row = last.and_then(|l| places.known.get(&l).copied());
        self.trim_done_end(file, &mut order);
        file.set_order(order);
        if self.rec.open_end
            && let Some(row) = last_row
        {
            file.end_open_after(row);
        }
        own
    }

    /// Takes the lines the operation added off the end of the done file
    /// when they are still its end, so nothing but them goes; a line
    /// appended after them keeps them blank in their place.
    fn trim_done_end(&self, file: &mut StagedFile, order: &mut Vec<usize>) {
        if file.source != Source::Done {
            return;
        }
        let mut added = vec![false; file.rows().len()];
        for c in &self.rec.changes {
            if c.kind == ChangeKind::Added
                && let Some(slot) =
                    self.row_index(c).and_then(|i| added.get_mut(i))
            {
                *slot = true;
            }
        }
        while let Some(&last) = order.last() {
            if !added.get(last).copied().unwrap_or(false) {
                break;
            }
            order.pop();
            file.rows_mut()[last].removed = true;
        }
    }

    /// Reverts one change, or notes the line it has to put back, which has
    /// no blank line left to go into. Gives the row it took back.
    fn revert_change<'c>(
        &self,
        file: &mut StagedFile,
        change: &'c Change,
        back: &mut Vec<&'c Change>,
    ) -> Option<usize> {
        let row = self.row_index(change);
        match (change.kind, row) {
            (ChangeKind::Changed, Some(i)) => {
                set_text(&mut file.rows_mut()[i], &change.before);
                return Some(i);
            }
            (ChangeKind::Added, Some(i)) => {
                unadd(file, i);
                return Some(i);
            }
            (ChangeKind::Moved, Some(i)) => {
                file.rows_mut()[i].moved = true;
                return Some(i);
            }
            (ChangeKind::Blanked | ChangeKind::Removed, _) => {
                let row = row.filter(|&i| file.rows()[i].raw.is_empty());
                if change.before.is_empty() {
                    return None;
                }
                if let Some(i) = row {
                    set_text(&mut file.rows_mut()[i], &change.before);
                    return Some(i);
                }
                back.push(change);
            }
            (
                ChangeKind::Changed | ChangeKind::Added | ChangeKind::Moved,
                None,
            ) => {}
        }
        None
    }
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
    fn new(order: &[usize], pre_of: &HashMap<usize, usize>) -> Self {
        let known = order
            .iter()
            .filter_map(|row| Some((*pre_of.get(row)?, *row)))
            .collect();
        Self {
            known,
            after: HashMap::new(),
            top: Vec::new(),
        }
    }

    fn put(&mut self, row: usize, line: usize) {
        match self.known.range(..line).next_back() {
            Some((_, &anchor)) => {
                self.after.entry(anchor).or_default().push(row);
            }
            None => self.top.push(row),
        }
        self.known.insert(line, row);
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
    matches!(kind, ChangeKind::Changed | ChangeKind::Added)
}

fn set_text(row: &mut Row, text: &[u8]) {
    row.raw = share(text);
    row.blanked = text.is_empty() && !row.orig.is_empty();
}

/// Takes out a line the operation added: the done file keeps a blank
/// line in its place (`trim_done_end` removes those at its end), the todo
/// file loses the line.
fn unadd(file: &mut StagedFile, index: usize) {
    if file.source == Source::Done {
        set_text(&mut file.rows_mut()[index], &[]);
        return;
    }
    file.rows_mut()[index].removed = true;
}
