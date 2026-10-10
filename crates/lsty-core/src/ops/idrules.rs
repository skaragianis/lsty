//! The id rules of the rule layer, checked for every staged line
//! whichever operation staged it.

use std::collections::HashMap;

use super::{
    refs::{Target, invalid_id},
    staged::{LineRef, Row},
    tx::Tx,
};
use crate::{
    errs::{Code, Error, Field, Nr, Ref, Result, Source},
    list::{Item, ItemId, State},
    todotxt::{Id, Line, Tags},
};

/// The rows that carry an id: the file and the index into its rows.
type Held = HashMap<Vec<u8>, Vec<LineRef>>;

impl Tx {
    /// Checks the ids of every line that changed.
    pub(super) fn check_ids(&self) -> Result<()> {
        let mut held: Option<Held> = None;
        for source in [Source::Todo, Source::Done] {
            for (index, row) in self.file(source).rows().iter().enumerate() {
                let at = LineRef { source, index };
                self.check_row_id(at, row, &mut held)?;
            }
        }
        Ok(())
    }

    fn check_row_id(
        &self,
        at: LineRef,
        row: &Row,
        held: &mut Option<Held>,
    ) -> Result<()> {
        if row.removed || row.raw == row.orig || Line::is_blank_text(&row.raw)
        {
            return Ok(());
        }
        let cur = Line::parse_shared(&row.raw);
        let tags = cur.tags();
        let (old_id, old_extra) = self.old_ids(row);
        if extra_ids(&tags) > old_extra {
            return Err(self.second_id(at, row, &tags, old_id.as_deref()));
        }
        let Some(id) = tags.id else {
            return Ok(());
        };
        if old_id.as_deref() == Some(id) {
            return Ok(());
        }
        if !Id::is_valid(id) {
            return Err(invalid_id(&String::from_utf8_lossy(id)));
        }
        let held = held.get_or_insert_with(|| self.held_ids());
        self.check_taken(at, id, old_id.as_deref(), held)
    }

    /// The id the line had when read and how many extra `id:` tokens.
    fn old_ids(&self, row: &Row) -> (Option<Vec<u8>>, usize) {
        let Some(item) = row.was().and_then(|id| self.snap.item(id)) else {
            return (None, 0);
        };
        let tags = self.snap.tags(item);
        (tags.id.map(<[u8]>::to_vec), extra_ids(&tags))
    }

    fn held_ids(&self) -> Held {
        let mut held = Held::new();
        for (at, id) in self.live_ids() {
            held.entry(id).or_default().push(at);
        }
        held
    }

    /// The id a row carries. A row as read uses the id the snapshot
    /// found; only a changed row is parsed again.
    fn row_id(&self, row: &Row) -> Option<Vec<u8>> {
        if row.raw == row.orig
            && let Some(id) = row.item
        {
            return self.id_bytes(id);
        }
        Line::parse_shared(&row.raw).tags().id.map(<[u8]>::to_vec)
    }

    /// The id of every line that will be written, with its place.
    pub(super) fn live_ids(&self) -> impl Iterator<Item = (LineRef, Vec<u8>)> {
        [Source::Todo, Source::Done]
            .into_iter()
            .flat_map(move |source| {
                let rows = self.file(source).rows().iter().enumerate();
                rows.filter(|(_, row)| !row.removed).filter_map(
                    move |(index, row)| {
                        let id = self.row_id(row)?;
                        Some((LineRef { source, index }, id))
                    },
                )
            })
    }

    fn check_taken(
        &self,
        at: LineRef,
        id: &[u8],
        old_id: Option<&[u8]>,
        held: &Held,
    ) -> Result<()> {
        let others = held.get(id).map(Vec::as_slice).unwrap_or_default();
        let Some(&holder) = others.iter().find(|&&h| h != at) else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(id).into_owned();
        let handle = self.row_handle(holder);
        let err = Error::new(
            Code::IdTaken,
            format!("id:{text} is taken by {handle}"),
        )
        .with_field("taken", Field::Str(text));
        let err = self
            .row_ref(holder, Some(id))
            .into_iter()
            .fold(err, |err, r| err.with_field("holder", Field::Ref(r)));
        Err(self.with_row_item(err, at, old_id))
    }

    fn second_id(
        &self,
        at: LineRef,
        row: &Row,
        tags: &Tags<'_>,
        old_id: Option<&[u8]>,
    ) -> Error {
        let id = tags.id.unwrap_or_default();
        let err = self.already_has(at, row.item, id, "an item has one id");
        self.with_row_item(err, at, old_id)
    }

    /// `wrong_state`: the row has an id, so it can't take another.
    fn already_has(
        &self,
        at: LineRef,
        item: Option<ItemId>,
        id: &[u8],
        hint: &str,
    ) -> Error {
        let state = item
            .and_then(|i| self.snap.item(i))
            .map_or(State::Todo, Item::state);
        let id = String::from_utf8_lossy(id);
        Error::new(
            Code::WrongState,
            format!("{} already has id:{id}", self.row_handle(at)),
        )
        .with_hint(hint)
        .with_field("state", Field::Str(state.as_str().to_owned()))
    }

    fn with_row_item(
        &self,
        err: Error,
        at: LineRef,
        old_id: Option<&[u8]>,
    ) -> Error {
        let row = self.row_ref(at, old_id);
        row.into_iter().fold(err, Error::with_item)
    }

    /// The line number a row has now: the one it was read at, or its
    /// place among the lines to be written.
    pub(super) fn row_nr(&self, at: LineRef) -> Option<Nr> {
        let file = self.file(at.source);
        match file.rows().get(at.index)?.nr {
            Some(nr) => Some(nr),
            None => Nr::from_index(file.position(at.index)?),
        }
    }

    fn row_ref(&self, at: LineRef, id: Option<&[u8]>) -> Option<Ref> {
        Some(Ref {
            nr: self.row_nr(at)?,
            source: at.source,
            id: id.map(|id| String::from_utf8_lossy(id).into_owned()),
        })
    }

    pub(super) fn row_handle(&self, at: LineRef) -> String {
        let row = self.file(at.source).rows().get(at.index);
        if let Some(handle) = row.and_then(|r| r.handle.clone()) {
            return handle;
        }
        let nr = self.row_nr(at).map_or(0, |nr| i64::from(nr.get()));
        Target::at(at.source, nr).handle()
    }

    /// Whether a line of either file that will be written carries `id`.
    pub(super) fn holds_id(&self, id: &[u8]) -> bool {
        self.live_ids().any(|(_, held)| held == id)
    }

    /// The error for asking a todo row that has an id for another one.
    pub(super) fn id_kept_error(&self, index: usize, have: &[u8]) -> Error {
        let at = LineRef {
            source: Source::Todo,
            index,
        };
        let item = self.todo.rows().get(index).and_then(|row| row.item);
        let err = self.already_has(at, item, have, "an id is never changed");
        self.with_row_item(err, at, Some(have))
    }

    /// The ids of the lines that will be written and are not as they were
    /// read; the others are found in the snapshot.
    pub(super) fn edited_ids(&self) -> Vec<Vec<u8>> {
        [Source::Todo, Source::Done]
            .into_iter()
            .flat_map(|source| self.file(source).rows().iter())
            .filter(|r| !r.removed && r.raw != r.orig)
            .filter_map(|r| {
                Line::parse_shared(&r.raw).tags().id.map(<[u8]>::to_vec)
            })
            .collect()
    }
}

fn extra_ids(tags: &Tags<'_>) -> usize {
    tags.extra.iter().filter(|t| t.key == b"id").count()
}
