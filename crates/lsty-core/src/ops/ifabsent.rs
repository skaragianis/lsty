//! `add` with `if_absent`: a text that an open item already matches is
//! reported instead of added, and may give that item an id.

use super::add::append_id;
use super::env::Env;
use super::ids::Taken;
use super::staged::{LineRef, Path};
use super::tx::Tx;
use crate::errs::{Code, Error, Result};
use crate::todotxt::{Kind, Line, TokenView};

/// What became of one text of an add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum How {
    /// A new line.
    Added,
    /// An open item was already there.
    Found,
    /// An open item was already there and got an id.
    Named,
}

/// The line one text ended up as.
#[derive(Clone, Copy, Debug)]
pub(super) struct Placed {
    pub(super) line: LineRef,
    pub(super) how: How,
}

/// An open item that a text matched.
#[derive(Clone, Copy, Debug)]
pub(super) struct Found<'a> {
    /// The row of the todo file.
    pub(super) index: usize,
    /// The id that names the item, if the text has one.
    pub(super) key: Option<&'a [u8]>,
    /// Whether the add asks for a random id.
    pub(super) random: bool,
}

/// The tags that do not make one item differ from another.
const IGNORED: [&[u8]; 5] = [b"id", b"dep", b"started", b"owner", b"pri"];

fn counts(token: &TokenView<'_>) -> bool {
    token.kind != Kind::Tag || !IGNORED.contains(&token.key)
}

/// The words of a line that tell it apart from other items.
fn words(line: &Line) -> impl Iterator<Item = &[u8]> {
    line.tokens().filter(counts).map(|t| t.text)
}

fn same_text(a: &Line, b: &Line) -> bool {
    words(a).eq(words(b))
}

impl Tx {
    /// The row of the first open line of the todo file that is the item
    /// `new` describes: the one with the key id, else the first with the
    /// same text. A key id held by a done item or by the done file
    /// finds nothing, so that the add runs into the id rules.
    pub(super) fn find_open(
        &self,
        new: &Line,
        key: Option<&[u8]>,
    ) -> Option<usize> {
        if let Some(key) = key {
            if let Some(index) = self.first_open(|_, id| id == Some(key)) {
                return Some(index);
            }
            if self.holds_id(key) {
                return None;
            }
        }
        self.first_open(|line, _| same_text(line, new))
    }

    fn first_open(
        &self,
        hit: impl Fn(&Line, Option<&[u8]>) -> bool,
    ) -> Option<usize> {
        self.todo.order.iter().copied().find(|&index| {
            self.with_row(index, |line, id| {
                !line.is_blank() && !line.is_done() && hit(line, id)
            })
            .unwrap_or(false)
        })
    }

    /// Calls `f` with the line of a todo row and its id, if it has one.
    /// A row as read is not parsed again.
    fn with_row<R>(
        &self,
        index: usize,
        f: impl FnOnce(&Line, Option<&[u8]>) -> R,
    ) -> Option<R> {
        let row = self.todo.all.get(index)?;
        let item = row.item.and_then(|id| self.snap.item(id));
        if let (true, Some(item)) = (row.raw == row.orig, item) {
            let id = self.snap.id_of(item).filter(|id| !id.is_empty());
            return Some(f(self.snap.line(item), id));
        }
        let line = Line::parse(row.raw.clone());
        let id = line.tags().id.filter(|id| !id.is_empty());
        Some(f(&line, id))
    }

    /// Decides what the add does with the open item at todo row `index`
    /// that a text matched: nothing, or give it an id.
    ///
    /// # Errors
    ///
    /// `wrong_state` when the item has another id than the key, and
    /// whatever drawing or appending an id fails with.
    pub(super) fn settle(
        &mut self,
        env: &Env<'_>,
        found: &Found<'_>,
        taken: &mut Taken,
    ) -> Result<Placed> {
        let Found { index, key, random } = *found;
        let line = self.todo.line_ref(index);
        let same = Placed {
            line,
            how: How::Found,
        };
        let have = self
            .with_row(index, |_, id| id.map(<[u8]>::to_vec))
            .flatten();
        match (have, key) {
            (Some(have), Some(key)) if have != key => {
                return Err(self.id_kept_error(index, &have));
            }
            (Some(_), _) => return Ok(same),
            (None, None) if !random => return Ok(same),
            (None, _) => {}
        }
        let id = match key {
            Some(key) => key.to_vec(),
            None => taken.draw(env.ids)?.into_bytes(),
        };
        self.give_id(index, &id)?;
        self.side(&[line]);
        Ok(Placed {
            line,
            how: How::Named,
        })
    }

    fn give_id(&mut self, index: usize, id: &[u8]) -> Result<()> {
        let Some(row) = self.todo.all.get_mut(index) else {
            return Err(Error::new(Code::Usage, "the matched line is gone"));
        };
        row.raw = append_id(&row.raw, id)?;
        row.path = row.path.max(Path::Action);
        Ok(())
    }
}
