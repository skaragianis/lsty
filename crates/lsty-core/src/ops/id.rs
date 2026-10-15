//! `id`: give a task an id, chosen or random.

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
    errs::{Code, Error, Result, lossy},
    list::TaskId,
    todotxt::Id,
};

/// Gives each target an id: the one chosen, or else a random one. An
/// task that has an id keeps it.
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
        return Err(Error::new(Code::Usage, "a chosen id takes one task"));
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
    tx.each_edit(targets, &env.actor, |tx, task| {
        give_id(tx, env, task, chosen, &mut taken)
    })
}

fn give_id(
    tx: &mut Tx,
    env: &Env<'_>,
    task: TaskId,
    chosen: Option<&str>,
    taken: &mut Taken,
) -> Result<()> {
    if let Some(have) = tx.id_bytes(task) {
        keep_id(tx, task, &have, chosen)?;
        let have = lossy(&have);
        let handle = tx.handle(task);
        tx.say(format!("{handle} is id:{have} (unchanged)"));
        return Ok(());
    }
    let id = match chosen {
        Some(chosen) => chosen.to_owned(),
        None => taken.draw(tx.snapshot(), env.ids)?.to_string(),
    };
    put_id(tx, task, &id)
}

/// Appends `id:ID` to the task and says so.
pub(super) fn put_id(tx: &mut Tx, task: TaskId, id: &str) -> Result<()> {
    let line = tx.current(task).ok_or_else(gone)?;
    let line = tx.append_tok(task, &line, format!("id:{id}").as_bytes())?;
    tx.set(task, line.raw(), Path::Action);
    let handle = tx.handle(task);
    tx.say(format!("{handle} is id:{id}"));
    Ok(())
}

/// Refuses `chosen` for a task that has the id `have`, unless it is
/// the same.
pub(super) fn keep_id(
    tx: &Tx,
    task: TaskId,
    have: &[u8],
    chosen: Option<&str>,
) -> Result<()> {
    match (tx.task(task), chosen) {
        (Some(found), Some(c)) if c.as_bytes() != have => Err(wrong_state(
            tx.snapshot(),
            found,
            found.state(),
            format!("{} already has id:{}", tx.handle(task), lossy(have)),
            "an id is never changed".to_owned(),
        )),
        _ => Ok(()),
    }
}
