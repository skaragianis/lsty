//! `stop`: take underway items back to todo.

use super::{
    claim::{ClaimMode, Guard},
    env::Env,
    lifecycle::{ClaimOpts, emptied, hollowed},
    refs::Target,
    result::Outcome,
    run::run,
    staged::Path,
    tokens::is_claim_tag,
    tx::Tx,
};
use crate::{
    errs::Result,
    list::{ItemId, State},
};

/// Removes `started:` and `owner:` from each underway target. There is
/// no blocked check.
///
/// # Errors
///
/// `claimed` for an agent on another actor's claim without `takeover`,
/// and `wrong_state` when the item would be left without a description.
pub fn stop(
    env: &Env<'_>,
    targets: &[Target],
    opts: ClaimOpts,
) -> Result<Outcome> {
    run(env, "stop", false, |tx| {
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Ends,
            opts,
        };
        tx.each_guarded(targets, guard, stop_item)
    })
}

fn stop_item(tx: &mut Tx, id: ItemId) -> Result<()> {
    let handle = tx.handle(id);
    let Some(item) = tx.item(id) else {
        return Ok(());
    };
    let (state, line) = (item.state(), tx.snapshot().line(item).clone());
    if state != State::Doing {
        tx.say(format!("{handle} is not underway"));
        return Ok(());
    }
    let (stopped, _) = tx.remove_where(id, &line, is_claim_tag)?;
    if hollowed(&line, &stopped) {
        return Err(emptied(tx, id));
    }
    tx.set(id, stopped.raw(), Path::Action);
    tx.say(format!("{handle} stopped"));
    Ok(())
}
