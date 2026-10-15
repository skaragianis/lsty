//! `up`, `down` and `reorder`: move tasks as one block, and `deduplicate`.
//! The lines are lifted out of the file and put back at the new place;
//! blank lines are neither moved nor created.

use std::collections::HashSet;

use super::{
    claim::ClaimMode,
    env::Env,
    refs::Target,
    result::Outcome,
    run::run,
    tx::{Tx, gone},
};
use crate::{
    errs::{Code, Error, Field, Nr, Result, Source},
    list::{Task, TaskId},
};

/// Where [`reorder`] puts the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// So that the first line of the block is this line of the file
    /// (1-based); past the end means the end.
    Position(i64),
    /// Directly below the task.
    After(Target),
    /// Directly above the task.
    Before(Target),
}

/// The tasks to move, in file order, and what is left of the file.
struct Block {
    tasks: Vec<TaskId>,
    /// Line indexes of the tasks.
    lines: Vec<usize>,
    /// Line indexes of every other line, in order.
    rest: Vec<usize>,
}

/// Which way `up` and `down` go.
#[derive(Clone, Copy)]
enum Way {
    Down,
    Up,
}

impl Way {
    const fn action(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
        }
    }

    const fn edge(self) -> &'static str {
        match self {
            Self::Down => "at the end",
            Self::Up => "at the top",
        }
    }
}

/// Moves the targets above the `n`-th task over them.
///
/// # Errors
///
/// `invalid_value` for an `n` below 1; `no_task` or `wrong_state` for a
/// target that is not a task of the todo file.
pub fn up(env: &Env<'_>, targets: &[Target], n: i64) -> Result<Outcome> {
    shift(env, Way::Up, targets, n)
}

/// Moves the targets below the `n`-th task under them.
///
/// # Errors
///
/// As [`up`].
pub fn down(env: &Env<'_>, targets: &[Target], n: i64) -> Result<Outcome> {
    shift(env, Way::Down, targets, n)
}

fn shift(
    env: &Env<'_>,
    way: Way,
    targets: &[Target],
    n: i64,
) -> Result<Outcome> {
    if n < 1 {
        return Err(invalid_place("count", n));
    }
    let n = usize::try_from(n).unwrap_or(usize::MAX);
    run(env, way.action(), false, |tx| {
        let block = tx.lift(targets)?;
        if block.tasks.is_empty() {
            return Ok(());
        }
        match tx.find(&block, way, n) {
            Some(at) => tx.regroup(env, &block, at),
            None => tx.say_each(&block, &format!("is already {}", way.edge())),
        }
        Ok(())
    })
}

/// Moves the targets, as one block, to the placement.
///
/// # Errors
///
/// `invalid_value` for a position below 1; a usage error when the
/// anchor is one of the targets; `no_task` or `wrong_state` for a target
/// or an anchor that is not a task of the todo file.
pub fn reorder(
    env: &Env<'_>,
    targets: &[Target],
    place: &Placement,
) -> Result<Outcome> {
    if let Placement::Position(pos) = place
        && *pos < 1
    {
        return Err(invalid_place("position", *pos));
    }
    run(env, "reorder", false, |tx| {
        let block = tx.lift(targets)?;
        if block.tasks.is_empty() {
            return Ok(());
        }
        let at = tx.place_at(&block, place)?;
        tx.regroup(env, &block, at);
        Ok(())
    })
}

fn invalid_place(field: &str, value: i64) -> Error {
    Error::new(Code::InvalidValue, format!("{field} must be 1 or more"))
        .with_field("field", Field::Str(field.to_owned()))
        .with_field("value", Field::Int(value))
}

impl Tx {
    fn is_task_at(&self, index: usize) -> bool {
        Nr::from_index(index)
            .and_then(|nr| self.snap.at(Source::Todo, nr))
            .is_some()
    }

