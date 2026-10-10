//! The changes of one operation, staged until it has passed every check.

use std::{collections::HashSet, sync::Arc};

use super::{
    env::Env,
    refs::{Target, find, no_item},
    staged::{LineRef, Path, Row, StagedFile},
};
use crate::{
    discover::FileState,
    errs::{Code, Error, Field, Result, Source},
    list::{Item, ItemId, Snapshot, State},
    todotxt::{Line, share},
};

/// What a target names once it is found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Picked {
    /// An item.
    Item(ItemId),
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
    pub(super) dest: Option<Source>,
    pub(super) primary: Vec<LineRef>,
    pub(super) side: Vec<LineRef>,
    pub(super) spawned: Vec<LineRef>,
    pub(super) messages: Vec<Vec<u8>>,
    /// Messages about an id the operation may remove: the message's
    /// index and the id.
    pub(super) lost: Vec<(usize, Vec<u8>)>,
    /// Lines of the other file that a moved line has taken the place of.
    pub(super) paired: HashSet<ItemId>,
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
        let mut found = env.files.clone();
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
            side: Vec::new(),
            spawned: Vec::new(),
            messages: Vec::new(),
            lost: Vec::new(),
            paired: HashSet::new(),
            after: None,
        }
    }

    pub(super) const fn file(&self, source: Source) -> &StagedFile {
        match source {
            Source::Todo => &self.todo,
            Source::Done => &self.done,
        }
    }

    pub(super) const fn file_mut(
        &mut self,
        source: Source,
    ) -> &mut StagedFile {
        match source {
            Source::Todo => &mut self.todo,
            Source::Done => &mut self.done,
        }
    }

    /// The lists as read under the lock.
    pub(super) const fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    /// The item behind `id`, which must come from this transaction.
    pub(super) fn item(&self, id: ItemId) -> Option<&Item> {
        self.snap.item(id)
    }

    /// The line of an item, as read.
    pub(super) fn line(&self, id: ItemId) -> Option<&Line> {
        self.snap.item(id).map(|item| self.snap.line(item))
    }

    /// The line of an item that was resolved a moment ago.
    ///
    /// # Errors
    ///
    /// `no_item` when the item is not in the snapshot, which a resolved
    /// id never is.
    pub(super) fn line_of(&self, id: ItemId) -> Result<&Line> {
        self.line(id).ok_or_else(gone)
    }

    /// The reference to the line of an item.
    pub(super) fn line_ref(&self, id: ItemId) -> Option<LineRef> {
        let item = self.snap.item(id)?;
        Some(self.file(item.source()).line_ref(item.nr().index()))
    }

    /// How messages name an item: the reference its target used, or its
    /// number.
    pub(super) fn handle(&self, id: ItemId) -> String {
        let Some(item) = self.snap.item(id) else {
            return String::new();
        };
        let row = self.row(item);
        match row.and_then(|r| r.handle.as_ref()) {
            Some(handle) => handle.clone(),
            None => item.handle(),
        }
    }

    /// The reference as the caller wrote it; the handle when it was not
    /// given.
    pub(super) fn given(&self, id: ItemId) -> String {
        let given = self
            .snap
            .item(id)
            .and_then(|item| self.row(item))
            .and_then(|row| row.given.clone());
        given.unwrap_or_else(|| self.handle(id))
    }

    /// The line of an item as staged now.
    pub(super) fn current(&self, id: ItemId) -> Option<Line> {
        let item = self.snap.item(id)?;
        let row = self.row(item)?;
        Some(Line::parse_shared(&row.raw))
    }

    fn row(&self, item: &Item) -> Option<&Row> {
        self.file(item.source()).rows().get(item.nr().index())
    }

    /// Finds the item `target` names. A blank line is gone; a line that
    /// is not in the file is `no_item`; an item of the done file is
    /// refused unless the target accepts it.
    pub(super) fn target(&mut self, target: &Target) -> Result<Picked> {
        let handle = target.handle();
        let Some(item) = find(&self.snap, target)? else {
            let source = target.line().map_or(Source::Todo, |(s, _)| s);
            return Ok(Picked::Gone(gone_message(source, &handle)));
        };
        if item.source() == Source::Done && !target.is_archived() {
            return Err(archived_error(&self.snap, item, &handle));
        }
        let (source, index, id) =
            (item.source(), item.nr().index(), item.item_id());
        if let Some(row) = self.file_mut(source).row_mut(index)
            && row.handle.is_none()
        {
            row.handle = Some(handle);
            row.given =
                Some(target.given().to_owned()).filter(|g| !g.is_empty());
        }
        Ok(Picked::Item(id))
    }

    /// Finds the item `target` names. A blank line is said to be gone and
    /// gives `None`.
    pub(super) fn pick(&mut self, target: &Target) -> Result<Option<ItemId>> {
        match self.target(target)? {
            Picked::Item(id) => Ok(Some(id)),
            Picked::Gone(message) => {
                self.say(message);
                Ok(None)
            }
        }
    }

    /// The item an anchor names: it must be an item of the todo file.
    ///
    /// # Errors
    ///
    /// `no_item` for a blank line or a line that is not there,
    /// `wrong_state` for an item of the done file.
    pub(super) fn anchor(&mut self, anchor: &Target) -> Result<ItemId> {
        let anchor = anchor.clone().current_only();
        match self.target(&anchor)? {
            Picked::Item(id) => Ok(id),
            Picked::Gone(_) => Err(no_item(&anchor)),
        }
    }

    /// Marks the line of an item as one the operation is about.
    pub(super) fn mark_primary(&mut self, id: ItemId) {
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
    pub(super) fn each_item<F>(
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
    pub(super) fn set(
        &mut self,
        id: ItemId,
        raw: impl AsRef<[u8]>,
        path: Path,
    ) {
        let Some(item) = self.snap.item(id) else {
            return;
        };
        let (source, index) = (item.source(), item.nr().index());
        if let Some(row) = self.file_mut(source).row_mut(index) {
            let raw = raw.as_ref();
            if *row.raw != *raw {
                row.raw = share(raw);
                row.path = row.path.max(path);
            }
        }
    }

    /// The staged row of an item.
    pub(super) fn row_mut(&mut self, id: ItemId) -> Option<&mut Row> {
        let item = self.snap.item(id)?;
        let (source, index) = (item.source(), item.nr().index());
        self.file_mut(source).row_mut(index)
    }

    /// Stages an item as an empty line that keeps its place.
    pub(super) fn blank(&mut self, id: ItemId) {
        let Some(item) = self.snap.item(id) else {
            return;
        };
        let (source, index) = (item.source(), item.nr().index());
        if let Some(row) = self.file_mut(source).row_mut(index) {
            row.raw = Arc::default();
            row.blanked = true;
        }
    }

    /// Stages a new line after line `after` of `source`; 0 is the top.
    pub(super) fn insert(
        &mut self,
        source: Source,
        after: usize,
        raw: impl AsRef<[u8]>,
        path: Path,
    ) -> LineRef {
        let index =
            self.file_mut(source)
                .insert(after, share(raw.as_ref()), path);
        self.file(source).line_ref(index)
    }

    /// Stages a new line at the end of `source`.
    pub(super) fn append(
        &mut self,
        source: Source,
        raw: impl AsRef<[u8]>,
        path: Path,
    ) -> LineRef {
        let index = self.file_mut(source).append(share(raw.as_ref()), path);
        self.file(source).line_ref(index)
    }

    /// Stages items, all of one file, after line `after` of it; 0 is the
    /// top.
    pub(super) fn relocate(&mut self, ids: &[ItemId], after: usize) {
        let items: Vec<_> =
            ids.iter().filter_map(|&id| self.snap.item(id)).collect();
        let Some(first) = items.first() else {
            return;
        };
        let source = first.source();
        let rows: Vec<usize> = items.iter().map(|i| i.nr().index()).collect();
        self.file_mut(source).relocate(&rows, after);
    }

    /// Names the file the operation moves lines into; it is written
    /// first.
    pub(super) const fn dest(&mut self, source: Source) {
        self.dest = Some(source);
    }

    /// Marks the lines the operation is about; they are reported as
    /// items.
    pub(super) fn primary(&mut self, refs: &[LineRef]) {
        self.primary.extend_from_slice(refs);
    }

    /// Marks lines changed as a side effect.
    pub(super) fn side(&mut self, refs: &[LineRef]) {
        self.side.extend_from_slice(refs);
    }

    /// Marks lines the operation created as a by-product.
    pub(super) fn spawn(&mut self, refs: &[LineRef]) {
        self.spawned.extend_from_slice(refs);
    }

    /// Adds a message to the result. It is bytes, because it can carry
    /// the text of a line.
    pub(super) fn say(&mut self, message: impl Into<Vec<u8>>) {
        self.messages.push(message.into());
    }

    /// The error for an edit that would turn a word into a stamp.
    pub(super) fn stamp_word_refusal(&self, id: ItemId) -> Error {
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
    Error::new(Code::NoItem, "no such item")
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

pub(super) fn archived_error(
    snap: &Snapshot,
    item: &Item,
    handle: &str,
) -> Error {
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
