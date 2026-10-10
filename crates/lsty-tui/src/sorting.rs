//! `s`: cycles the order of the list. Nothing is written.

use lsty_core::list::SortField;

use crate::model::Model;

/// The orders `s` goes through.
const ORDERS: [SortField; 4] = [
    SortField::File,
    SortField::Priority,
    SortField::Due,
    SortField::Created,
];

impl Model {
    /// Whether the list is in an order other than the file's.
    pub(crate) fn sorted(&self) -> bool {
        self.sort != SortField::File
    }

    /// `cycle_sort`: manual, priority, due, created, manual. The cursor
    /// stays on its item.
    pub(crate) fn cycle_sort(&mut self) {
        let at = ORDERS.iter().position(|f| *f == self.sort);
        self.sort = ORDERS[at.map_or(0, |i| (i + 1) % ORDERS.len())];
        self.reshow();
    }
}
