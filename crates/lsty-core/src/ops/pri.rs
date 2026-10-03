//! `pri` and `depri`: give an open item a priority or take it away.

use std::collections::HashMap;

use super::env::Env;
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::{Tx, wrong_state};
use crate::errs::{Code, Error, Field, Result};
use crate::list::{ItemId, State};
use crate::todotxt::Priority;

/// A target and the letter to give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriPair {
    /// The item.
    pub target: Target,
    /// `A` to `Z`, in either case. For [`reprioritize`] an empty letter
    /// removes the priority.
    pub letter: String,
}

/// Sets the priority of each target.
pub fn pri(env: &Env<'_>, pairs: &[PriPair]) -> Result<Outcome> {
    let letters = parse_letters(pairs, false)?;
    prioritize(env, pairs, &letters)
}

/// [`pri`], where an empty letter removes the priority, so one call can
/// set some items and clear others.
pub fn reprioritize(env: &Env<'_>, pairs: &[PriPair]) -> Result<Outcome> {
    let letters = parse_letters(pairs, true)?;
    prioritize(env, pairs, &letters)
}

/// Removes the priority of each target.
pub fn depri(env: &Env<'_>, targets: &[Target]) -> Result<Outcome> {
    run(env, "depri", false, |tx| {
        tx.each_item(targets, remove_priority)
    })
}

fn prioritize(
    env: &Env<'_>,
    pairs: &[PriPair],
    letters: &[Option<Priority>],
) -> Result<Outcome> {
    run(env, "pri", false, |tx| {
        let mut given: HashMap<ItemId, Option<Priority>> = HashMap::new();
        for (pair, &letter) in pairs.iter().zip(letters) {
            let Some(id) = tx.pick(&pair.target)? else {
                continue;
            };
            match given.insert(id, letter) {
                Some(prev) if prev != letter => {
                    return Err(conflicting(&tx.handle(id)));
                }
                Some(_) => {}
                None => edit_priority(tx, id, letter)?,
            }
        }
        Ok(())
    })
}

fn edit_priority(
    tx: &mut Tx,
    id: ItemId,
    letter: Option<Priority>,
) -> Result<()> {
    tx.mark_primary(id);
    match letter {
        Some(letter) => set_priority(tx, id, letter),
        None => remove_priority(tx, id),
    }
}

fn parse_letters(
    pairs: &[PriPair],
    empty_removes: bool,
) -> Result<Vec<Option<Priority>>> {
    pairs
        .iter()
        .map(|p| {
            if empty_removes && p.letter.is_empty() {
                return Ok(None);
            }
            parse_letter(&p.letter).map(Some)
        })
        .collect()
}

pub(super) fn parse_letter(text: &str) -> Result<Priority> {
    let upper = text.to_uppercase();
    match upper.as_bytes() {
        [letter] => Priority::new(*letter),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(
            Code::InvalidValue,
            format!("priority {text} is not a letter from A to Z"),
        )
        .with_field("field", Field::Str("priority".to_owned()))
        .with_field("value", Field::Str(text.to_owned()))
        .with_field("accepted", Field::Strs(vec!["A-Z".to_owned()]))
    })
}

fn conflicting(handle: &str) -> Error {
    Error::new(
        Code::Usage,
        format!("{handle} is given two different priorities"),
    )
}

/// The item's old priority and whether it is done; a done item is
/// refused.
fn open_priority(tx: &Tx, id: ItemId) -> Result<Option<Priority>> {
    let (Some(item), Some(line)) = (tx.item(id), tx.line(id)) else {
        return Ok(None);
    };
    if item.state() != State::Done {
        return Ok(line.effective_priority());
    }
    let handle = tx.handle(id);
    Err(wrong_state(
        tx.snapshot(),
        item,
        item.state(),
        format!("{handle} is done; reopen it first"),
        format!("use lsty reopen {handle}"),
    ))
}

fn set_priority(tx: &mut Tx, id: ItemId, letter: Priority) -> Result<()> {
    let cur = open_priority(tx, id)?;
    let handle = tx.handle(id);
    let raw = tx.line(id).map(|l| l.raw().to_vec()).unwrap_or_default();
    let (text, said) = match cur {
        Some(cur) if cur == letter => {
            tx.say(format!("{handle} already has priority ({letter})"));
            return Ok(());
        }
        None => (
            [format!("({letter}) ").as_bytes(), &raw].concat(),
            format!("{handle} prioritized ({letter})"),
        ),
        Some(cur) => (
            [
                format!("({letter})").as_bytes(),
                raw.get(3..).unwrap_or_default(),
            ]
            .concat(),
            format!("{handle} re-prioritized from ({cur}) to ({letter})"),
        ),
    };
    tx.set(id, text, Path::Action);
    tx.say(said);
    Ok(())
}

fn remove_priority(tx: &mut Tx, id: ItemId) -> Result<()> {
    let cur = open_priority(tx, id)?;
    let handle = tx.handle(id);
    let raw = tx.line(id).map(|l| l.raw().to_vec()).unwrap_or_default();
    if cur.is_none() {
        tx.say(format!("{handle} has no priority"));
        return Ok(());
    }
    tx.set(id, raw.get(4..).unwrap_or_default(), Path::Action);
    tx.say(format!("{handle} deprioritized"));
    Ok(())
}
