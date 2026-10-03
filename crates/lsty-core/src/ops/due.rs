//! `due`: set or clear the due date.

use super::env::Env;
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::tag::{Applied, Tag, apply_tag};
use super::tokens::first_of_key;
use super::tx::Tx;
use crate::dates::{self, Resolved};
use crate::errs::{Field, Result};
use crate::list::ItemId;
use crate::todotxt::Kind;

/// Sets the due date of each target; the empty input clears it.
///
/// # Errors
///
/// `invalid_value` for an input that is not a date, and whatever the
/// rules refuse.
pub fn due(env: &Env<'_>, targets: &[Target], input: &str) -> Result<Outcome> {
    let resolved = dates::resolve(input, env.clock.today())
        .map_err(|e| e.with_field("field", Field::Str("due".to_owned())))?;
    run(env, "due", false, |tx| {
        tx.each_item(targets, |tx, id| match resolved {
            Resolved::Clear => clear_due(tx, env, id),
            Resolved::On(date) => set_due(tx, env, id, &date.to_string()),
        })
    })
}

fn set_due(tx: &mut Tx, env: &Env<'_>, id: ItemId, date: &str) -> Result<()> {
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    let handle = tx.handle(id);
    let tag = Tag {
        kind: Kind::Tag,
        key: b"due".to_vec(),
        text: format!("due:{date}").into_bytes(),
    };
    let next = match apply_tag(tx, id, &line, &tag)? {
        Applied::Kept => {
            tx.say(format!("{handle} is already due {date}"));
            return Ok(());
        }
        Applied::Added(next) | Applied::Set(next, _) => next,
    };
    tx.set_edited(&env.actor, id, next.raw().to_vec())?;
    tx.say(format!("{handle} due {date}"));
    Ok(())
}

fn clear_due(tx: &mut Tx, env: &Env<'_>, id: ItemId) -> Result<()> {
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    let handle = tx.handle(id);
    let Some(at) = first_of_key(&line, b"due") else {
        tx.say(format!("{handle} has no due date"));
        return Ok(());
    };
    let next = line
        .remove_token(at)
        .map_err(|_| tx.stamp_word_refusal(id))?;
    tx.set_edited(&env.actor, id, next.raw().to_vec())?;
    tx.say(format!("{handle} due date cleared"));
    Ok(())
}
