//! `J` and `K`: move the item under the cursor, or the selected block,
//! past the next shown item, in one write.

use lsty_core::errs::Source;
use lsty_core::keys::Action;
use lsty_core::list::ItemId;
use lsty_core::ops::{self, Placement, Target};

use crate::actions::{Chosen, archived_refusal};
use crate::history::Again;
use crate::messages;
use crate::model::Model;

impl Model {
    /// `move_item_down` and `move_item_up`. With a count the item
    /// passes that many shown items, or as many as there are. In VISUAL
    /// mode the selection moves as a block and the count is dropped.
    pub(crate) fn move_item(&mut self, action: Action, count: usize) {
        if self.visual.is_some() {
            let (lo, hi) = self.selection();
            self.move_rows(action, lo, hi);
            return;
        }
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        let ids = [row.id];
        let down = action == Action::MoveItemDown;
        let again = Again::Item { action, count };
        let past = |m: &Self| m.neighbour(count.max(1), down);
        self.reorder_past(&ids, down, past, again);
    }

    /// Moves the rows `lo` to `hi` past the next shown row, and tells
    /// `.` to repeat the move over as many rows.
    fn move_rows(&mut self, action: Action, lo: usize, hi: usize) {
        let down = action == Action::MoveItemDown;
        let next = if down {
            Some(hi + 1)
        } else {
            lo.checked_sub(1)
        };
        let ids = self.row_items(lo, hi);
        let again = Again::Block {
            action,
            count: ids.len(),
        };
        let past = |m: &Self| {
            let row = m.rows.get(next?)?;
            (row.source != Source::Done).then_some(row.id)
        };
        self.reorder_past(&ids, down, past, again);
    }

    /// `.` after a block move: `count` rows from the cursor move as one
    /// block again.
    pub(crate) fn repeat_block(&mut self, action: Action, count: usize) {
        let ids = self.picked(count);
        if ids.is_empty() {
            return;
        }
        self.move_rows(action, self.cursor, self.cursor + ids.len() - 1);
    }

    /// Moves `ids` past the item that `past` finds, in one write, unless
    /// the view is sorted, an item is archived or there is none; it
    /// then sets what `.` repeats.
    fn reorder_past(
        &mut self,
        ids: &[ItemId],
        down: bool,
        past: impl FnOnce(&Self) -> Option<ItemId>,
        again: Again,
    ) {
        if self.sorted() {
            self.message = Some(messages::sorted(self.sort_name()));
            return;
        }
        let items = self.chosen(ids);
        if let Some(text) = archived_refusal(&items) {
            self.alert(text);
            return;
        }
        let Some(past) = past(self) else {
            return;
        };
        let Some(anchor) = self.chosen(&[past]).first().map(Chosen::target)
        else {
            return;
        };
        let targets: Vec<Target> = items.iter().map(Chosen::target).collect();
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

    /// The shown item `n` rows from the cursor, held within the todo
    /// rows; `None` when the cursor cannot move that way.
    fn neighbour(&self, n: usize, down: bool) -> Option<ItemId> {
        let todo = self.rows[self.cursor..]
            .iter()
            .take_while(|r| r.source != Source::Done)
            .count();
        let to = if down {
            self.cursor + n.min(todo.saturating_sub(1))
        } else {
            self.cursor.saturating_sub(n)
        };
        (to != self.cursor).then(|| self.rows.get(to).map(|r| r.id))?
    }
}
