//! `archive`: move the done lines of the todo file to the done file and
//! remove the blank lines of the todo file.

use std::sync::Arc;

use super::{env::Env, result::Outcome, run::run, tx::Tx};
use crate::{
    errs::{Result, Source},
    list::State,
    todotxt::Line,
};

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
        tx.dest = Some(Source::Done);
        let (moved, finished) = tx.move_done_lines()?;
        let blanks = tx.blank_lines();
        tx.todo.compact();
        if moved > 0 {
            let name = tx.file_name(Source::Done);
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

/// How many lines moved, and the ids of those that went over a copy.
type Moved = (usize, Vec<Vec<u8>>);

impl Tx {
    /// Sends each done line of the todo file to the done file. Gives how
    /// many it moved and the ids of the ones that went over a copy.
    pub(super) fn move_done_lines(&mut self) -> Result<Moved> {
        let done: Vec<_> = self
            .snap
            .in_source(Source::Todo)
            .iter()
            .filter(|item| item.state() == State::Done)
            .map(|item| (item.item_id(), self.snap.line(item).shared()))
            .collect();
        let mut finished = Vec::new();
        for &(id, ref raw) in &done {
            let (line, over) =
                self.stage_in(Source::Done, id, Arc::clone(raw))?;
            self.primary.push(line);
            if over {
                finished.extend(self.id_bytes(id));
            }
        }
        Ok((done.len(), finished))
    }

    /// How many lines of the todo file were blank when read.
    fn blank_lines(&self) -> usize {
        self.todo
            .rows()
            .iter()
            .filter(|row| Line::is_blank_text(&row.orig))
            .count()
    }
}
