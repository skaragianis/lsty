//! `due`: set or clear the due date.

use super::{
    env::Env,
    refs::Target,
    result::Outcome,
    run::run,
    tag::{Applied, Tag, apply_tag},
    tokens::first_of_key,
    tx::Tx,
};
use crate::{
    dates,
    errs::{Field, Result},
    list::ItemId,
    todotxt::Kind,
};

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
        tx.each_edit(targets, &env.actor, |tx, id| match resolved {
            None => clear_due(tx, env, id),
            Some(date) => set_due(tx, env, id, &date.to_string()),
        })
    })
}

fn set_due(tx: &mut Tx, env: &Env<'_>, id: ItemId, date: &str) -> Result<()> {
    let line = tx.line_of(id)?.clone();
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
    let line = tx.line_of(id)?.clone();
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
