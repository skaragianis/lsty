//! `start`: mark items as underway.

use super::{
    env::Env,
    lifecycle::{
        StartOpts, is_done, refuse_blocked, say_was_blocked, with_claim,
    },
    refs::Target,
    result::Outcome,
    run::run,
    staged::Path,
    tx::Tx,
};
use crate::{
    errs::Result,
    list::{ItemId, State},
    todotxt::Line,
};

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
        tx.each_item(targets, |tx, id| start_item(tx, env, id, &stamp, opts))
    })
}

fn start_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    stamp: &str,
    opts: StartOpts,
) -> Result<()> {
    let handle = tx.handle(id);
    let Some(item) = tx.item(id) else {
        return Ok(());
    };
    let line = tx.snapshot().line(item).clone();
    match item.state() {
        State::Done => return Err(is_done(tx, id)),
        State::Doing => {
            return restart_item(tx, env, (id, &line), stamp, opts);
        }
        State::Todo => {}
    }
    if !opts.ignore_blocked {
        refuse_blocked(tx, id)?;
    }
    let claimed = with_claim(tx, id, &line, &env.actor, stamp)?;
    tx.set(id, claimed.raw(), Path::Action);
    let by = match env.actor.name() {
        "" => String::new(),
        name => format!(" by {name}"),
    };
    tx.say(format!("{handle} started{by}"));
    if opts.ignore_blocked {
        say_was_blocked(tx, id);
    }
    Ok(())
}

/// Starts an item that is underway: a no-op on the actor's own claim,
/// otherwise a takeover.
fn restart_item(
    tx: &mut Tx,
    env: &Env<'_>,
    (id, line): (ItemId, &Line),
    stamp: &str,
    opts: StartOpts,
) -> Result<()> {
    let handle = tx.handle(id);
    if !tx.held_by_other(&env.actor, id) {
        tx.say(format!("{handle} is already underway"));
        return Ok(());
    }
    tx.check_claim(&env.actor, id, opts.claim)?;
    let claimed = with_claim(tx, id, line, &env.actor, stamp)?;
    tx.set(id, claimed.raw(), Path::Action);
    let holder = tx.holder_name(id);
    let phrase: &[u8] = if env.actor.is_human() {
        b" was claimed by "
    } else {
        b" taken over from "
    };
    let mut said = [handle.as_bytes(), phrase, &holder].concat();
    if env.actor.is_human() {
        said.extend_from_slice(b"; now yours");
    }
    tx.say(said);
    Ok(())
}
