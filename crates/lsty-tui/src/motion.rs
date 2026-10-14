//! Moving the cursor and scrolling the view.

use lsty_core::{errs::Source, keys::Action};

use crate::{
    focus::Focus, model::Model, rows::Row, sizes::Sizes, track::Place,
};

/// What a view keeps when the rows are listed again.
#[derive(Clone, Copy)]
pub(crate) enum View {
    /// The cursor row stays this many rows below the top.
    Cursor(usize),
    /// The top stays, with the cursor row out of view.
    Top(usize),
}

/// Where a motion takes the cursor and the view of a pane.
#[derive(Clone, Copy)]
pub(crate) enum Step {
    /// The cursor by this many entries, up when negative.
    By(isize),
    /// To entry number `.0` when there is one (counting from 1), else to
    /// the last entry when `.1` is set and the first otherwise.
    Jump(Option<usize>, bool),
    /// The view by this many rows; the cursor stays on its entry unless
    /// that leaves the view, then it goes to the nearest row in view.
    Scroll(isize),
    /// The view, so that this many halves of it lie above the cursor row
    /// (0 the top, 1 the middle, 2 the bottom).
    Place(usize),
    /// To the n-th row from the top of the view when positive, from the
    /// bottom when negative, to the middle one when 0.
    Row(isize),
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
        let page = lines(count.max(1).saturating_mul(rows.max(1)));
        let nr = (count > 0).then_some(count);
        Some(match action {
            Action::CursorDown => Self::By(one),
            Action::CursorUp => Self::By(-one),
            Action::GoTop => Self::Jump(nr, false),
            Action::GoBottom => Self::Jump(nr, true),
            Action::HalfPageDown => Self::By(half),
            Action::HalfPageUp => Self::By(-half),
            Action::PageDown => Self::By(page),
            Action::PageUp => Self::By(-page),
            Action::ScrollDown => Self::Scroll(one),
            Action::ScrollUp => Self::Scroll(-one),
            Action::CentreView => Self::Place(1),
            Action::TopView => Self::Place(0),
            Action::BottomView => Self::Place(2),
            Action::GoViewTop => Self::Row(one),
            Action::GoViewMiddle => Self::Row(0),
            Action::GoViewBottom => Self::Row(-one),
            _ => return None,
        })
    }

    /// The cursor and the first entry shown after the motion in a pane of
    /// `len` entries with `rows` showing, from the cursor at `at` (none
    /// when it is on no entry) and the view at `top`. The cursor stays
    /// within `0..len` and in view, and the view moves the least that
    /// shows it, unless the motion is about the view.
    pub(crate) fn to(
        self,
        (len, rows): (usize, usize),
        at: Option<usize>,
        top: usize,
    ) -> (usize, usize) {
        self.to_sized(Sizes::flat(len, rows.max(1)), at, top)
    }

    /// `to` in a pane whose entries take `size.height` rows each.
    pub(crate) fn to_sized(
        self,
        size: Sizes<'_>,
        at: Option<usize>,
        top: usize,
    ) -> (usize, usize) {
        let last = size.len.saturating_sub(1);
        let landed = |to: usize| (to, size.window(top, to));
        match self {
            Self::By(n) => landed(
                at.map_or(0, |p| p + 1)
                    .saturating_add_signed(n)
                    .saturating_sub(1)
                    .min(last),
            ),
            Self::Jump(Some(nth), _) => landed((nth - 1).min(last)),
            Self::Jump(None, end) => landed(usize::from(end) * last),
            Self::Scroll(n) => {
                let top = size.scrolled(top, n);
                let bottom =
                    (top + size.whole(top)).saturating_sub(1).min(last);
                (at.unwrap_or(top).clamp(top, bottom), top)
            }
            Self::Place(halves) => {
                let at = at.unwrap_or(top);
                let room = size.rows.saturating_sub(size.height(at));
                (at, size.above(at, room * halves / 2).min(size.last_top()))
            }
            Self::Row(n) => {
                let shown = size.whole(top);
                let row = match n.signum() {
                    0 => shown.saturating_sub(1) / 2,
                    1 => n.unsigned_abs() - 1,
                    _ => shown.saturating_sub(n.unsigned_abs()),
                };
                (top + row.min(shown.saturating_sub(1)), top)
            }
        }
    }

    /// The first line a text shows after the motion, when its view can
    /// start at `0..=last` and starts at `top`.
    pub(crate) fn text_top(self, last: usize, top: usize) -> usize {
        self.to((last + 1, 1), Some(top), top).1
    }

    /// Whether the motion works on the view as drawn (the cursor may be
    /// out of it) rather than on the cursor and what follows it.
    pub(crate) const fn works_on_view(self) -> bool {
        matches!(self, Self::Scroll(_) | Self::Place(_) | Self::Row(_))
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
        self.list_motion(action, count);
    }

    /// Moves the cursor row; a count on `gg` or `G` names a todo item.
    fn list_motion(&mut self, action: Action, count: usize) {
        let rows = self.pane_rows();
        let step =
            Step::of(action, count, rows).filter(|_| !self.rows.is_empty());
        let Some(step) = step else {
            return;
        };
        let step = match step {
            Step::Jump(Some(nr), end) => {
                Step::Jump(Some(self.row_from(nr) + 1), end)
            }
            Step::By(_) => self.by_screen(action, count, step),
            Step::Jump(None, _)
            | Step::Scroll(_)
            | Step::Place(_)
            | Step::Row(_) => step,
        };
        (self.cursor, self.top) = self
            .sized(|size| step.to_sized(size, Some(self.cursor), self.top));
    }

    /// A half page or page step as the items that fill the same cells
    /// from the cursor, when the list wraps (a count of half pages is
    /// items, as for `j`).
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the page moves change"
    )]
    fn by_screen(&self, action: Action, count: usize, step: Step) -> Step {
        let rows = self.pane_rows();
        let (cells, down) = match action {
            Action::HalfPageDown if count == 0 => (rows / 2, true),
            Action::HalfPageUp if count == 0 => (rows / 2, false),
            Action::PageDown => (count.max(1).saturating_mul(rows), true),
            Action::PageUp => (count.max(1).saturating_mul(rows), false),
            _ => return step,
        };
        let n = self.sized(|size| size.reach(self.cursor, cells.max(1), down));
        let n = isize::try_from(n).unwrap_or(isize::MAX);
        Step::By(if down { n } else { -n })
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
        self.top = self.sized(|size| size.scrolled(top, 0));
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
        let window = self.follow_window();
        if let Some(draft) = &mut self.edit {
            draft.window = window;
        }
        let at = self.field_at();
        self.top = self.sized(|size| size.window(self.top, at));
    }
}
