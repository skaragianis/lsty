//! Which tasks an action is about.

use lsty_core::list::TaskId;

use crate::{model::Model, rows::Row};

/// The tasks of `rows`.
pub(crate) fn ids_of(rows: &[Row]) -> Vec<TaskId> {
    rows.iter().map(|row| row.id).collect()
}

impl Model {
    /// The rows of an action: the one under the cursor, or `count` shown
    /// rows from the cursor down (fewer when the list ends). `count` is 0
    /// when none was typed.
    pub(crate) fn picked_rows(&self, count: usize) -> &[Row] {
        let rest = self.rows.get(self.cursor..).unwrap_or_default();
        rest.get(..count.max(1)).unwrap_or(rest)
    }

    /// The shown rows `lo` to `hi`, both included.
    pub(crate) fn rows_between(&self, lo: usize, hi: usize) -> &[Row] {
        self.rows.get(lo..=hi).unwrap_or_default()
    }
}
