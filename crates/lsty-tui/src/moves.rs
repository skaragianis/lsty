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

    /// Whether `J` or `K` (`action`) moves the rows of `move_span` only past
    /// lines equal to them and next to them in the file, so no task changes
    /// place.
    pub(crate) fn moves_nothing(
        &self,
        action: Action,
        block: bool,
        count: usize,
    ) -> bool {
        let down = action == Action::MoveTaskDown;
        if !down && action != Action::MoveTaskUp {
            return false;
        }
        let Some((lo, hi, Some(past))) = self.move_span(down, block, count)
        else {
            return false;
        };
        let rows = self.rows.get(lo.min(past)..=hi.max(past)).unwrap_or(&[]);
        // The line number plus the rows after it, one for lines in a run.
        let mut lines =
            rows.iter().zip((0..rows.len()).rev()).map(|(r, i)| {
                let task = self.snap.task(r.id)?;
                Some((self.snap.line(task).raw(), task.nr().index() + i))
            });
        let first = lines.next().flatten();
        first.is_some() && lines.all(|line| line == first)
    }

    /// `move_task_down` and `move_task_up`: moves the rows of `move_span`
    /// past its row in one write, unless the rule refuses or no task would
    /// change place; it then sets what `.` repeats. With a count the task
    /// passes that many shown tasks, or as many as there are. In VISUAL
    /// mode the selection moves as a block and the count is dropped; with
    /// `block`, a `.` after a block move, `count` rows from the cursor move
    /// as one block again.
    pub(crate) fn move_by(
        &mut self,
        action: Action,
        block: bool,
        count: usize,
    ) {
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
        if self.moves_nothing(action, block, count) {
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
        // Only a list behind the files aims a move that keeps every line.
        if matches!(&result, Ok((outcome, _)) if outcome.no_op()) {
            self.say_changed();
        } else if self.settle(result) {
            self.again = Some(again);
        }
    }
}
