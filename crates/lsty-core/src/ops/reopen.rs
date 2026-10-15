//! `reopen`: un-complete done tasks.

use super::{
    env::{Actor, Env},
    lifecycle::{ClaimOpts, emptied, hollowed, is_pri_tag, with_claim},
    refs::Target,
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Tx, gone},
};
use crate::{
    errs::Result,
    list::{State, TaskId},
    todotxt::{Line, Parts, Priority, build},
};

/// Un-completes each done target of the todo file. There is no blocked
/// check.
///
/// # Errors
///
/// `wrong_state` for a task of the done file, and for a task that would
/// be left without a description.
pub fn reopen(
    env: &Env<'_>,
    targets: &[Target],
    opts: ClaimOpts,
) -> Result<Outcome> {
    let stamp = env.clock.stamp();
    run(env, "reopen", false, |tx| {
        tx.each_task(targets, |tx, id| reopen_task(tx, env, id, &stamp, opts))
    })
}

fn reopen_task(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    stamp: &str,
    opts: ClaimOpts,
) -> Result<()> {
    let handle = tx.handle(id);
    let task = tx.task(id).ok_or_else(gone)?;
    let (state, line) = (task.state(), tx.snapshot().line(task).clone());
    if state != State::Done {
        tx.say(format!("{handle} is already open"));
        return Ok(());
    }
    let (open, back) = unfinished(tx, &env.actor, (id, &line), stamp, opts)?;
    tx.set(id, open.raw(), Path::Action);
    tx.say([handle.as_bytes(), b" reopened", &back].concat());
    if line.tags().rec.is_some() {
        tx.say(format!(
            "{handle} has rec:; its next occurrence stays in the list"
        ));
    }
    Ok(())
}

/// The line of the task `id` without its mark, taken over when the
/// options say so, and what it says about whose claim it is back to.
///
/// # Errors
///
/// `wrong_state` when nothing would be left of the line.
pub(super) fn unfinished(
    tx: &Tx,
    actor: &Actor,
    (id, line): (TaskId, &Line),
    stamp: &str,
    opts: ClaimOpts,
) -> Result<(Line, Vec<u8>)> {
    let open = reopened(tx, id, line);
    if hollowed(line, &open) {
        return Err(emptied(tx, id));
    }
    let back = back_to(actor, line, opts);
    if !opts.takeover {
        return Ok((open, back));
    }
    Ok((with_claim(tx, id, &open, actor, stamp)?, back))
}

/// What a reopened task says about whose claim it is back to.
fn back_to(actor: &Actor, line: &Line, opts: ClaimOpts) -> Vec<u8> {
    let tags = line.tags();
    match tags.owner {
        _ if opts.takeover => b"; now yours".to_vec(),
        _ if !tags.underway => Vec::new(),
        Some(owner) => [b"; back to ", owner].concat(),
        None if actor.is_human() => b"; yours again".to_vec(),
        None => b"; back to the human".to_vec(),
    }
}

/// `line` without its mark and completion date, with its priority back
/// and its `pri:` and `ended:` tags gone (all kept when removing them
/// would change a stamp).
fn reopened(tx: &Tx, id: TaskId, line: &Line) -> Line {
    let open = Line::parse(build(&Parts {
        priority: restored_priority(line),
        created: line.created(),
        description: line.description(),
        ..Parts::default()
    }));
    match tx.remove_where(id, &open, |t| is_pri_tag(t) || t.is_tag(b"ended")) {
        Ok((bare, _)) => bare,
        Err(_) => open,
    }
}

/// The `(X)` field, or else the first valid `pri:`.
pub(super) fn restored_priority(line: &Line) -> Option<Priority> {
    line.priority().or_else(|| {
        line.tokens()
            .filter(is_pri_tag)
            .find_map(|t| match t.value {
                [letter] => Priority::new(*letter),
                _ => None,
            })
    })
}
