//! `pri` and `depri`: give an open item a priority or take it away.

use std::collections::HashMap;

use super::{
    claim::Guard,
    env::Env,
    refs::Target,
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Tx, gone, wrong_state},
};
use crate::{
    errs::{Code, Error, Field, Result},
    list::{Item, ItemId, State},
    todotxt::{Line, Priority},
};

/// A target and the priority to give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriPair {
    /// The item.
    pub target: Target,
    /// The priority to give; `None` removes it, so one call can set some
    /// items and clear others.
    pub priority: Option<Priority>,
}

/// Sets the priority of each target, or removes it where the pair has
/// none.
pub fn pri(env: &Env<'_>, pairs: &[PriPair]) -> Result<Outcome> {
    run(env, "pri", false, |tx| {
        let mut given: HashMap<ItemId, Option<Priority>> = HashMap::new();
        for pair in pairs {
            let Some(id) = tx.pick(&pair.target)? else {
                continue;
            };
            match given.insert(id, pair.priority) {
                Some(prev) if prev != pair.priority => {
                    return Err(conflicting(&tx.handle(id)));
                }
                Some(_) => {}
                None => edit_priority(tx, env, id, pair.priority)?,
            }
        }
        Ok(())
    })
}

/// Removes the priority of each target.
pub fn depri(env: &Env<'_>, targets: &[Target]) -> Result<Outcome> {
    run(env, "depri", false, |tx| {
        tx.each_edit(targets, &env.actor, |tx, id| put_priority(tx, id, None))
    })
}

fn edit_priority(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    priority: Option<Priority>,
) -> Result<()> {
    tx.mark_primary(id)?;
    tx.guarded(Guard::edit(&env.actor), id, |tx| {
        put_priority(tx, id, priority)
    })
}

/// The priority a letter given on the command line names, in either case.
///
/// # Errors
///
/// `invalid_value` naming the field `priority` for anything but one
/// letter from `A` to `Z`.
pub fn parse_letter(text: &str) -> Result<Priority> {
    Priority::parse(text).ok_or_else(|| {
        let message = format!("priority {text} is not a letter from A to Z");
        Error::invalid_value(message, text, &["A-Z"])
            .with_field("field", Field::Str("priority".to_owned()))
    })
}

fn conflicting(handle: &str) -> Error {
    Error::new(
        Code::Usage,
        format!("{handle} is given two different priorities"),
    )
}

/// The item's line as read; a done item is refused.
fn open_line(tx: &Tx, id: ItemId) -> Result<&Line> {
    let item = tx.item(id).ok_or_else(gone)?;
    if item.state() == State::Done {
        return Err(reopen_first(tx, id, item));
    }
    tx.line_of(id)
}

/// The refusal of a change to the priority of a done item.
pub(super) fn reopen_first(tx: &Tx, id: ItemId, item: &Item) -> Error {
    let handle = tx.handle(id);
    wrong_state(
        tx.snapshot(),
        item,
        item.state(),
        format!("{handle} is done; reopen it first"),
        format!("use lsty reopen {handle}"),
    )
}

/// Gives the open item the priority `want`, or none.
fn put_priority(
    tx: &mut Tx,
    id: ItemId,
    want: Option<Priority>,
) -> Result<()> {
    let line = open_line(tx, id)?;
    let (cur, raw) = (line.priority(), line.shared());
    let handle = tx.handle(id);
    let (text, said) = match (cur, want) {
        (None, None) => (None, format!("{handle} has no priority")),
        (Some(cur), Some(want)) if cur == want => {
            (None, format!("{handle} already has priority ({want})"))
        }
        (None, Some(want)) => (
            Some([format!("({want}) ").as_bytes(), &raw].concat()),
            format!("{handle} prioritized ({want})"),
        ),
        (Some(cur), Some(want)) => (
            Some(
                [
                    format!("({want})").as_bytes(),
                    raw.get(3..).unwrap_or_default(),
                ]
                .concat(),
            ),
            format!("{handle} re-prioritized from ({cur}) to ({want})"),
        ),
        (Some(_), None) => (
            Some(raw.get(4..).unwrap_or_default().to_vec()),
            format!("{handle} deprioritized"),
        ),
    };
    if let Some(text) = text {
        tx.set(id, text, Path::Action);
    }
    tx.say(said);
    Ok(())
}
