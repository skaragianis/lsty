//! Sticky items: items that the human's own action made stop matching
//! the view stay in sight until the cursor leaves them.

use std::collections::{BTreeMap, BTreeSet};

use lsty_core::errs::Source;
use lsty_core::list::Item;

use crate::model::Model;
use crate::rows::Row;
use crate::track::{Place, Remap, Touched, place_of};

/// The text that takes the place of the date and claim columns.
pub(crate) const WORD: &str = "filtered";

/// The lines that are shown though the view does not match them.
#[derive(Debug, Default)]
pub(crate) struct Sticky {
    held: BTreeSet<Place>,
    /// Where the todo file had the line of an archived item, in manual
    /// order.
    origin: BTreeMap<Place, Place>,
}

impl Sticky {
    pub(crate) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    pub(crate) fn has(&self, at: Place) -> bool {
        self.held.contains(&at)
    }

    pub(crate) fn clear(&mut self) {
        self.held.clear();
        self.origin.clear();
    }

    pub(crate) fn hold(&mut self, at: Place) {
        self.held.insert(at);
    }

    pub(crate) fn drop_at(&mut self, at: Place) {
        self.held.remove(&at);
        self.origin.remove(&at);
    }

    /// Notes that the item at `at` stood at `was` in the todo file.
    pub(crate) fn came_from(&mut self, at: Place, was: Place) {
        if self.held.contains(&at) {
            self.origin.insert(at, was);
        }
    }

    /// Moves the places to the new reading; those of lines that are gone
    /// are dropped.
    pub(crate) fn remap(&mut self, moved: &Remap) {
        let mut next = Self::default();
        for &at in &self.held {
            let Some(now) = moved.track(at) else {
                continue;
            };
            next.held.insert(now);
            if let Some(&was) = self.origin.get(&at) {
                next.origin.insert(now, moved.place(was));
            }
        }
        *self = next;
    }

    /// `items` with the archived items that are sticky put back among
    /// the todo items, where they came from.
    pub(crate) fn placed<'a>(&self, items: Vec<&'a Item>) -> Vec<&'a Item> {
        if self.origin.is_empty() {
            return items;
        }
        let (mut back, rest): (Vec<&Item>, Vec<&Item>) = items
            .into_iter()
            .partition(|item| self.origin.contains_key(&place_of(item)));
        back.sort_by_key(|item| {
            self.origin.get(&place_of(item)).map(|at| at.1)
        });
        let mut back = back.into_iter().peekable();
        let mut out = Vec::new();
        for item in rest {
            let past = |back: &&Item| {
                item.source() == Source::Done
                    || self
                        .origin
                        .get(&place_of(back))
                        .is_some_and(|at| at.1 <= item.nr())
            };
            while let Some(early) = back.next_if(past) {
                out.push(early);
            }
            out.push(item);
        }
        out.extend(back);
        out
    }
}

impl Model {
    /// Marks the items at `touched` that the view does not match, and
    /// unmarks those it does. They are places of the current reading.
    pub(crate) fn stick(&mut self, touched: &[Touched]) {
        let held = {
            let scope = self.scope();
            let hold = |at: &Place| {
                let item = self.snap.at(at.0, at.1)?;
                Some(!scope.holds(&self.snap, item))
            };
            touched
                .iter()
                .filter_map(|t| hold(&t.now).map(|h| (t.now, h)))
                .collect::<Vec<_>>()
        };
        for (at, sticks) in held {
            if sticks {
                self.sticky.hold(at);
            } else {
                self.sticky.drop_at(at);
            }
        }
    }

    /// Notes where the touched items that went from the todo file to
    /// the done file stood, so that they are drawn there while sticky.
    pub(crate) fn note_origins(&mut self, moved: &Remap, touched: &[Touched]) {
        for line in touched {
            if let Some(was) = line.was
                && line.now.0 == Source::Done
                && was.0 == Source::Todo
            {
                self.sticky.came_from(line.now, moved.place(was));
            }
        }
    }

    /// Puts the cursor on the first touched item when it is sticky, and
    /// tells whether any touched item is shown as sticky.
    pub(crate) fn keep_sticky(&mut self, touched: &[Touched]) -> bool {
        if let Some(first) = touched.first().map(|t| t.now)
            && self.sticky.has(first)
            && let Some(i) = self.row_at(first)
        {
            self.cursor = i;
            self.scroll();
        }
        let stuck: std::collections::HashSet<Place> = self
            .rows
            .iter()
            .filter(|r| r.sticky)
            .map(Row::place)
            .collect();
        touched.iter().any(|t| stuck.contains(&t.now))
    }

    /// The row that shows the line at `at`.
    pub(crate) fn row_at(&self, at: Place) -> Option<usize> {
        self.rows.iter().position(|r| r.place() == at)
    }

    /// Drops the sticky items once the cursor has moved off its row onto
    /// one that is not sticky.
    pub(crate) fn release(&mut self, was: Option<Place>) {
        if self.sticky.is_empty() {
            return;
        }
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if row.sticky || self.cursor_place() == was {
            return;
        }
        self.reshow();
    }
}
