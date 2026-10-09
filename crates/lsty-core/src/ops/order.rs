//! `up`, `down` and `reorder`: move items as one block, and `deduplicate`.
//! The lines are lifted out of the file and put back at the new place;
//! blank lines are neither moved nor created.

use std::collections::HashSet;

use super::claim::ClaimMode;
use super::env::Env;
use super::refs::{Target, no_item};
use super::result::Outcome;
use super::run::run;
use super::tx::Tx;
use crate::errs::{Code, Error, Field, Nr, Result, Source};
use crate::list::ItemId;

/// Where [`reorder`] puts the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// So that the first line of the block is this line of the file
    /// (1-based); past the end means the end.
    Position(i64),
    /// Directly below the item.
    After(Target),
    /// Directly above the item.
    Before(Target),
}

/// The items to move, in file order, and what is left of the file.
struct Block {
    items: Vec<ItemId>,
    /// Line indexes of the items.
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

/// Moves the targets above the `n`-th item over them.
///
/// # Errors
///
/// `invalid_value` for an `n` below 1; `no_item` or `wrong_state` for a
/// target that is not an item of the todo file.
pub fn up(env: &Env<'_>, targets: &[Target], n: i64) -> Result<Outcome> {
    shift(env, Way::Up, targets, n)
}

/// Moves the targets below the `n`-th item under them.
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
        if block.items.is_empty() {
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
/// anchor is one of the targets; `no_item` or `wrong_state` for a target
/// or an anchor that is not an item of the todo file.
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
        if block.items.is_empty() {
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
    fn is_item_at(&self, index: usize) -> bool {
        Nr::from_index(index)
            .and_then(|nr| self.snap.at(Source::Todo, nr))
            .is_some()
    }

    /// The items the targets name, in file order, and the other lines.
    fn lift(&mut self, targets: &[Target]) -> Result<Block> {
        let mut seen = HashSet::new();
        let mut picked = Vec::new();
        for target in targets {
            if let Some(id) = self.pick(target)?
                && seen.insert(id)
                && let Some(item) = self.item(id)
            {
                picked.push((item.nr().index(), id));
            }
        }
        picked.sort_by_key(|&(index, _)| index);
        let lines: Vec<usize> = picked.iter().map(|p| p.0).collect();
        let items: Vec<ItemId> = picked.iter().map(|p| p.1).collect();
        for &id in &items {
            self.mark_primary(id);
        }
        let all = self.snap.file(Source::Todo).lines.len();
        let rest = (0..all)
            .filter(|i| lines.binary_search(i).is_err())
            .collect();
        Ok(Block { items, lines, rest })
    }

    /// The indexes into `rest` of the items that `keep` accepts.
    fn candidates(
        &self,
        block: &Block,
        keep: impl Fn(usize) -> bool,
    ) -> Vec<usize> {
        let mut out = Vec::new();
        for (at, &line) in block.rest.iter().enumerate() {
            if keep(line) && self.is_item_at(line) {
                out.push(at);
            }
        }
        out
    }

    /// Where in `rest` the block goes, or `None` when there is nothing
    /// to pass.
    fn find(&self, block: &Block, way: Way, n: usize) -> Option<usize> {
        match way {
            Way::Down => {
                let last = block.lines.last().copied()?;
                let seen = self.candidates(block, |l| l > last);
                let at = *seen.iter().take(n).next_back()?;
                Some(at + 1)
            }
            Way::Up => {
                let first = block.lines.first().copied()?;
                let seen = self.candidates(block, |l| l < first);
                seen.get(seen.len().saturating_sub(n)).copied()
            }
        }
    }

    fn say_each(&mut self, block: &Block, what: &str) {
        for &id in &block.items {
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
        let anchor_target = anchor;
        let id = self.anchor(anchor_target)?;
        if block.items.contains(&id) {
            let handle = self.handle(id);
            return Err(Error::new(
                Code::Usage,
                format!("{handle} is both a target and the anchor"),
            ));
        }
        let item = self.item(id).ok_or_else(|| no_item(anchor_target))?;
        let line = item.nr().index();
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
        for (i, &id) in block.items.iter().enumerate() {
            let pos = at + i;
            let prev = pos.checked_sub(1).and_then(|p| seq.get(p));
            self.relocate(id, prev.map_or(0, |p| p + 1));
            let new_nr = pos + 1;
            let handle = self.handle(id);
            self.say(format!("{handle} moved to {new_nr}"));
            self.note_claim(&env.actor, id, ClaimMode::Edit);
        }
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
            tx.mark_primary(id);
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

impl Tx {
    /// The items whose line is equal to an earlier one.
    fn repeats(&self) -> Vec<ItemId> {
        let mut seen: HashSet<&[u8]> = HashSet::new();
        let mut out = Vec::new();
        for (index, line) in
            self.snap.file(Source::Todo).lines.iter().enumerate()
        {
            let item = Nr::from_index(index)
                .and_then(|nr| self.snap.at(Source::Todo, nr));
            let Some(item) = item else { continue };
            if !seen.insert(line.raw()) {
                out.push(item.item_id());
            }
        }
        out
    }
}
