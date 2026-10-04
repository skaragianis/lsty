//! `J` and `K`: move the item under the cursor past the next shown
//! items, in one write.

use lsty_core::errs::{self, Source};
use lsty_core::keys::Action;
use lsty_core::ops::{self, Placement};

use crate::actions::Chosen;
use crate::history::Again;
use crate::messages;
use crate::model::Model;

impl Model {
    /// `move_item_down` and `move_item_up`. With a count the item
    /// passes that many shown items, or as many as there are.
    pub(crate) fn move_item(&mut self, action: Action, count: usize) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if self.sorted() {
            self.message = Some(messages::sorted(self.sort_name()));
            return;
        }
        let id = row.id;
        let down = action == Action::MoveItemDown;
        if row.source == Source::Done {
            let text =
                self.chosen(&[id]).first().map(|c| errs::lossy(&c.text));
            self.alert(messages::archived(text.as_deref()));
            return;
        }
        let Some(anchor) = self.neighbour(count.max(1), down) else {
            return;
        };
        let (Some(item), Some(past)) = (
            self.chosen(&[id]).first().map(Chosen::target),
            self.chosen(&[anchor]).first().map(Chosen::target),
        ) else {
            return;
        };
        let place = if down {
            Placement::After(past)
        } else {
            Placement::Before(past)
        };
        let result = self.attempt(|env| ops::reorder(env, &[item], &place));
        if self.settle(result) {
            self.again = Some(Again::Item { action, count });
        }
    }

    /// The shown item `n` rows from the cursor, held within the todo
    /// rows; `None` when the cursor cannot move that way.
    fn neighbour(
        &self,
        n: usize,
        down: bool,
    ) -> Option<lsty_core::list::ItemId> {
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
