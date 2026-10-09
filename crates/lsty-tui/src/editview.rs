//! Where the field of INSERT mode is on the screen: its rows among the
//! rows of the list, its window when it is taller than the list, and
//! the text cursor.

use lsty_core::errs::{Nr, Source};

use crate::layout::BORDER;
use crate::model::Model;

/// The fewest cells a row of the field holds.
const MIN_FIELD_WIDTH: usize = 2;

/// What a line of the list pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Body {
    /// The row of the list at this index.
    Row(usize),
    /// A row of the field; `first` for the first one that is shown.
    Field { row: usize, first: bool },
}

/// Where the field is among the rows of the list.
struct Place {
    /// The row of the list the field takes the place of, or goes before.
    at: usize,
    /// How many rows the field wraps onto.
    height: usize,
    /// Whether the field stands in the row of its item.
    replacing: bool,
    /// The first row of the field that is shown.
    window: usize,
}

impl Place {
    /// Adds the lines that the row `v` of the list takes.
    fn push(&self, out: &mut Vec<Body>, v: usize) {
        if v == self.at {
            let from = self.window.min(self.height.saturating_sub(1));
            out.extend((from..self.height).map(|row| Body::Field {
                row,
                first: row == from,
            }));
        } else if v > self.at && !self.replacing {
            out.push(Body::Row(v - 1));
        } else {
            out.push(Body::Row(v));
        }
    }
}

impl Model {
    /// Where the field starts in a row of the list, and how many cells
    /// a wrapped row of it holds.
    pub(crate) fn field_box(&self) -> (usize, usize) {
        let inner = self.geometry(self.dims()).list.w.saturating_sub(BORDER);
        let mut left = 1;
        let gutter = self.gutter_width();
        if gutter > 0 {
            left += gutter + 1;
        }
        let width = inner.saturating_sub(left + 1).max(MIN_FIELD_WIDTH);
        (left, width)
    }

    /// How many rows the field wraps onto.
    pub(crate) fn field_height(&self) -> usize {
        let width = self.field_box().1;
        self.edit.as_ref().map_or(0, |d| d.field.layout(width).rows)
    }

    /// The row of the list the field takes the place of, or goes before.
    pub(crate) fn field_at(&self) -> usize {
        let Some(draft) = &self.edit else {
            return self.cursor;
        };
        if draft.replacing() {
            return draft
                .place()
                .and_then(|at| self.row_at(at))
                .unwrap_or(self.cursor);
        }
        if let Some((_, nr)) = self.pin_spot().filter(|_| !draft.pin_is_live())
        {
            return self.row_beyond(nr);
        }
        match draft.place().and_then(|at| self.row_at(at)) {
            None => self.end_row(),
            Some(i) if draft.below() => i + 1,
            Some(i) => i,
        }
    }

    /// The first row at or after line `nr` of the todo file; a row of
    /// the done file counts as beyond every line.
    fn row_beyond(&self, nr: Nr) -> usize {
        self.rows
            .iter()
            .position(|r| r.source == Source::Done || r.nr >= nr)
            .unwrap_or(self.rows.len())
    }

    /// The row after the last item of the todo file.
    fn end_row(&self) -> usize {
        self.rows
            .iter()
            .position(|r| r.source == Source::Done)
            .unwrap_or(self.rows.len())
    }

    /// The first row of a field that is taller than the list, moved the
    /// least amount that shows the text cursor.
    pub(crate) fn follow_window(&self) -> usize {
        let Some(draft) = &self.edit else {
            return 0;
        };
        let layout = draft.field.layout(self.field_box().1);
        let room = self.pane_rows();
        if layout.rows <= room {
            return 0;
        }
        let row = layout.cell(draft.field.cursor).row;
        let mut window = draft.window;
        if row < window {
            window = row;
        } else if row >= window + room {
            window = row + 1 - room;
        }
        window.min(layout.rows - room)
    }

    /// The lines of the list pane, `room` of them from the top row on:
    /// the field takes as many lines as it wraps onto.
    pub(crate) fn body_lines(&self, room: usize) -> Vec<Body> {
        let Some(draft) = &self.edit else {
            return (self.top..self.top + room).map(Body::Row).collect();
        };
        let place = Place {
            at: self.field_at(),
            height: self.field_height(),
            replacing: draft.replacing(),
            window: draft.window,
        };
        let mut out = Vec::with_capacity(room);
        let mut v = self.top;
        while out.len() < room {
            place.push(&mut out, v);
            v += 1;
        }
        out.truncate(room);
        out
    }

    /// Where the text cursor of the field is on the screen.
    pub(crate) fn field_cursor(&self) -> Option<(u16, u16)> {
        let draft = self.edit.as_ref()?;
        let (left, width) = self.field_box();
        let cell = draft.field.layout(width).cell(draft.field.cursor);
        let list = self.panes(self.dims()).list;
        let x = usize::from(list.x) + left + cell.col + 1;
        let y = (1 + self.field_at() + cell.row)
            .saturating_sub(self.top + draft.window);
        let x = u16::try_from(x).ok()?.min(self.width.saturating_sub(1));
        let y = u16::try_from(y).ok()?.min(self.height.saturating_sub(1));
        Some((x, y))
    }
}