    /// The tasks the targets name, in file order, and the other lines.
    fn lift(&mut self, targets: &[Target]) -> Result<Block> {
        let mut picked = Vec::new();
        for target in targets {
            if let Some(id) = self.pick(target)? {
                picked.push((self.line_ref(id)?.index, id));
            }
        }
        picked.sort_unstable_by_key(|p| p.0);
        picked.dedup_by_key(|p| p.0);
        let (lines, tasks): (Vec<usize>, Vec<TaskId>) =
            picked.into_iter().unzip();
        for &id in &tasks {
            self.mark_primary(id)?;
        }
        let all = self.snap.file(Source::Todo).lines.len();
        let rest = (0..all)
            .filter(|i| lines.binary_search(i).is_err())
            .collect();
        Ok(Block { tasks, lines, rest })
    }

    /// Where in `rest` the block goes, or `None` when there is nothing
    /// to pass.
    fn find(&self, block: &Block, way: Way, n: usize) -> Option<usize> {
        let tasks = block
            .rest
            .iter()
            .enumerate()
            .filter(|&(_, &l)| self.is_task_at(l));
        match way {
            Way::Down => {
                let last = block.lines.last().copied()?;
                let (at, _) =
                    tasks.filter(|&(_, &l)| l > last).take(n).last()?;
                Some(at + 1)
            }
            Way::Up => {
                let first = block.lines.first().copied()?;
                let (at, _) =
                    tasks.filter(|&(_, &l)| l < first).rev().take(n).last()?;
                Some(at)
            }
        }
    }

    fn say_each(&mut self, block: &Block, what: &str) {
        for &id in &block.tasks {
            let handle = self.handle(id);
            self.say(format!("{handle} {what}"));
        }
    }

    /// Where in `rest` a placement puts the block.
    fn place_at(&mut self, block: &Block, place: &Placement) -> Result<usize> {
        let (anchor, shift) = match place {
            Placement::Position(pos) => {
                let pos = usize::try_from(*pos).unwrap_or(usize::MAX);
                return Ok(pos.saturating_sub(1));
            }
            Placement::After(anchor) => (anchor, 1),
            Placement::Before(anchor) => (anchor, 0),
        };
        let id = self.anchor(anchor)?;
        if block.tasks.contains(&id) {
            let handle = self.handle(id);
            return Err(Error::new(
                Code::Usage,
                format!("{handle} is both a target and the anchor"),
            ));
        }
        let task = self.task(id).ok_or_else(gone)?;
        let line = task.nr().index();
        // The anchor is not in the block, so its place among the other
        // lines is its index less the block lines above it.
        let above = block.lines.partition_point(|&l| l < line);
        Ok(line - above + shift)
    }

    /// Puts the block at `at` in `rest`.
    fn regroup(&mut self, env: &Env<'_>, block: &Block, at: usize) {
        let at = at.min(block.rest.len());
        let (head, tail) = block.rest.split_at(at);
        let seq: Vec<usize> = [head, &block.lines, tail].concat();
        if seq.is_sorted() {
            self.say_each(block, "is already there");
            return;
        }
        let prev = head.last();
        self.relocate(&block.tasks, prev.map_or(0, |p| p + 1));
        for (i, &id) in block.tasks.iter().enumerate() {
            let new_nr = at + i + 1;
            let handle = self.handle(id);
            self.say(format!("{handle} moved to {new_nr}"));
            self.note_claim(&env.actor, id, ClaimMode::Edit);
        }
    }

    /// The tasks whose line is equal to an earlier one.
    fn repeats(&self) -> Vec<TaskId> {
        let mut seen = HashSet::new();
        self.snap
            .in_source(Source::Todo)
            .iter()
            .filter(|task| !seen.insert(self.snap.line(task).raw()))
            .map(Task::task_id)
            .collect()
    }
}

/// Blanks every later copy of a line of the todo file.
///
/// # Errors
///
/// Only those of reading and writing the file.
pub fn deduplicate(env: &Env<'_>) -> Result<Outcome> {
    run(env, "deduplicate", false, |tx| {
        let repeats = tx.repeats();
        for &id in &repeats {
            tx.mark_primary(id)?;
            tx.blank(id);
        }
        if repeats.is_empty() {
            tx.say("no duplicates");
        } else {
            tx.say(format!("removed {} duplicates", repeats.len()));
        }
        Ok(())
    })
}
