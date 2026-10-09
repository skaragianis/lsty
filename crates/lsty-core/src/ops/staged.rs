//! The lines of one file as an operation stages them: each line of the
//! file as read, the lines added, and the order they will be written in.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::errs::{Nr, Source};
use crate::list::{Item, ItemId, Snapshot};
use crate::todotxt::{BOM, Eol, File, Line};

/// Where the text of a staged line comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Path {
    /// One of lsty's own actions; its text is not checked for stamps.
    Action,
    /// Text a person typed; stamps must be left alone.
    Text,
}

/// Names a line of the snapshot, or one staged, for the whole
/// transaction, however lines are inserted, moved or removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LineRef {
    pub(super) source: Source,
    pub(super) index: usize,
}

/// One line of a staged file.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the flags are independent facts about one line"
)]
pub(super) struct Row {
    /// The line number it had when read; `None` for an added line.
    pub(super) nr: Option<Nr>,
    /// The item it was when read; `None` for a blank or added line.
    pub(super) item: Option<ItemId>,
    /// For a line made by moving an item, the item as it was read.
    pub(super) origin: Option<ItemId>,
    /// The text as read, sharing the bytes of the line read; empty for
    /// an added line.
    pub(super) orig: Arc<[u8]>,
    /// The text now: the same bytes as `orig` until it is set.
    pub(super) raw: Arc<[u8]>,
    pub(super) eol: Eol,
    pub(super) path: Path,
    /// For an added line, the line it was inserted after.
    pub(super) anchor: Option<usize>,
    pub(super) handle: Option<String>,
    /// The reference as the caller wrote it, when it was given.
    pub(super) given: Option<String>,
    pub(super) added: bool,
    pub(super) blanked: bool,
    pub(super) moved: bool,
    pub(super) removed: bool,
    /// The operation may remove the line's `id:`.
    pub(super) drops_id: bool,
}

/// What an operation did to one line, as far as undo cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// The text differs.
    Changed,
    /// The line is new.
    Added,
    /// The line is gone from the file.
    Removed,
    /// The line is empty and keeps its place.
    Blanked,
    /// The line has another line number.
    Moved,
}

impl Row {
    fn read(index: usize, line: &Line, item: Option<&Item>) -> Self {
        Self {
            nr: Nr::from_index(index),
            item: item.map(Item::item_id),
            origin: None,
            orig: line.shared(),
            raw: line.shared(),
            eol: line.eol(),
            path: Path::Action,
            anchor: None,
            handle: None,
            given: None,
            added: false,
            blanked: false,
            moved: false,
            removed: false,
            drops_id: false,
        }
    }

    pub(super) fn added(
        raw: Arc<[u8]>,
        path: Path,
        anchor: Option<usize>,
    ) -> Self {
        Self {
            nr: None,
            item: None,
            origin: None,
            orig: Arc::default(),
            raw,
            eol: Eol::None,
            path,
            anchor,
            handle: None,
            given: None,
            added: true,
            blanked: false,
            moved: false,
            removed: false,
            drops_id: false,
        }
    }

    /// The item the line was when read: for a line made by a move, the
    /// item that was moved.
    pub(super) fn was(&self) -> Option<ItemId> {
        self.origin.or(self.item)
    }

    /// The text after the change; a removed line has none.
    pub(super) fn after(&self) -> &[u8] {
        if self.removed { &[] } else { &self.raw }
    }

    /// The kind of change, or `None` when the line is as it was read.
    /// `nr` is its line number now.
    pub(super) fn kind(&self, nr: Option<Nr>) -> Option<ChangeKind> {
        if self.added && self.removed {
            return None;
        }
        if self.added {
            return Some(ChangeKind::Added);
        }
        if self.removed {
            return Some(ChangeKind::Removed);
        }
        if self.blanked && self.raw.is_empty() {
            return Some(ChangeKind::Blanked);
        }
        if self.raw != self.orig {
            return Some(ChangeKind::Changed);
        }
        (self.moved && nr != self.nr).then_some(ChangeKind::Moved)
    }
}

