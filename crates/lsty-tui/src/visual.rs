//! VISUAL mode: a selection of whole shown rows, from the anchor to the
//! cursor, that an action takes as its items.

use std::collections::HashMap;

use lsty_core::keys::Mode;
use lsty_core::list::ItemId;

use crate::model::Model;
use crate::rows::Row;
use crate::track::{Place, Remap};

impl Model {
    /// `enter_visual`: starts a selection at the cursor's row.
    pub(crate) fn enter_visual(&mut self) {
        let Some(at) = self.cursor_place() else {
            return;
        };
        self.visual = Some(at);
        self.reset_keys();
    }

    /// `exit_visual`: ends the selection; the cursor stays.
    pub(crate) fn exit_visual(&mut self) {
        self.visual = None;
        self.reset_keys();
    }

    /// Drops the keys typed so far in every mode, but an open text line
    /// keeps its own.
    pub(crate) fn reset_keys(&mut self) {
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

    /// The rows of the selection, first and last, when there is one.
    pub(crate) fn selected_rows(&self) -> Option<(usize, usize)> {
        self.visual?;
        Some(self.selection())
    }

    /// The rows from the anchor to the cursor, both included.
    pub(crate) fn selection(&self) -> (usize, usize) {
        let at = self.anchor_row();
        (at.min(self.cursor), at.max(self.cursor))
    }

    /// The row of the anchor; the cursor's when the anchor is not
    /// shown.
    fn anchor_row(&self) -> usize {
        self.visual
            .and_then(|at| self.row_at(at))
            .unwrap_or(self.cursor)
    }

    /// The row of the item `id`.
    pub(crate) fn row_of(&self, id: ItemId) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    /// How many items are selected.
    pub(crate) fn selected_count(&self) -> usize {
        let (lo, hi) = self.selection();
        hi - lo + 1
    }

    /// The items an action is about: the selection, or what a count
    /// asks for in NORMAL mode.
    pub(crate) fn items_of(&self, count: usize) -> Vec<ItemId> {
        if self.visual.is_none() {
            return self.picked(count);
        }
        let (lo, hi) = self.selection();
        self.row_items(lo, hi)
    }

    /// The count that repeats an action over `n` items: the selection
    /// repeats as a count.
    pub(crate) const fn count_for(&self, count: usize, n: usize) -> usize {
        if self.visual.is_some() { n } else { count }
    }

    /// Runs a write over `items`. In VISUAL mode the selection ends
    /// first, with the cursor on the first item, and comes back when
    /// the write is refused. Tells whether the write ran.
    pub(crate) fn ending(
        &mut self,
        items: &[ItemId],
        run: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        let Some(anchor) = self.visual else {
            return run(self);
        };
        let cursor = self.cursor;
        if let Some(i) = items.first().and_then(|id| self.row_of(*id)) {
            self.cursor = i;
        }
        self.exit_visual();
        if run(self) {
            return true;
        }
        self.visual = Some(anchor);
        self.cursor = cursor;
        self.reset_keys();
        self.scroll();
        false
    }

    /// The lines of the selected rows, the anchor's first.
    pub(crate) fn selected_places(&self) -> Vec<Place> {
        let Some((lo, hi)) = self.selected_rows() else {
            return Vec::new();
        };
        let mut out: Vec<Place> = self
            .rows
            .get(lo..=hi)
            .unwrap_or_default()
            .iter()
            .map(Row::place)
            .collect();
        if self.anchor_row() > lo {
            out.reverse();
        }
        out
    }

    /// Puts the anchor and the cursor on the rows of the selected lines
    /// that are left after a reading, and ends the selection when none
    /// is.
    pub(crate) fn resume(&mut self, moved: &Remap, selected: &[Place]) {
        let (Some(_), Some(last)) = (self.visual, selected.last()) else {
            return;
        };
        let index = self.row_index();
        let Some(anchor) = survivor(moved, selected.iter(), &index) else {
            self.exit_visual();
            return;
        };
        self.visual = self.rows.get(anchor).map(Row::place);
        let tracked = moved.track(*last).and_then(|at| index.get(&at));
        self.cursor = tracked.copied().unwrap_or_else(|| {
            survivor(moved, selected.iter().rev(), &index).unwrap_or(anchor)
        });
        self.scroll();
    }

    /// The row of every line that is shown.
    fn row_index(&self) -> HashMap<Place, usize> {
        self.rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.place(), i))
            .collect()
    }
}

/// The row of the first of `places` that its line was followed to.
fn survivor<'a>(
    moved: &Remap,
    places: impl Iterator<Item = &'a Place>,
    index: &HashMap<Place, usize>,
) -> Option<usize> {
    places
        .filter_map(|at| moved.followed(*at))
        .find_map(|at| index.get(&at).copied())
}
