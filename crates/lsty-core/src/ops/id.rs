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
    if let Some(have) = tx.id_bytes(item) {
        let have = String::from_utf8_lossy(&have);
        if chosen.is_some_and(|c| c != have) {
            return Err(id_kept(tx, item, &have));
        }
        let handle = tx.handle(item);
        tx.say(format!("{handle} is id:{have} (unchanged)"));
        return Ok(());
    }
    let id = match chosen {
        Some(chosen) => chosen.to_owned(),
        None => taken.draw(tx.snapshot(), env.ids)?.to_string(),
    };
    put_id(tx, item, &id)
}

/// Appends `id:ID` to the item and says so.
pub(super) fn put_id(tx: &mut Tx, item: ItemId, id: &str) -> Result<()> {
    let line = tx.current(item).ok_or_else(gone)?;
    let line = tx.append_tok(item, &line, format!("id:{id}").as_bytes())?;
    tx.set(item, line.raw(), Path::Action);
    let handle = tx.handle(item);
    tx.say(format!("{handle} is id:{id}"));
    Ok(())
}

/// The error for asking an item that has the id `have` for another one.
pub(super) fn id_kept(tx: &Tx, item: ItemId, have: &str) -> Error {
    let Some(found) = tx.item(item) else {
        return gone();
    };
    wrong_state(
        tx.snapshot(),
        found,
        found.state(),
        format!("{} already has id:{have}", tx.handle(item)),
        "an id is never changed".to_owned(),
    )
}