/// The lines of one file, staged.
#[derive(Debug)]
pub(super) struct StagedFile {
    source: Source,
    bom: bool,
    /// The ending a line without one gets: the first line's.
    newline: Eol,
    /// Whether the file as read ended without a newline.
    pub(super) last_has_no_eol: bool,
    /// The row an undo restores as the last one; the file then ends without
    /// a newline as it did before the write.
    reopen: Option<usize>,
    create: bool,
    /// The number of lines when read.
    orig_len: usize,
    /// Every row, in the order they were made.
    all: Vec<Row>,
    /// Indexes into `all`, in the order the lines will be written.
    order: Vec<usize>,
    /// Where the last added line went; reset with the state whenever a
    /// row leaves or moves in `order`.
    tail: Option<Tail>,
    /// The state the lines are in; see [`StagedFile::state`].
    state: u64,
}

/// A number no staged file of the process has been in before.
fn new_state() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The last line added after an original line, remembered so that the
/// next one added there does not look for the place again.
#[derive(Clone, Copy, Debug)]
struct Tail {
    /// The original line it was added after, as given.
    after: usize,
    /// Its position in `order` when it was added.
    at: usize,
}

/// A file as it will be written, fixed once planning is over.
#[derive(Debug)]
pub(super) struct SealedFile {
    /// The bytes to write.
    pub(super) bytes: Vec<u8>,
    /// Whether they differ from the file as read.
    pub(super) changed: bool,
    /// Whether the file is written: it changed, or it is new.
    pub(super) writes: bool,
}

