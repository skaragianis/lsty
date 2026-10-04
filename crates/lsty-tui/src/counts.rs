//! Which items an action is about.

use lsty_core::list::ItemId;

use crate::model::Model;

impl Model {
    /// The items of an action: the one under the cursor, or `count`
    /// shown items from the cursor down (fewer when the list ends).
    /// `count` is 0 when none was typed.
    pub(crate) fn picked(&self, count: usize) -> Vec<ItemId> {
        if self.cursor >= self.rows.len() {
            return Vec::new();
        }
        let end = self
            .cursor
            .saturating_add(count.max(1))
            .min(self.rows.len());
        self.row_items(self.cursor, end - 1)
    }

    /// The items of the shown rows `lo` to `hi`, both included.
    pub(crate) fn row_items(&self, lo: usize, hi: usize) -> Vec<ItemId> {
        self.rows
            .get(lo..=hi)
            .unwrap_or_default()
            .iter()
            .map(|row| row.id)
            .collect()
    }
}
