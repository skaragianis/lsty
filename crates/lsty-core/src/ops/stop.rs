//! `stop`: take underway items back to todo.

use super::env::Env;
use super::lifecycle::{ClaimOpts, emptied, hollowed, is_claim_tag};
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::Tx;
use crate::errs::Result;
use crate::list::{ItemId, State};

/// Removes `started:` and `owner:` from each underway target. There is
/// no blocked check.
///
/// # Errors
///
/// `wrong_state` when the item would be left without a description.
pub fn stop(
    env: &Env<'_>,
    targets: &[Target],
    _opts: ClaimOpts,
) -> Result<Outcome> {
    run(env, "stop", false, |tx| tx.each_item(targets, stop_item))
}

fn stop_item(tx: &mut Tx, id: ItemId) -> Result<()> {
    let handle = tx.handle(id);
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    if tx.item(id).map(crate::list::Item::state) != Some(State::Doing) {
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
