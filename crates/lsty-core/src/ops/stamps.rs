//! The stamps of a line: what a text edit may not change.

use super::staged::{Path, Row};
use super::tx::{Tx, stamped, wrong_state};
use crate::errs::{Code, Error, Field, Result, Source};
use crate::list::{ItemId, State};
use crate::todotxt::Line;

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
        if row.path != Path::Text || row.removed || row.raw == row.orig {
            return Ok(());
        }
        let cur = Line::parse_shared(&row.raw);
        if cur.is_blank() {
            return Ok(());
        }
        if row.added {
            return new_line_error(&cur);
        }
        match row.item {
            Some(id) => self.stamp_error(id, &cur, row.drops_id),
            None => Ok(()),
        }
    }

    fn stamp_error(
        &self,
        id: ItemId,
        cur: &Line,
        drops_id: bool,
    ) -> Result<()> {
        let handle = self.handle(id);
        let Some(item) = self.snap.item(id) else {
            return Ok(());
        };
        let old = self.snap.line(item);
        if let Some(hint) = life_hint(old, cur, &handle) {
            return Err(stamped(&self.snap, id, &handle, &hint));
        }
        if !drops_id && id_changed(self.snap.id_of(item), cur) {
            let id_text = self.snap.id_of(item).unwrap_or_default();
            let hint = format!(
                "an id is never changed; lsty untag {handle} id:{} removes it",
                String::from_utf8_lossy(id_text)
            );
            return Err(stamped(&self.snap, id, &handle, &hint));
        }
        if old.is_done()
            && old.effective_priority() != cur.effective_priority()
        {
            return Err(wrong_state(
                &self.snap,
                item,
                item.state(),
                format!("{handle} is done; reopen it first"),
                format!("use lsty reopen {handle}"),
            ));
        }
        Ok(())
    }
}

/// The hint for a change of the done mark or of a date, if there is one.
fn life_hint(old: &Line, cur: &Line, handle: &str) -> Option<String> {
    if !old.is_done() && cur.is_done() {
        Some(format!("use lsty do {handle}"))
    } else if old.is_done() && !cur.is_done() {
        Some(format!("use lsty reopen {handle}"))
    } else if old.completed() != cur.completed() {
        Some("the completion date is stamped".to_owned())
    } else if old.created() != cur.created() {
        Some("the creation date is stamped".to_owned())
    } else {
        None
    }
}

/// Whether an item that has an id lost it or got another.
fn id_changed(id: Option<&[u8]>, cur: &Line) -> bool {
    id.is_some_and(|id| cur.tags().id != Some(id))
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
