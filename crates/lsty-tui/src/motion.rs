//! Moving the cursor and scrolling the view.

use lsty_core::errs::Source;
use lsty_core::keys::Action;

use crate::model::Model;

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
        let distance = i64::try_from(count.max(1)).unwrap_or(i64::MAX);
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

    fn half_page(&self, count: usize) -> i64 {
        let rows = if count > 0 {
            count
        } else {
            (self.pane_rows() / 2).max(1)
        };
        i64::try_from(rows).unwrap_or(i64::MAX)
    }

    fn move_by(&mut self, n: i64) {
        if self.rows.is_empty() {
            return;
        }
        let at = i64::try_from(self.cursor).unwrap_or(i64::MAX);
        let last = i64::try_from(self.rows.len() - 1).unwrap_or(i64::MAX);
        let to = at.saturating_add(n).clamp(0, last);
        self.cursor = usize::try_from(to).unwrap_or(0);
        self.scroll();
    }

    /// Goes to the row of the todo item with NR `count`, or the nearest
    /// row below it; to `row` when there is no count.
    fn jump(&mut self, count: usize, row: usize) {
        if self.rows.is_empty() {
            return;
        }
        let row = if count > 0 { self.row_from(count) } else { row };
        let by = i64::try_from(row).unwrap_or(0)
            - i64::try_from(self.cursor).unwrap_or(0);
        self.move_by(by);
    }

    /// The row of the todo item with NR `nr`, else the one with the
    /// smallest NR above it, else the last row.
    fn row_from(&self, nr: usize) -> usize {
        let wanted = u32::try_from(nr).unwrap_or(u32::MAX);
        let best = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.source == Source::Todo && r.nr >= wanted)
            .min_by_key(|(_, r)| r.nr);
        best.map_or_else(|| self.rows.len().saturating_sub(1), |(i, _)| i)
    }

    /// The line the cursor row shows, if there is a row.
    pub(crate) fn cursor_place(&self) -> Option<(Source, u32)> {
        let row = self.rows.get(self.cursor)?;
        Some((row.source, row.nr))
    }

    /// Puts the cursor on the first row at or after `place`, in file
    /// order, and keeps that row `offset` rows below the top of the view.
    pub(crate) fn follow(
        &mut self,
        place: Option<(Source, u32)>,
        offset: usize,
    ) {
        self.place_cursor(place);
        self.top = self.cursor.saturating_sub(offset);
        self.scroll();
    }

    /// Puts the cursor on the first row at or after `place`, and moves
    /// the view the least amount that shows it.
    pub(crate) fn settle_cursor(&mut self, place: Option<(Source, u32)>) {
        self.place_cursor(place);
        self.scroll();
    }

    fn place_cursor(&mut self, place: Option<(Source, u32)>) {
        let last = self.rows.len().saturating_sub(1);
        self.cursor = place.map_or(0, |at| self.row_at_or_after(at, last));
    }

    fn row_at_or_after(&self, at: (Source, u32), last: usize) -> usize {
        let key = |source: Source, nr: u32| (source != Source::Todo, nr);
        let wanted = key(at.0, at.1);
        let best = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (key(r.source, r.nr), i))
            .filter(|(k, _)| *k >= wanted)
            .min();
        best.map_or(last, |(_, i)| i)
    }

    /// Moves the view the least amount that shows the cursor row.
    pub(crate) fn scroll(&mut self) {
        let rows = self.pane_rows();
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + rows {
            self.top = (self.cursor + 1).saturating_sub(rows);
        }
        self.top = self.top.min(self.rows.len().saturating_sub(rows));
    }
}
