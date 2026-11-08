//! VISUAL mode: a selection of whole shown rows, from the anchor to the
//! cursor, that an action takes as its tasks.

use lsty_core::{keys::Mode, list::TaskId, ops};

use crate::{counts::ids_of, model::Model, rows::Row, track::Place};

impl Model {
    /// `enter_visual`: starts a selection at the cursor's row.
    pub(crate) fn enter_visual(&mut self) {
        let Some(at) = self.cursor_place() else {
            return;
        };
        self.reset_keys();
        self.visual = Some(at);
    }

    /// `exit_visual`: ends the selection; the cursor stays.
    pub(crate) fn exit_visual(&mut self) {
        self.visual = None;
        self.reset_keys();
    }

    /// Says that the list changed under what was under way, which ends a
    /// selection and reads the files again; the cursor stays.
    pub(crate) fn say_changed(&mut self) {
        self.owe_reload();
        self.exit_visual();
        self.alert(ops::LIST_CHANGED.to_owned());
    }

    /// Drops the keys typed so far in every mode, but an open text line
    /// keeps its own and a selection what it knows of its lines.
    pub(crate) fn reset_keys(&mut self) {
        self.keys_lost &= self.visual.is_some();
        for mode in [
            Mode::Normal,
            Mode::Visual,
            Mode::Insert,
            Mode::Completion,
            Mode::Command,
        ] {
            if mode == Mode::Command && self.line.is_some() {
                continue;
            }
            if let Some(matcher) = self.matchers.get_mut(&mode) {
                matcher.reset();
            }
        }
    }

    /// The rows of the selection's ends, when there is one: the anchor, or
    /// the cursor when the anchor is not shown, and the cursor.
    pub(crate) fn selection_ends(&self) -> Option<(usize, usize)> {
        let at = self.row_at(self.visual?).unwrap_or(self.cursor);
        Some((at, self.cursor))
    }

    /// The rows of the selection, first and last, when there is one.
    pub(crate) fn selected_rows(&self) -> Option<(usize, usize)> {
        let (at, cursor) = self.selection_ends()?;
        Some((at.min(cursor), at.max(cursor)))
    }

    /// The row of the task `id`.
    fn row_of(&self, id: TaskId) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    /// The rows an action is about: the selection, or what a count asks
    /// for in NORMAL mode.
    pub(crate) fn rows_of(&self, count: usize) -> &[Row] {
        self.selected_rows().map_or_else(
            || self.picked_rows(count),
            |(lo, hi)| self.rows_between(lo, hi),
        )
    }

    /// The tasks of `rows_of`.
    pub(crate) fn tasks_of(&self, count: usize) -> Vec<TaskId> {
        ids_of(self.rows_of(count))
    }

    /// The count that repeats an action over `n` tasks: the selection
    /// repeats as a count.
    pub(crate) const fn count_for(&self, count: usize, n: usize) -> usize {
        if self.visual.is_some() { n } else { count }
    }

    /// Runs a write over `tasks`. In VISUAL mode the selection ends
    /// first, with the cursor on the first task, and comes back when
    /// the write is refused for anything but a changed list. Tells whether
    /// the write ran.
    pub(crate) fn ending(
        &mut self,
        tasks: &[TaskId],
        run: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        let Some(anchor) = self.visual else {
            return run(self);
        };
        let cursor = self.cursor;
        if let Some(i) = tasks.first().and_then(|id| self.row_of(*id)) {
            self.cursor = i;
        }
        self.exit_visual();
        if run(self) {
            return true;
        }
        if self.message.as_deref() != Some(ops::LIST_CHANGED) {
            self.visual = Some(anchor);
        }
        self.cursor = cursor;
        self.reset_keys();
        self.scroll();
        false
    }

    /// Puts the ends of a selection, lit on the rows `ends` of `old`, back
    /// after a reading: on the lines that `line` finds, else on the same
    /// rows. Only an empty list ends it.
    pub(crate) fn resume(
        &mut self,
        ends: Option<(usize, usize)>,
        old: &[Row],
        line: impl Fn(Place) -> Option<Place>,
    ) {
        let Some((anchor, cursor)) = ends else {
            return;
        };
        let last = self.rows.len().saturating_sub(1);
        let row = |i: usize| {
            let at = line(old.get(i)?.place())?;
            self.row_at(at)
        };
        let (anchor, cursor) =
            row(anchor).zip(row(cursor)).unwrap_or((anchor, cursor));
        self.visual = self.rows.get(anchor.min(last)).map(Row::place);
        self.cursor = cursor.min(last);
    }
}
