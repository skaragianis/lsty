//! The changes of one operation, staged until it has passed every check.

use std::collections::HashSet;

use super::refs::{Target, find};
use super::staged::{LineRef, Path, StagedFile};
use crate::errs::{Code, Error, Field, Result, Source};
use crate::list::{Item, ItemId, Snapshot, State};
use crate::todotxt::Line;

/// What a target names once it is found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// An item.
    Item(ItemId),
    /// A blank line: the target is already deleted (todo file) or
    /// restored (done file). It holds the message that says so.
    Gone(String),
}

/// Stages the changes of one operation. Nothing is written until the
/// operation has planned and the rules have passed.
#[derive(Debug)]
pub struct Tx {
    pub(super) snap: Snapshot,
    pub(super) todo: StagedFile,
    pub(super) done: StagedFile,
    pub(super) dest: Option<Source>,
    pub(super) primary: Vec<LineRef>,
    pub(super) side: Vec<LineRef>,
    pub(super) spawned: Vec<LineRef>,
    pub(super) messages: Vec<Vec<u8>>,
}

impl Tx {
    pub(super) fn new(snap: Snapshot, create: bool) -> Self {
        Self {
            todo: StagedFile::new(Source::Todo, &snap, create),
            done: StagedFile::new(Source::Done, &snap, false),
            snap,
            dest: None,
            primary: Vec::new(),
            side: Vec::new(),
            spawned: Vec::new(),
            messages: Vec::new(),
        }
    }

    pub(super) const fn file(&self, source: Source) -> &StagedFile {
        match source {
            Source::Todo => &self.todo,
            Source::Done => &self.done,
        }
    }

    const fn file_mut(&mut self, source: Source) -> &mut StagedFile {
        match source {
            Source::Todo => &mut self.todo,
            Source::Done => &mut self.done,
        }
    }

