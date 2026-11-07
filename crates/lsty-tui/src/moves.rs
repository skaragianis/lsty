//! `J` and `K`: move the task under the cursor, or the selected block,
//! past the next shown task, in one write.

use lsty_core::{
    errs::Source,
    keys::Action,
    ops::{self, Placement, Target},
};

use crate::{
    actions::Chosen,
    counts::ids_of,
    history::Again,
    model::Model,
    press::{Answer, Summary, decide},
};

/// The rows `lo` to `hi` a move takes and the row it goes past, if any.
pub(crate) type Span = (usize, usize, Option<usize>);

impl Model {
    /// `move_task_down` and `move_task_up`. With a count the task
    /// passes that many shown tasks, or as many as there are. In VISUAL
    /// mode the selection moves as a block and the count is dropped.
    pub(crate) fn move_task(&mut self, action: Action, count: usize) {
        self.move_by(action, false, count);
    }

    /// `.` after a block move: `count` rows from the cursor move as one
    /// block again.
    pub(crate) fn repeat_block(&mut self, action: Action, count: usize) {
        self.move_by(action, true, count);
    }

    /// What a move takes and goes past; `None` when it takes no rows. The
    /// selection, or with `block` as many rows as `count` picks from the
    /// cursor, goes past the next row; else the cursor's task goes past
    /// `count` shown tasks, held within the todo rows.
    pub(crate) fn move_span(
        &self,
        down: bool,
        block: bool,
        count: usize,
    ) -> Option<Span> {
        let n = self.picked_rows(count).len();
        let span = if block {
            Some((n > 0).then(|| (self.cursor, self.cursor + n - 1))?)
        } else {
            self.selected_rows()
        };
        if let Some((lo, hi)) = span {
            let past = down.then_some(hi + 1).or_else(|| lo.checked_sub(1));
            return Some((lo, hi, past));
        }
        let todo = self.rows[self.cursor..]
            .iter()
            .take_while(|r| r.source != Source::Done)
            .count();
        let to = if down {
            self.cursor + count.max(1).min(todo.saturating_sub(1))
        } else {
            self.cursor.saturating_sub(count.max(1))
        };
        Some((self.cursor, self.cursor, (to != self.cursor).then_some(to)))
    }

    /// Moves the rows of `move_span` past its row in one write, unless the
    /// rule refuses; it then sets what `.` repeats.
    fn move_by(&mut self, action: Action, block: bool, count: usize) {
        let down = action == Action::MoveTaskDown;
        let Some((lo, hi, past)) = self.move_span(down, block, count) else {
            return;
        };
        let again = if block || self.selected_rows().is_some() {
            Again::Block {
                action,
                count: hi - lo + 1,
            }
        } else {
            Again::Task { action, count }
        };
        let rows = self.rows_between(lo, hi);
        let s = Summary::of(&self.snap, lo, rows);
        let answer = decide(again.action(), self, &s);
        let tasks = self.chosen(&ids_of(rows));
        if let Answer::Refused(why) = answer {
            self.refuse(why, &tasks);
            return;
        }
        let Some(past) = past.and_then(|i| self.rows.get(i)).map(|r| r.id)
        else {
            return;
        };
        let Some(anchor) = self.chosen(&[past]).first().map(Chosen::target)
        else {
            return;
        };
        let targets: Vec<Target> = tasks.iter().map(Chosen::target).collect();
        let place = if down {
            Placement::After(anchor)
        } else {
            Placement::Before(anchor)
        };
        let result = self.attempt(|env| ops::reorder(env, &targets, &place));
        if self.settle(result) {
            self.again = Some(again);
        }
    }
}
