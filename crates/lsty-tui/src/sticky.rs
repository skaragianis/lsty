//! Sticky tasks: tasks that the human's own action made stop matching
//! the view stay in sight until the next key or mouse input, or until the
//! notice that tells of them is over.

use std::collections::{BTreeMap, HashSet};

use lsty_core::{
    errs::{Nr, Source},
    list::Task,
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
    /// Each line held, with whether the input being handled held it.
    held: BTreeMap<Place, bool>,
    /// The number the todo file had for the line of an archived task.
    origin: BTreeMap<Place, Nr>,
}

impl Sticky {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    pub(crate) fn has(&self, at: Place) -> bool {
        self.held.contains_key(&at)
    }

    pub(crate) fn clear(&mut self) {
        self.held.clear();
        self.origin.clear();
    }

    pub(crate) fn hold(&mut self, at: Place) {
        self.held.insert(at, true);
    }

    /// Makes every line held one that an earlier input held.
    pub(crate) fn age(&mut self) {
        self.held.values_mut().for_each(|fresh| *fresh = false);
    }

    /// Drops the lines an earlier input held, but `keep`; whether there
    /// were any.
    pub(crate) fn drop_aged(&mut self, keep: Option<Place>) -> bool {
        let before = self.held.len();
        self.held.retain(|at, fresh| *fresh || Some(*at) == keep);
        self.origin.retain(|at, _| self.held.contains_key(at));
        self.held.len() != before
    }

    pub(crate) fn drop_at(&mut self, at: Place) {
        self.held.remove(&at);
        self.origin.remove(&at);
    }

    /// Moves the places to the new reading; those of lines that are gone
    /// are dropped.
    pub(crate) fn remap(&mut self, moved: &Remap) {
        let mut next = Self::default();
        for (&at, &fresh) in &self.held {
            let Some(now) = moved.track(at) else {
                continue;
            };
            next.held.insert(now, fresh);
            if let Some(&was) = self.origin.get(&at) {
                next.origin.insert(now, moved.place((Source::Todo, was)).1);
            }
        }
        *self = next;
    }

    /// `tasks` with the archived tasks that are sticky put back among
    /// the todo tasks, where they came from.
    pub(crate) fn placed<'a>(&self, tasks: Vec<&'a Task>) -> Vec<&'a Task> {
        if self.origin.is_empty() {
            return tasks;
        }
        let (mut back, rest): (Vec<_>, Vec<_>) = tasks
            .into_iter()
            .partition(|task| self.origin.contains_key(&place_of(task)));
        back.sort_by_key(|task| self.origin[&place_of(task)]);
        let mut back = back.into_iter().peekable();
        let mut out = Vec::new();
        for task in rest {
            let past = |back: &&Task| {
                task.source() == Source::Done
                    || self.origin[&place_of(back)] <= task.nr()
            };
            while let Some(early) = back.next_if(past) {
                out.push(early);
            }
            out.push(task);
        }
        out.extend(back);
        out
    }
}

impl Model {
    /// Marks the tasks at `touched` that the view does not match, and
    /// unmarks those it does. They are places of the current reading. An
    /// task that went from the todo file to the done file notes where it
    /// stood, so that it is drawn there while sticky.
    pub(crate) fn stick(&mut self, moved: &Remap, touched: &[Touched]) {
        let held = {
            let scope = self.scope();
            let hold = |at: &Place| {
                let task = self.snap.at(at.0, at.1)?;
                Some(!scope.holds(&self.snap, task))
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

    /// Puts the cursor on the touched task it stood on (`from`) when that
    /// is sticky: a write by keys leaves a task it archived in view, and
    /// the cursor stays with it.
    pub(crate) fn stick_cursor(
        &mut self,
        touched: &[Touched],
        from: Option<Place>,
    ) {
        if let Some(own) = touched.iter().find(|t| t.was == from)
            && self.sticky.has(own.now)
            && let Some(i) = self.row_at(own.now)
        {
            self.cursor = i;
            self.scroll();
        }
    }

    /// Adds the sticky notice to the message when any touched task is
    /// shown as sticky.
    pub(crate) fn keep_sticky(&mut self, touched: &[Touched]) {
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

    /// The place the cursor row is drawn at: for an archived task that is
    /// sticky, where it stood in the todo file.
    pub(crate) fn drawn_cursor(&self) -> Option<Place> {
        self.cursor_place().map(|at| {
            self.sticky
                .origin
                .get(&at)
                .map_or(at, |&nr| (Source::Todo, nr))
        })
    }

    /// Lists the rows again without the tasks that an earlier input kept
    /// in sight, the cursor on the first row at or after where its row
    /// was drawn and the view as it was.
    /// A key keeps the row under the cursor for itself, so that a move
    /// counts over the rows that stay and may step off it.
    pub(crate) fn leave_sticky(&mut self, keep: Option<Place>) {
        let place = self.drawn_cursor();
        let view = self.view_offset();
        if self.sticky.drop_aged(keep) {
            self.rebuild_at(place, view);
        }
    }
}
