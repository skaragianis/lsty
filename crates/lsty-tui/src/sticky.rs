//! Sticky items: items that the human's own action made stop matching
//! the view stay in sight until the cursor leaves them.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use lsty_core::{
    errs::{Nr, Source},
    list::Item,
};

use crate::{
    messages,
    model::Model,
    rows::Row,
    track::{Place, Remap, Touched, place_of},
};

/// The text that takes the place of the date and claim columns.
pub(crate) const WORD: &str = "filtered";

/// The lines that are shown though the view does not match them.
#[derive(Debug, Default)]
pub(crate) struct Sticky {
    held: BTreeSet<Place>,
    /// The number the todo file had for the line of an archived item.
    origin: BTreeMap<Place, Nr>,
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
                next.origin.insert(now, moved.place((Source::Todo, was)).1);
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
        let (mut back, rest): (Vec<_>, Vec<_>) = items
            .into_iter()
            .partition(|item| self.origin.contains_key(&place_of(item)));
        back.sort_by_key(|item| self.origin[&place_of(item)]);
        let mut back = back.into_iter().peekable();
        let mut out = Vec::new();
        for item in rest {
            let past = |back: &&Item| {
                item.source() == Source::Done
                    || self.origin[&place_of(back)] <= item.nr()
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
    /// unmarks those it does. They are places of the current reading. An
    /// item that went from the todo file to the done file notes where it
    /// stood, so that it is drawn there while sticky.
    pub(crate) fn stick(&mut self, moved: &Remap, touched: &[Touched]) {
        let held = {
            let scope = self.scope();
            let hold = |at: &Place| {
                let item = self.snap.at(at.0, at.1)?;
                Some(!scope.holds(&self.snap, item))
            };
            touched
                .iter()
                .filter_map(|t| hold(&t.now).map(|h| (t, h)))
                .collect::<Vec<_>>()
        };
        for (line, sticks) in held {
            let at = line.now;
            if !sticks {
                self.sticky.drop_at(at);
                continue;
            }
            self.sticky.hold(at);
            if let Some(was) = line.was
                && at.0 == Source::Done
                && was.0 == Source::Todo
            {
                self.sticky.origin.insert(at, moved.place(was).1);
            }
        }
    }

    /// Puts the cursor on the first touched item when it is sticky, and
    /// adds the sticky notice to the message when any touched item is
    /// shown as sticky.
    pub(crate) fn keep_sticky(&mut self, touched: &[Touched]) {
        if let Some(first) = touched.first().map(|t| t.now)
            && self.sticky.has(first)
            && let Some(i) = self.row_at(first)
        {
            self.cursor = i;
            self.scroll();
        }
        let stuck: HashSet<Place> = self
            .rows
            .iter()
            .filter(|r| r.sticky)
            .map(Row::place)
            .collect();
        if touched.iter().any(|t| stuck.contains(&t.now)) {
            self.note(messages::STICKY);
        }
    }

    /// Adds a notice to the status message, or shows it alone.
    pub(crate) fn note(&mut self, text: &str) {
        match &mut self.message {
            Some(message) => {
                message.push_str("; ");
                message.push_str(text);
            }
            None => self.message = Some(text.to_owned()),
        }
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