    /// The lists as read under the lock.
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    /// The item behind `id`, which must come from this transaction.
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.snap.item(id)
    }

    /// The line of an item, as read.
    pub fn line(&self, id: ItemId) -> Option<&Line> {
        self.snap.item(id).map(|item| self.snap.line(item))
    }

    /// The reference to the line of an item.
    pub fn line_ref(&self, id: ItemId) -> Option<LineRef> {
        let item = self.snap.item(id)?;
        Some(self.file(item.source()).line_ref(item.nr().index()))
    }

    /// How messages name an item: the reference its target used, or its
    /// number.
    pub fn handle(&self, id: ItemId) -> String {
        let Some(item) = self.snap.item(id) else {
            return String::new();
        };
        let row = self.row(item);
        match row.and_then(|r| r.handle.as_ref()) {
            Some(handle) => handle.clone(),
            None => match item.source() {
                Source::Todo => item.nr().to_string(),
                Source::Done => format!("done:{}", item.nr()),
            },
        }
    }

    fn row(&self, item: &Item) -> Option<&super::staged::Row> {
        self.file(item.source()).all.get(item.nr().index())
    }

    /// Finds the item `target` names. A blank line is gone; a line that
    /// is not in the file is `no_item`; an item of the done file is
    /// refused unless the target accepts it.
    pub fn target(&mut self, target: &Target) -> Result<Picked> {
        let handle = target.handle();
        let Some(item) = find(&self.snap, target)? else {
            return Ok(Picked::Gone(gone_message(target.source(), &handle)));
        };
        if item.source() == Source::Done && !target.is_archived() {
            return Err(archived_error(&self.snap, item, &handle));
        }
        let (source, index, id) =
            (item.source(), item.nr().index(), item.item_id());
        if let Some(row) = self.file_mut(source).all.get_mut(index)
            && row.handle.is_none()
        {
            row.handle = Some(handle);
        }
        Ok(Picked::Item(id))
    }

    /// Finds the item `target` names. A blank line is said to be gone and
    /// gives `None`.
    pub fn pick(&mut self, target: &Target) -> Result<Option<ItemId>> {
        match self.target(target)? {
            Picked::Item(id) => Ok(Some(id)),
            Picked::Gone(message) => {
                self.say(message);
                Ok(None)
            }
        }
    }

    /// Marks the line of an item as one the operation is about.
    pub fn mark_primary(&mut self, id: ItemId) {
        if let Some(line) = self.line_ref(id) {
            self.primary(&[line]);
        }
    }

    /// Runs `each` on every item the targets name, once per item and in
    /// the order given, and marks each as primary. A blank line is said
    /// to be gone and skipped.
    ///
    /// # Errors
    ///
    /// The first error of a target or of `each`.
    pub fn each_item<F>(
        &mut self,
        targets: &[Target],
        mut each: F,
    ) -> Result<()>
    where
        F: FnMut(&mut Self, ItemId) -> Result<()>,
    {
        let mut seen = HashSet::new();
        for target in targets {
            let Some(id) = self.pick(target)? else {
                continue;
            };
            if seen.insert(id) {
                self.mark_primary(id);
                each(self, id)?;
            }
        }
        Ok(())
    }

    /// Stages `raw` as the new text of an item. The same text as now
    /// stages nothing.
    pub fn set(&mut self, id: ItemId, raw: impl Into<Vec<u8>>, path: Path) {
        let Some(item) = self.snap.item(id) else {
            return;
        };
        let (source, index) = (item.source(), item.nr().index());
        if let Some(row) = self.file_mut(source).all.get_mut(index) {
            let raw = raw.into();
            if row.raw != raw {
                row.raw = raw;
                row.path = row.path.max(path);
            }
        }
    }

    /// Stages an item as an empty line that keeps its place.
    pub fn blank(&mut self, id: ItemId) {
        let Some(item) = self.snap.item(id) else {
            return;
        };
        let (source, index) = (item.source(), item.nr().index());
        if let Some(row) = self.file_mut(source).all.get_mut(index) {
            row.raw.clear();
            row.blanked = true;
        }
    }

    /// Stages a new line after line `after` of `source`; 0 is the top.
    pub fn insert(
        &mut self,
        source: Source,
        after: usize,
        raw: impl Into<Vec<u8>>,
        path: Path,
    ) -> LineRef {
        let index = self.file_mut(source).insert(after, raw.into(), path);
        self.file(source).line_ref(index)
    }

    /// Stages an item after line `after` of its file; 0 is the top.
    pub fn relocate(&mut self, id: ItemId, after: usize) {
        let Some(item) = self.snap.item(id) else {
            return;
        };
        let (source, index) = (item.source(), item.nr().index());
        self.file_mut(source).relocate(index, after);
    }

    /// Stages the removal of the blank lines of `source`.
    pub fn compact(&mut self, source: Source) {
        self.file_mut(source).compact();
    }

    /// Names the file the operation moves lines into; it is written
    /// first.
    pub const fn dest(&mut self, source: Source) {
        self.dest = Some(source);
    }

    /// Marks the lines the operation is about; they are reported as
    /// items.
    pub fn primary(&mut self, refs: &[LineRef]) {
        self.primary.extend_from_slice(refs);
    }

    /// Marks lines changed as a side effect.
    pub fn side(&mut self, refs: &[LineRef]) {
        self.side.extend_from_slice(refs);
    }

    /// Marks lines the operation created as a by-product.
    pub fn spawn(&mut self, refs: &[LineRef]) {
        self.spawned.extend_from_slice(refs);
    }

    /// Adds a message to the result. It is bytes, because it can carry
    /// the text of a line.
    pub fn say(&mut self, message: impl Into<Vec<u8>>) {
        self.messages.push(message.into());
    }

    /// The error for an edit that would turn a word into a stamp.
    pub fn stamp_word_refusal(&self, id: ItemId) -> Error {
        let handle = self.handle(id);
        stamped(
            &self.snap,
            id,
            &handle,
            "the text would start with a stamp-like word; reword it first",
        )
    }
}

fn gone_message(source: Source, handle: &str) -> String {
    match source {
        Source::Done => format!("{handle} already restored"),
        Source::Todo => format!("{handle} already deleted"),
    }
}

/// A `wrong_state` error about `item`, whose `state` field is `state`.
pub(super) fn wrong_state(
    snap: &Snapshot,
    item: &Item,
    state: State,
    message: String,
    hint: String,
) -> Error {
    Error::new(Code::WrongState, message)
        .with_hint(hint)
        .with_item(snap.ref_of(item))
        .with_field("state", Field::Str(state.as_str().to_owned()))
}

fn archived_error(snap: &Snapshot, item: &Item, handle: &str) -> Error {
    wrong_state(
        snap,
        item,
        State::Done,
        format!("{handle} is read-only history"),
        format!("{handle} is archived; use restore"),
    )
}

/// The refusal of a text edit that changes a stamp.
pub(super) fn stamped(
    snap: &Snapshot,
    id: ItemId,
    handle: &str,
    hint: &str,
) -> Error {
    let message = format!("{handle}: a text edit can't change a stamp");
    match snap.item(id) {
        Some(item) => {
            wrong_state(snap, item, item.state(), message, hint.to_owned())
        }
        None => Error::new(Code::WrongState, message).with_hint(hint),
    }
}
