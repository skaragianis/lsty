//! Moving the cursor and scrolling the view.

use lsty_core::{errs::Source, keys::Action};

use crate::{
    focus::Focus,
    model::Model,
    rows::Row,
    text::{scrolled, window},
    track::Place,
};

/// What a view keeps when the rows are listed again.
#[derive(Clone, Copy)]
pub(crate) enum View {
    /// The cursor row stays this many rows below the top.
    Cursor(usize),
    /// The top stays, with the cursor row out of view.
    Top(usize),
}

/// Where a motion takes a position among entries.
#[derive(Clone, Copy)]
pub(crate) enum Step {
    /// By this many entries, up when negative.
    By(isize),
    /// To entry number `.0` when there is one (counting from 1), else to
    /// the last entry when `.1` is set and the first otherwise.
    Jump(Option<usize>, bool),
}

impl Step {
    /// What `action` does with `count` (0 when none was typed) in a pane
    /// that shows `rows` entries; `None` if it is not a motion. This is
    /// the one place that says what the motions mean.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the motions move"
    )]
    pub(crate) fn of(
        action: Action,
        count: usize,
        rows: usize,
    ) -> Option<Self> {
        let lines = |n: usize| isize::try_from(n).unwrap_or(isize::MAX);
        let one = lines(count.max(1));
        let half = if count > 0 {
            one
        } else {
            lines((rows / 2).max(1))
        };
        let nr = (count > 0).then_some(count);
        Some(match action {
            Action::CursorDown => Self::By(one),
            Action::CursorUp => Self::By(-one),
            Action::GoTop => Self::Jump(nr, false),
            Action::GoBottom => Self::Jump(nr, true),
            Action::HalfPageDown => Self::By(half),
            Action::HalfPageUp => Self::By(-half),
            _ => return None,
        })
    }

    /// The position after the motion from `at` (none when the cursor is
    /// not on any entry), staying within `0..=last`.
    pub(crate) fn to(self, at: Option<usize>, last: usize) -> usize {
        match self {
            Self::By(n) => at
                .map_or(0, |p| p + 1)
                .saturating_add_signed(n)
                .saturating_sub(1)
                .min(last),
            Self::Jump(Some(nth), _) => (nth - 1).min(last),
            Self::Jump(None, end) => usize::from(end) * last,
        }
    }
}

impl Model {
    /// Does a cursor motion. `count` is 0 when none was typed.
    pub(crate) fn motion(&mut self, action: Action, count: usize) {
        if self.focus == Focus::Detail {
            self.detail_motion(action, count);
            return;
        }
        if let Some(panel) = self.focus.panel() {
            self.panel_motion(panel, action, count);
            return;
        }
        let was = self.cursor_place();
        self.list_motion(action, count);
        self.release(was);
    }

    /// Moves the cursor row; a count on `gg` or `G` names a todo item.
    fn list_motion(&mut self, action: Action, count: usize) {
        let (Some(last), Some(step)) = (
            self.rows.len().checked_sub(1),
            Step::of(action, count, self.pane_rows()),
        ) else {
            return;
        };
        self.cursor = match step {
            Step::Jump(Some(nr), _) => self.row_from(nr),
            Step::By(_) | Step::Jump(None, _) => {
                step.to(Some(self.cursor), last)
            }
        };
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
        self.rows.get(self.cursor).map(Row::place)
    }

    /// Puts the cursor on the first row at or after `place`, in file
    /// order, and keeps the view as `view` says.
    pub(crate) fn follow(&mut self, place: Option<Place>, view: View) {
        self.place_cursor(place);
        match view {
            View::Cursor(offset) => {
                self.top = self.cursor.saturating_sub(offset);
                self.scroll();
            }
            View::Top(top) => self.keep_top(top),
        }
    }

    /// Puts the top of the view at `top`, or at the last page when there
    /// is less to show, whether or not that shows the cursor row.
    pub(crate) fn keep_top(&mut self, top: usize) {
        self.top = scrolled(top, 0, self.rows.len(), self.pane_rows());
    }

    /// Brings the cursor row back into view after the wheel took it
    /// away, as any key does before it acts.
    pub(crate) fn reveal(&mut self) {
        if self.cursor_hidden() {
            self.scroll();
        }
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
        self.settle_focus();
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
