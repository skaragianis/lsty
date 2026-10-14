//! The stamps of a line: what a text edit may not change.

use super::{
    pri::reopen_first,
    staged::{Path, Row},
    tx::{Tx, gone, stamped},
};
use crate::{
    errs::{Code, Error, Field, Result, Source, lossy},
    list::{ItemId, State},
    todotxt::Line,
};

impl Tx {
    /// Checks every line staged on the text path against the line it
    /// replaces.
    pub(super) fn check_stamps(&self) -> Result<()> {
        for source in [Source::Todo, Source::Done] {
            for row in self.file(source).rows() {
                self.check_row(row)?;
            }
        }
        Ok(())
    }

    fn check_row(&self, row: &Row) -> Result<()> {
        if row.path == Path::Action
            || row.removed
            || row.raw == row.orig
            || Line::is_blank_text(&row.raw)
        {
            return Ok(());
        }
        let cur = Line::parse_shared(&row.raw);
        if row.added {
            return new_line_error(&cur);
        }
        match row.item {
            Some(id) => self.stamp_error(id, &cur, row),
            None => Ok(()),
        }
    }

    fn stamp_error(&self, id: ItemId, cur: &Line, row: &Row) -> Result<()> {
        let handle = self.handle(id);
        let item = self.snap.item(id).ok_or_else(gone)?;
        let old = self.snap.line(item);
        if let Some(hint) =
            life_hint(old, cur, &handle, row.path == Path::Typed)
        {
            return Err(stamped(&self.snap, id, &handle, &hint));
        }
        if !row.drops_id
            && let Some(old_id) = self.snap.id_of(item)
            && cur.tags().id != Some(old_id)
        {
            let hint = format!(
                "an id is never changed; lsty untag {handle} id:{} removes it",
                lossy(old_id)
            );
            return Err(stamped(&self.snap, id, &handle, &hint));
        }
        if old.is_done()
            && old.effective_priority() != cur.effective_priority()
        {
            return Err(reopen_first(self, id, item));
        }
        Ok(())
    }
}

/// The hint for a change of the done mark or, unless the dates are
/// `typed` text, of a date, if there is one.
fn life_hint(
    old: &Line,
    cur: &Line,
    handle: &str,
    typed: bool,
) -> Option<String> {
    if !old.is_done() && cur.is_done() {
        Some(format!("use lsty do {handle}"))
    } else if old.is_done() && !cur.is_done() {
        Some(format!("use lsty reopen {handle}"))
    } else if typed {
        None
    } else if old.completed() != cur.completed() {
        Some("the completion date is stamped".to_owned())
    } else if old.created() != cur.created() {
        Some("the creation date is stamped".to_owned())
    } else {
        None
    }
}

fn new_line_error(cur: &Line) -> Result<()> {
    if !cur.is_done() {
        return Ok(());
    }
    Err(
        Error::new(Code::WrongState, "a new line can't start as done")
            .with_hint("add it open, then use lsty do")
            .with_field("state", Field::Str(State::Todo.as_str().to_owned())),
    )
}
