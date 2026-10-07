//! `archive`: move the done lines of the todo file to the done file and
//! remove the blank lines of the todo file.

use super::env::Env;
use super::result::Outcome;
use super::run::run;
use super::tx::Tx;
use crate::errs::{Result, Source};
use crate::list::State;
use crate::todotxt::Line;

/// Moves every done line of the todo file to the done file, in file
/// order, and removes the blank lines of the todo file; the done file only
/// gains lines, so its `done:N` numbers hold. A line whose `id:`
/// is already on a line of the done file goes over that line.
///
/// # Errors
///
/// Whatever reading, locking or writing the files fails with.
pub fn archive(env: &Env<'_>) -> Result<Outcome> {
    run(env, "archive", true, |tx| {
        tx.dest(Source::Done);
        let (moved, finished) = tx.move_done_lines();
        let blanks = tx.blank_lines();
        tx.compact(Source::Todo);
        if moved > 0 {
            let name = tx.done_name();
            tx.say(format!("archived {moved} to {name}"));
        } else if blanks == 0 {
            tx.say("nothing to archive");
        }
        if blanks > 0 {
            tx.say(format!("removed {blanks} blank lines"));
        }
        for id in finished {
            tx.say_interrupted(&id);
        }
        Ok(())
    })
}

impl Tx {
    /// Sends each done line of the todo file to the done file. Gives how
    /// many it moved and the ids of the ones that went over a copy.
    fn move_done_lines(&mut self) -> (usize, Vec<Vec<u8>>) {
        let ids: Vec<_> = self
            .snap
            .in_source(Source::Todo)
            .iter()
            .filter(|item| item.state() == State::Done)
            .map(crate::list::Item::item_id)
            .collect();
        let mut finished = Vec::new();
        for &id in &ids {
            let Some(raw) = self.line(id).map(|l| l.raw().to_vec()) else {
                continue;
            };
            if let Some((_, true)) = self.send_to_done(id, &raw) {
                finished.extend(self.id_bytes(id));
            }
        }
        (ids.len(), finished)
    }

    /// How many lines of the todo file were blank when read.
    fn blank_lines(&self) -> usize {
        self.todo
            .all
            .iter()
            .filter(|row| !row.added && Line::is_blank_text(&row.orig))
            .count()
    }
}
