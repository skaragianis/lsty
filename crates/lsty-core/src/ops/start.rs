//! `start`: mark tasks as underway.

use super::{
    claim::{
        ClaimMode, check_task_claim, claim_phrase, held_by_other, holder_name,
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
    list::{State, TaskId},
    todotxt::Line,
};

/// Marks each target as underway, with the time of the clock.
///
/// # Errors
///
/// `wrong_state` for a done task, `blocked` for a task that waits for
/// another unless `ignore_blocked`, and whatever the rules refuse.
pub fn start(
    env: &Env<'_>,
    targets: &[Target],
    opts: StartOpts,
) -> Result<Outcome> {
    let stamp = env.clock.stamp();
    run(env, "start", false, |tx| {
        tx.each_task(targets, |tx, id| start_task(tx, env, id, &stamp, opts))
    })
}

fn start_task(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    stamp: &str,
    opts: StartOpts,
) -> Result<()> {
    let name = tx.name(id);
    let task = tx.task(id).ok_or_else(gone)?;
    let line = tx.snapshot().line(task).clone();
    match task.state() {
        State::Done => {
            return Err(wrong_state(
                tx.snapshot(),
                task,
                State::Done,
                name.then(" is done"),
                format!("use lsty reopen {} first", tx.given(id)),
            ));
        }
        State::Doing => {
            return restart_task(tx, env, (id, &line), stamp, opts);
        }
        State::Todo => {}
    }
    if !opts.ignore_blocked {
        refuse_blocked(tx, id, task)?;
    }
    let claimed = with_claim(tx, id, &line, &env.actor, stamp)?;
    tx.set(id, claimed.raw(), Path::Action);
    let by = match env.actor.name() {
        "" => String::new(),
        name => format!(" by {name}"),
    };
    tx.say(name.then(format!(" started{by}")));
    if opts.ignore_blocked {
        say_was_blocked(tx, id);
    }
    Ok(())
}

/// Starts a task that is underway: a no-op on the actor's own claim,
/// otherwise a takeover.
fn restart_task(
    tx: &mut Tx,
    env: &Env<'_>,
    (id, line): (TaskId, &Line),
    stamp: &str,
    opts: StartOpts,
) -> Result<()> {
    let name = tx.name(id);
    let task = tx.task(id).ok_or_else(gone)?;
    let snap = tx.snapshot();
    if !held_by_other(snap, &env.actor, task) {
        tx.say(name.then(" is already underway"));
        return Ok(());
    }
    check_task_claim(snap, &env.actor, task, &name.typed, opts.claim)?;
    let holder = holder_name(snap, task).to_vec();
    let claimed = with_claim(tx, id, line, &env.actor, stamp)?;
    tx.set(id, claimed.raw(), Path::Action);
    let phrase = claim_phrase(env.actor.is_human(), ClaimMode::Ends);
    let mut said = name.then(phrase).then(holder);
    if env.actor.is_human() {
        said = said.then("; now yours");
    }
    tx.say(said);
    Ok(())
}
