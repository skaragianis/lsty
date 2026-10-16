//! Where the field of INSERT mode is on the screen: its place among the
//! rows of the list, its window when it is taller than the list, and
//! the text cursor.

use lsty_core::errs::{Nr, Source};

use crate::{layout::BORDER, model::Model, rows::Row};

/// The fewest cells a row of the field holds.
const MIN_FIELD_WIDTH: usize = 2;

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
            return self.row_beyond(Some(nr));
        }
        match draft.place().and_then(|at| self.row_at(at)) {
            None => self.row_beyond(None),
            Some(i) if draft.below() => i + 1,
            Some(i) => i,
        }
    }

    /// The first row at or after line `nr` of the todo file, or after
    /// its last task without one; a row of the done file counts as
    /// beyond every line.
    fn row_beyond(&self, nr: Option<Nr>) -> usize {
        let beyond = |r: &Row| {
            r.source == Source::Done || nr.is_some_and(|nr| r.nr >= nr)
        };
        self.rows.iter().position(beyond).unwrap_or(self.rows.len())
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
        let window = draft.window.min(row).max((row + 1).saturating_sub(room));
        window.min(layout.rows - room)
    }

    /// The line of the list pane the first shown row of the field is on.
    pub(crate) fn field_line(&self) -> usize {
        let at = self.field_at();
        self.sized(|size| {
            let above = size.shown(self.top).into_iter();
            above.take_while(|&(v, _)| v < at).map(|(_, n)| n).sum()
        })
    }

    /// The screen cell of position `p` of the field, not yet held
    /// inside the screen.
    pub(crate) fn field_cell(&self, p: usize) -> Option<(usize, usize)> {
        let draft = self.edit.as_ref()?;
        let (left, width) = self.field_box();
        let cell = draft.field.layout(width).cell(p);
        let list = self.panes(self.dims()).list;
        let x = usize::from(list.x) + left + cell.col + 1;
        let y =
            (1 + self.field_line() + cell.row).saturating_sub(draft.window);
        Some((x, y))
    }

    /// Where the text cursor of the field is on the screen.
    pub(crate) fn field_cursor(&self) -> Option<(u16, u16)> {
        let (x, y) = self.field_cell(self.edit.as_ref()?.field.cursor)?;
        let x = u16::try_from(x).ok()?.min(self.width.saturating_sub(1));
        let y = u16::try_from(y).ok()?.min(self.height.saturating_sub(1));
        Some((x, y))
    }
}
