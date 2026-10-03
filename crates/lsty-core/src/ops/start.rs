//! `start`: mark items as underway.

use super::env::Env;
use super::lifecycle::{
    StartOpts, is_claim_tag, is_done, refuse_blocked, say_was_blocked,
};
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::Tx;
use crate::errs::Result;
use crate::list::{ItemId, State};

/// Marks each target as underway, with the time of the clock.
///
/// # Errors
///
/// `wrong_state` for a done item, `blocked` for an item that waits for
/// another unless `ignore_blocked`, and whatever the rules refuse.
pub fn start(
    env: &Env<'_>,
    targets: &[Target],
    opts: StartOpts,
) -> Result<Outcome> {
    let stamp = env.clock.stamp();
    run(env, "start", false, |tx| {
        tx.each_item(targets, |tx, id| start_item(tx, id, &stamp, opts))
    })
}

fn start_item(
    tx: &mut Tx,
    id: ItemId,
    stamp: &str,
    opts: StartOpts,
) -> Result<()> {
    let handle = tx.handle(id);
    match tx.item(id).map(crate::list::Item::state) {
        Some(State::Done) => return Err(is_done(tx, id)),
        Some(State::Doing) => {
            tx.say(format!("{handle} is already underway"));
            return Ok(());
        }
        Some(State::Todo) | None => {}
    }
    if !opts.ignore_blocked {
        refuse_blocked(tx, id)?;
    }
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    let (bare, _) = tx.remove_where(id, &line, is_claim_tag)?;
    let claimed =
        tx.append_tok(id, &bare, format!("started:{stamp}").as_bytes())?;
    tx.set(id, claimed.raw(), Path::Action);
    tx.say(format!("{handle} started"));
    if opts.ignore_blocked {
        say_was_blocked(tx, id);
    }
    Ok(())
}