impl StagedFile {
    pub(super) fn new(source: Source, snap: &Snapshot, create: bool) -> Self {
        let file: &File = snap.file(source);
        let all: Vec<Row> = file
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let item =
                    Nr::from_index(i).and_then(|nr| snap.at(source, nr));
                Row::read(i, line, item)
            })
            .collect();
        let newline = match file.lines.first().map(Line::eol) {
            Some(Eol::CrLf) => Eol::CrLf,
            Some(Eol::Lf | Eol::None) | None => Eol::Lf,
        };
        Self {
            source,
            bom: file.bom,
            newline,
            last_has_no_eol: file
                .lines
                .last()
                .is_some_and(|l| l.eol() == Eol::None),
            reopen: None,
            create,
            orig_len: file.lines.len(),
            order: (0..all.len()).collect(),
            tail: None,
            all,
            state: new_state(),
        }
    }

    /// The file the lines belong to.
    pub(super) const fn source(&self) -> Source {
        self.source
    }

    /// The state the lines are in. It is new whenever they may have
    /// changed (each way to change them takes a new one, even a change
    /// that is undone later), and no other staged file has had it, so
    /// two equal states are the same bytes to write.
    pub(super) const fn state(&self) -> u64 {
        self.state
    }

    /// Every row, in the order they were made.
    pub(super) fn rows(&self) -> &[Row] {
        &self.all
    }

    /// Indexes into [`StagedFile::rows`], in the order the lines will
    /// be written.
    pub(super) fn order(&self) -> &[usize] {
        &self.order
    }

    /// Row `index`, to change.
    pub(super) fn row_mut(&mut self, index: usize) -> Option<&mut Row> {
        self.state = new_state();
        self.all.get_mut(index)
    }

    /// Every row, to change.
    pub(super) fn rows_mut(&mut self) -> &mut [Row] {
        self.state = new_state();
        &mut self.all
    }

    /// Keeps a row apart from the order and says its index.
    pub(super) fn push_row(&mut self, row: Row) -> usize {
        self.state = new_state();
        self.all.push(row);
        self.all.len() - 1
    }

    /// Drops what was worked out from the order: the state and the insert
    /// cursor.
    fn forget_order(&mut self) {
        self.state = new_state();
        self.tail = None;
    }

    /// Makes `order`, indexes of rows, the written order.
    pub(super) fn set_order(&mut self, order: Vec<usize>) {
        self.forget_order();
        self.order = order;
    }

    /// Takes row `index` out of the written order.
    pub(super) fn leave_out(&mut self, index: usize) {
        self.forget_order();
        self.order.retain(|&i| i != index);
    }

    /// Takes the last row out of the written order.
    pub(super) fn leave_out_last(&mut self) {
        self.forget_order();
        self.order.pop();
    }

    /// Lets the file end without a newline when its last line is row `row`.
    pub(super) fn end_open_after(&mut self, row: usize) {
        self.state = new_state();
        self.reopen = Some(row);
    }

    pub(super) fn line_ref(&self, index: usize) -> LineRef {
        LineRef {
            source: self.source,
            index,
        }
    }

    /// The position of row `index` in the written order.
    pub(super) fn position(&self, index: usize) -> Option<usize> {
        self.order.iter().position(|&i| i == index)
    }

    /// Where a line goes that follows original line `nr`; 0 is the top.
    fn after(&self, nr: usize) -> usize {
        nr.min(self.orig_len)
            .checked_sub(1)
            .and_then(|i| self.position(i))
            .map_or(0, |at| at + 1)
    }

    /// Adds a line after original line `after`, behind the lines already
    /// added there.
    pub(super) fn insert(
        &mut self,
        after: usize,
        raw: Arc<[u8]>,
        path: Path,
    ) -> usize {
        self.state = new_state();
        let mut at = self.run_start(after);
        while let Some(&next) = self.order.get(at) {
            let row = &self.all[next];
            if !(row.added && row.anchor == Some(after)) {
                break;
            }
            at += 1;
        }
        let index = self.all.len();
        self.all.push(Row::added(raw, path, Some(after)));
        self.order.insert(at, index);
        self.tail = Some(Tail { after, at });
        index
    }

    /// Where the lines added after original line `after` begin: for the
    /// anchor of the line added last, the place just behind it, which is
    /// where the walk over the lines added before would start from.
    fn run_start(&self, after: usize) -> usize {
        self.tail
            .filter(|t| t.after == after)
            .map_or_else(|| self.after(after), |t| t.at)
    }

    /// Adds a line at the end of the file, behind every line already
    /// there or added.
    pub(super) fn append(&mut self, raw: Arc<[u8]>, path: Path) -> usize {
        self.state = new_state();
        let index = self.all.len();
        self.all.push(Row::added(raw, path, None));
        self.order.push(index);
        index
    }

    /// Moves row `index` after original line `after`.
    pub(super) fn relocate(&mut self, index: usize, after: usize) {
        if after.min(self.orig_len).checked_sub(1) == Some(index) {
            return;
        }
        self.forget_order();
        if let Some(at) = self.position(index) {
            self.order.remove(at);
        }
        let to = self.after(after);
        self.order.insert(to, index);
        self.all[index].moved = true;
    }

    /// Removes the blank lines.
    pub(super) fn compact(&mut self) {
        self.forget_order();
        let all = &mut self.all;
        self.order.retain(|&i| {
            if !Line::is_blank_text(&all[i].raw) {
                return true;
            }
            all[i].removed = true;
            false
        });
    }

    /// The ending of the row written at `at`. A file that ended without a
    /// newline keeps doing so while its last line is still the last: a
    /// write that adds a line, or takes the last line away, ends the file
    /// with a newline.
    fn ending(&self, at: usize, row: &Row, keeps_open_end: bool) -> Eol {
        if at + 1 == self.order.len() && keeps_open_end {
            Eol::None
        } else if row.eol == Eol::None {
            self.newline
        } else {
            row.eol
        }
    }

    /// The position of every row in the written order, by row index;
    /// `None` for a row that is not written.
    pub(super) fn positions(&self) -> Vec<Option<usize>> {
        let mut at = vec![None; self.all.len()];
        for (pos, &i) in self.order.iter().enumerate() {
            if let Some(slot) = at.get_mut(i) {
                *slot = Some(pos);
            }
        }
        at
    }

    /// Fixes the bytes to write, once planning is over.
    pub(super) fn seal(&self) -> SealedFile {
        let bytes = self.build();
        let changed = !self.reads_as_read(&bytes);
        SealedFile {
            bytes,
            changed,
            writes: self.create || changed,
        }
    }

    /// Whether `bytes` are the file as it was read: its byte order mark,
    /// then the text and ending of each line read, in the order read.
    fn reads_as_read(&self, bytes: &[u8]) -> bool {
        let mut rest = bytes;
        if self.bom {
            let Some(after) = rest.strip_prefix(&BOM) else {
                return false;
            };
            rest = after;
        }
        for row in self.all.iter().take(self.orig_len) {
            let Some(after) = rest
                .strip_prefix(&*row.orig)
                .and_then(|r| r.strip_prefix(row.eol.as_bytes()))
            else {
                return false;
            };
            rest = after;
        }
        rest.is_empty()
    }

    /// Whether the last line written ends without a newline.
    fn keeps_open_end(&self) -> bool {
        // the file still ends as it did: with the line that had no newline,
        // and with no line added
        let as_read = self.last_has_no_eol
            && self.order.last().is_some_and(|&i| i + 1 == self.orig_len)
            && !self.order.iter().any(|&i| self.all[i].added);
        as_read || self.reopen.is_some_and(|r| self.order.last() == Some(&r))
    }

    fn build(&self) -> Vec<u8> {
        let mut out = if self.bom { BOM.to_vec() } else { Vec::new() };
        let keeps_open_end = self.keeps_open_end();
        for (at, &i) in self.order.iter().enumerate() {
            let row = &self.all[i];
            out.extend_from_slice(&row.raw);
            out.extend_from_slice(
                self.ending(at, row, keeps_open_end).as_bytes(),
            );
        }
        out
    }

    /// The file that [`StagedFile::seal`]'s bytes parse to. A line whose
    /// text is that of a line `snap` holds (the row as read, or the item
    /// a moved line was) is that line again; only the other lines are
    /// parsed. Should a written line not read back as itself, the bytes
    /// are parsed whole.
    pub(super) fn after_file(&self, snap: &Snapshot) -> File {
        let keeps_open_end = self.keeps_open_end();
        let read = snap.file(self.source);
        let mut lines = Vec::with_capacity(self.order.len());
        for (at, &i) in self.order.iter().enumerate() {
            let row = &self.all[i];
            let eol = self.ending(at, row, keeps_open_end);
            let starts = at == 0 && !self.bom;
            if !reads_back(&row.raw, eol, starts) {
                return File::parse(&self.build());
            }
            let known = match row.origin {
                Some(origin) => snap.item(origin).map(|item| snap.line(item)),
                None => row.nr.and_then(|nr| read.line(nr)),
            };
            let mut line = match known {
                Some(line) if line.raw() == &*row.raw => line.clone(),
                Some(_) | None => Line::parse_shared(&row.raw),
            };
            line.set_eol(eol);
            lines.push(line);
        }
        let file = File {
            lines,
            bom: self.bom,
        };
        debug_assert_eq!(
            file,
            File::parse(&self.build()),
            "after list of {:?}",
            self.build()
        );
        file
    }

    /// Whether a line that was read has another number, apart from the
    /// rows in `skip`.
    pub(super) fn shifted(&self, skip: &HashSet<usize>) -> bool {
        self.order.iter().enumerate().any(|(at, &i)| {
            let row = &self.all[i];
            row.item.is_some()
                && !skip.contains(&i)
                && Nr::from_index(at) != row.nr
        })
    }
}

/// Whether `raw`, written with `eol`, parses back to one line with that
/// text and ending. `starts` says it opens a file with no byte order
/// mark, where a leading mark would be taken for the file's.
fn reads_back(raw: &[u8], eol: Eol, starts: bool) -> bool {
    let whole = match eol {
        Eol::None => !raw.is_empty(),
        Eol::Lf => raw.last() != Some(&b'\r'),
        Eol::CrLf => true,
    };
    whole && !raw.contains(&b'\n') && !(starts && raw.starts_with(&BOM))
}
