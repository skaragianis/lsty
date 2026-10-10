//! `id`: give an item an id, chosen or random.

use super::{
    env::Env,
    ids::Taken,
    refs::{Target, invalid_id},
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Tx, gone, wrong_state},
};
use crate::{
    errs::{Code, Error, Result},
    list::ItemId,
    todotxt::Id,
};

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
    let mut taken = Taken::new(tx.edited_ids());
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
    if let Some(have) = tx.id_bytes(item) {
        let have = String::from_utf8_lossy(&have);
        return keep(tx, item, &handle, &have, chosen);
    }
    let id = match chosen {
        Some(chosen) => chosen.to_owned(),
        None => taken.draw(tx.snapshot(), env.ids)?.to_string(),
    };
    let line =
        tx.append_tok(item, tx.line_of(item)?, format!("id:{id}").as_bytes())?;
    tx.set(item, line.raw(), Path::Action);
    tx.say(format!("{handle} is id:{id}"));
    Ok(())
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
    if chosen.is_some_and(|c| c != have) {
        let found = tx.item(item).ok_or_else(gone)?;
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
