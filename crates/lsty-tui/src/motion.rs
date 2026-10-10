//! Moving the cursor and scrolling the view.

use lsty_core::errs::Source;
use lsty_core::keys::Action;

use crate::model::Model;
use crate::text::window;
use crate::track::Place;

impl Model {
    /// Does a cursor motion. `count` is 0 when none was typed.
    pub(crate) fn motion(&mut self, action: Action, count: usize) {
        if let Some(panel) = self.focus.panel() {
            self.panel_motion(panel, action, count);
            return;
        }
        let was = self.cursor_place();
        self.list_motion(action, count);
        self.release(was);
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the motions move the cursor"
    )]
    fn list_motion(&mut self, action: Action, count: usize) {
        let distance = isize::try_from(count.max(1)).unwrap_or(isize::MAX);
        let last = self.rows.len().saturating_sub(1);
        match action {
            Action::CursorDown => self.move_by(distance),
            Action::CursorUp => self.move_by(-distance),
            Action::GoTop => self.jump(count, 0),
            Action::GoBottom => self.jump(count, last),
            Action::HalfPageDown => self.move_by(self.half_page(count)),
            Action::HalfPageUp => self.move_by(-self.half_page(count)),
            _ => {}
        }
    }

    fn half_page(&self, count: usize) -> isize {
        let rows = if count > 0 {
            count
        } else {
            (self.pane_rows() / 2).max(1)
        };
        isize::try_from(rows).unwrap_or(isize::MAX)
    }

    fn move_by(&mut self, n: isize) {
        let Some(last) = self.rows.len().checked_sub(1) else {
            return;
        };
        self.cursor = self.cursor.saturating_add_signed(n).min(last);
        self.scroll();
    }

    /// Goes to the row of the todo item with NR `count`, or the nearest
    /// row below it; to `row` when there is no count.
    fn jump(&mut self, count: usize, row: usize) {
        if self.rows.is_empty() {
            return;
        }
        let row = if count > 0 { self.row_from(count) } else { row };
        self.cursor = row;
        self.scroll();
    }

    /// The row of the todo item with NR `nr`, else the one with the
    /// smallest NR above it, else the last row.
    fn row_from(&self, nr: usize) -> usize {
        let wanted = u32::try_from(nr).unwrap_or(u32::MAX);
        let best = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.source == Source::Todo && r.nr.get() >= wanted)
            .min_by_key(|(_, r)| r.nr);
        best.map_or_else(|| self.rows.len().saturating_sub(1), |(i, _)| i)
    }

    /// The line the cursor row shows, if there is a row.
    pub(crate) fn cursor_place(&self) -> Option<Place> {
        let row = self.rows.get(self.cursor)?;
        Some(row.place())
    }

    /// Puts the cursor on the first row at or after `place`, in file
    /// order, and keeps that row `offset` rows below the top of the view.
    pub(crate) fn follow(&mut self, place: Option<Place>, offset: usize) {
        self.place_cursor(place);
        self.top = self.cursor.saturating_sub(offset);
        self.scroll();
    }

    /// Puts the cursor on the first row at or after `place`, and moves
    /// the view the least amount that shows it.
    pub(crate) fn settle_cursor(&mut self, place: Option<Place>) {
        self.place_cursor(place);
        self.scroll();
    }

    fn place_cursor(&mut self, place: Option<Place>) {
        let last = self.rows.len().saturating_sub(1);
        self.cursor = place.map_or(0, |at| self.row_at_or_after(at, last));
    }

    fn row_at_or_after(&self, at: Place, last: usize) -> usize {
        let key = |(source, nr): Place| (source != Source::Todo, nr);
        let wanted = key(at);
        let best = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (key(r.place()), i))
            .filter(|(k, _)| *k >= wanted)
            .min();
        best.map_or(last, |(_, i)| i)
    }

    /// Moves the view the least amount that shows the cursor row.
    ///
    /// With a field open, the field is what has to be shown, and the
    /// rows it takes count against the room.
    pub(crate) fn scroll(&mut self) {
        let mut rows = self.pane_rows();
        let mut at = self.cursor;
        let mut count = self.rows.len();
        if let Some(draft) = &self.edit {
            let height = self.field_height();
            rows = (rows + 1).saturating_sub(height).max(1);
            at = self.field_at();
            count += usize::from(!draft.replacing());
            let window = self.follow_window();
            if let Some(draft) = &mut self.edit {
                draft.window = window;
            }
        }
        self.top = window(self.top, at, count, rows);
    }
}
