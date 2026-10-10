//! `start`: mark items as underway.

use super::{
    claim::{
        ClaimMode, check_item_claim, claim_phrase, held_by_other, holder_name,
    },
    env::Env,
    lifecycle::{StartOpts, refuse_blocked, say_was_blocked, with_claim},
    refs::Target,
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Tx, gone, wrong_state},
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
    let item = tx.item(id).ok_or_else(gone)?;
    let line = tx.snapshot().line(item).clone();
    match item.state() {
        State::Done => {
            return Err(wrong_state(
                tx.snapshot(),
                item,
                State::Done,
                format!("{handle} is done"),
                format!("use lsty reopen {handle} first"),
            ));
        }
        State::Doing => {
            return restart_item(tx, env, (id, &line), stamp, opts);
        }
        State::Todo => {}
    }
    if !opts.ignore_blocked {
        refuse_blocked(tx, id, item)?;
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
    let item = tx.item(id).ok_or_else(gone)?;
    let snap = tx.snapshot();
    if !held_by_other(snap, &env.actor, item) {
        tx.say(format!("{handle} is already underway"));
        return Ok(());
    }
    check_item_claim(snap, &env.actor, item, &handle, opts.claim)?;
    let holder = holder_name(snap, item).to_vec();
    let claimed = with_claim(tx, id, line, &env.actor, stamp)?;
    tx.set(id, claimed.raw(), Path::Action);
    let phrase = claim_phrase(env.actor.is_human(), ClaimMode::Ends);
    let mut said = [handle.as_bytes(), phrase, &holder].concat();
    if env.actor.is_human() {
        said.extend_from_slice(b"; now yours");
    }
    tx.say(said);
    Ok(())
}
