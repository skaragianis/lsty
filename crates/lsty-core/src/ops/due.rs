//! `due`: set or clear the due date.

use super::{
    env::Env,
    refs::Target,
    result::Outcome,
    run::run,
    tag::{Applied, Tag, apply_tag, has},
    tokens::first_of_key,
    tx::Tx,
};
use crate::{
    dates::{self, Date},
    errs::{Field, Result},
    list::TaskId,
    todotxt::{Kind, Line},
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

/// Whether `due` with `input` would leave `line` as it is: it is due that
/// day already, or has no date to clear; not for an input that is not a
/// date, which the press refuses with its own words.
#[must_use]
pub fn due_keeps(line: &Line, input: &str, today: Date) -> bool {
    match dates::resolve(input, today) {
        Ok(None) => first_of_key(line, b"due").is_none(),
        Ok(Some(date)) => has(line, &due_tag(&date.to_string())),
        Err(_) => false,
    }
}

/// The `due:` tag of `date`.
fn due_tag(date: &str) -> Tag {
    Tag {
        kind: Kind::Tag,
        key: b"due".to_vec(),
        text: format!("due:{date}").into_bytes(),
    }
}

fn set_due(tx: &mut Tx, env: &Env<'_>, id: TaskId, date: &str) -> Result<()> {
    let line = tx.line_of(id)?.clone();
    let handle = tx.handle(id);
    let next = match apply_tag(tx, id, &line, &due_tag(date))? {
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

fn clear_due(tx: &mut Tx, env: &Env<'_>, id: TaskId) -> Result<()> {
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
