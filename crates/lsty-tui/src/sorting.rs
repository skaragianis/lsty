//! `s`: cycles the order of the list. Nothing is written.

use lsty_core::list::SortField;

use crate::model::Model;

/// The orders `s` goes through, and their names.
const ORDERS: [(SortField, &str); 4] = [
    (SortField::File, "manual"),
    (SortField::Priority, "priority"),
    (SortField::Due, "due"),
    (SortField::Created, "created"),
];

/// The name of `field` as the status line shows it.
pub(crate) fn name_of(field: SortField) -> &'static str {
    ORDERS
        .iter()
        .find(|(f, _)| *f == field)
        .map_or("alpha", |(_, name)| name)
}

impl Model {
    /// The name of the order of the list.
    pub(crate) fn sort_name(&self) -> &'static str {
        name_of(self.sort)
    }

    /// Whether the list is in an order other than the file's.
    pub(crate) fn sorted(&self) -> bool {
        self.sort != SortField::File
    }

    /// `cycle_sort`: manual, priority, due, created, manual. The cursor
    /// stays on its item.
    pub(crate) fn cycle_sort(&mut self) {
        let at = ORDERS.iter().position(|(f, _)| *f == self.sort);
        let next = at.map_or(0, |i| (i + 1) % ORDERS.len());
        self.sort = ORDERS.get(next).map_or(SortField::File, |(f, _)| *f);
        self.reshow();
    }
}
