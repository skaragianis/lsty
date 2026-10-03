//! `id`: give an item an id, chosen or random.

use super::env::Env;
use super::ids::Taken;
use super::refs::{Target, invalid_id};
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::{Tx, wrong_state};
use crate::errs::{Code, Error, Result};
use crate::list::ItemId;
use crate::todotxt::{EditError, Id};

/// Gives each target an id: the one chosen, or else a random one. An
/// item that has an id keeps it.
pub fn id(
    env: &Env<'_>,
    targets: &[Target],
    chosen: Option<&str>,
) -> Result<Outcome> {
    check_chosen(chosen, targets.len())?;
    run(env, "id", false, |tx| give_ids(tx, env, targets, chosen))
}

pub(super) fn check_chosen(chosen: Option<&str>, count: usize) -> Result<()> {
    let Some(chosen) = chosen else {
        return Ok(());
    };
    if !Id::is_valid(chosen) {
        return Err(invalid_id(chosen));
    }
    if count != 1 {
        return Err(Error::new(Code::Usage, "a chosen id takes one item"));
    }
    Ok(())
}

fn give_ids(
    tx: &mut Tx,
    env: &Env<'_>,
    targets: &[Target],
    chosen: Option<&str>,
) -> Result<()> {
    let staged = tx.staged_ids();
    let mut taken =
        Taken::new(tx.snapshot(), staged.iter().map(Vec::as_slice));
    tx.each_edit(targets, &env.actor, |tx, item| {
        give_id(tx, env, item, chosen, &mut taken)
    })
}

fn give_id(
    tx: &mut Tx,
    env: &Env<'_>,
    item: ItemId,
    chosen: Option<&str>,
    taken: &mut Taken,
) -> Result<()> {
    let handle = tx.handle(item);
    let have = tx
        .item(item)
        .and_then(|i| tx.snapshot().id_of(i))
        .map(|id| String::from_utf8_lossy(id).into_owned());
    if let Some(have) = have {
        return keep(tx, item, &handle, &have, chosen);
    }
    let id = match chosen {
        Some(chosen) => chosen.to_owned(),
        None => taken.draw(env.ids)?,
    };
    let line = tx
        .line(item)
        .map(|l| l.append_token(format!("id:{id}").as_bytes()));
    match line {
        Some(Ok(line)) => {
            tx.set(item, line.raw().to_vec(), Path::Action);
            tx.say(format!("{handle} is id:{id}"));
            Ok(())
        }
        Some(Err(EditError::NoToken | EditError::Refused(_))) => {
            Err(tx.stamp_word_refusal(item))
        }
        None => Ok(()),
    }
}

/// An item that has an id keeps it: a no-op, or an error when another
/// id is asked for.
fn keep(
    tx: &mut Tx,
    item: ItemId,
    handle: &str,
    have: &str,
    chosen: Option<&str>,
) -> Result<()> {
    if chosen.is_some_and(|c| c != have)
        && let Some(found) = tx.item(item)
    {
        return Err(wrong_state(
            tx.snapshot(),
            found,
            found.state(),
            format!("{handle} already has id:{have}"),
            "an id is never changed".to_owned(),
        ));
    }
    tx.say(format!("{handle} is id:{have} (unchanged)"));
    Ok(())
}
