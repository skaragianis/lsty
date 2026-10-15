//! Which tasks an action is about.

use lsty_core::list::TaskId;

use crate::model::Model;

impl Model {
    /// The tasks of an action: the one under the cursor, or `count`
    /// shown tasks from the cursor down (fewer when the list ends).
    /// `count` is 0 when none was typed.
    pub(crate) fn picked(&self, count: usize) -> Vec<TaskId> {
        self.rows
            .iter()
            .skip(self.cursor)
            .take(count.max(1))
            .map(|row| row.id)
            .collect()
    }

    /// The tasks of the shown rows `lo` to `hi`, both included.
    pub(crate) fn row_tasks(&self, lo: usize, hi: usize) -> Vec<TaskId> {
        self.rows
            .get(lo..=hi)
            .unwrap_or_default()
            .iter()
            .map(|row| row.id)
            .collect()
    }
}
