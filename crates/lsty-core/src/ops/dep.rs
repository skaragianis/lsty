//! `dep` and `undep`: make tasks wait for others, or stop.

use super::{
    claim::Guard,
    env::Env,
    id::{check_chosen, chosen_or_drawn, keep_id, put_id},
    ids::Taken,
    refs::{Target, no_task},
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Picked, Tx, gone, wrong_state},
};
use crate::{
    errs::{Result, Source},
    list::TaskId,
    todotxt::Line,
};

/// Makes each waiting task wait for each prerequisite. A prerequisite
/// without an id gets one: `chosen`, or a random one.
///
/// # Errors
///
/// A usage error when `chosen` is given for several prerequisites,
/// `invalid_value` when an agent chooses one for a prerequisite without
/// an id, `wrong_state` for a prerequisite that has another id or is archived
/// without one, `cycle` for a dependency that closes a loop, and
/// whatever the rules refuse.
pub fn dep(
    env: &Env<'_>,
    waiting: &[Target],
    prereqs: &[Target],
    chosen: Option<&str>,
) -> Result<Outcome> {
    check_chosen(chosen, prereqs.len())?;
    run(env, "dep", false, |tx| {
        let pres = prerequisites(tx, prereqs)?;
        let mut ids: Option<Vec<Vec<u8>>> = None;
        tx.each_edit(waiting, &env.actor, |tx, task| {
            if ids.is_none() {
                ids = Some(prerequisite_ids(tx, env, &pres, chosen)?);
            }
            let ids = ids.as_deref().unwrap_or_default();
            add_deps(tx, task, &pres, ids)
        })
    })
}

/// The tasks the prerequisite targets name, once each. A prerequisite
/// may be in the done file.
fn prerequisites(tx: &mut Tx, targets: &[Target]) -> Result<Vec<TaskId>> {
    let mut out = Vec::new();
    for target in targets {
        let target = target.clone().archived();
        match tx.target(&target)? {
            Picked::Gone(_) => return Err(no_task(&target)),
            Picked::Task(id) if !out.contains(&id) => out.push(id),
            Picked::Task(_) => {}
        }
    }
    Ok(out)
}

fn prerequisite_ids(
    tx: &mut Tx,
    env: &Env<'_>,
    pres: &[TaskId],
    chosen: Option<&str>,
) -> Result<Vec<Vec<u8>>> {
    let mut taken = Taken::new(tx.edited_ids());
    pres.iter()
        .map(|&pre| prerequisite_id(tx, env, pre, chosen, &mut taken))
        .collect()
}

/// The id a prerequisite has, or gets.
fn prerequisite_id(
    tx: &mut Tx,
    env: &Env<'_>,
    pre: TaskId,
    chosen: Option<&str>,
    taken: &mut Taken,
) -> Result<Vec<u8>> {
    let handle = tx.handle(pre);
    let task = tx.task(pre).ok_or_else(gone)?;
    if let Some(have) = tx.id_bytes(pre) {
        keep_id(tx, pre, &have, chosen)?;
        return Ok(have);
    }
    if task.source() == Source::Done {
        return Err(wrong_state(
            tx.snapshot(),
            task,
            task.state(),
            format!("{handle} has no id and is archived"),
            "restore it first, then give it an id".to_owned(),
        ));
    }
    let id = chosen_or_drawn(tx, env, chosen, taken)?;
    give_id(tx, env, pre, &id)?;
    Ok(id.into_bytes())
}

/// Appends `id:` to a prerequisite, as a metadata edit on it.
pub(super) fn give_id(
    tx: &mut Tx,
    env: &Env<'_>,
    pre: TaskId,
    id: &str,
) -> Result<()> {
    tx.guarded(Guard::edit(&env.actor), pre, |tx| {
        put_id(tx, pre, id)?;
        let line_ref = tx.line_ref(pre)?;
        tx.side.push(line_ref);
        Ok(())
    })
}

fn add_deps(
    tx: &mut Tx,
    task: TaskId,
    pres: &[TaskId],
    ids: &[Vec<u8>],
) -> Result<()> {
    let handle = tx.handle(task);
    let mut line = tx.current(task).ok_or_else(gone)?;
    for (&pre, id) in pres.iter().zip(ids) {
        let prereq = tx.handle(pre);
        if let Some(waiting) = with_dep(tx, task, &line, id)? {
            line = waiting;
            tx.say(format!("{handle} now waits for {prereq}"));
        } else {
            tx.say(format!("{handle} already waits for {prereq}"));
        }
    }
    tx.set(task, line.raw(), Path::Action);
    Ok(())
}

/// Makes `task` wait for the task whose id is `id`, as a side effect of
/// the operation. A task that waits already is left alone.
pub(super) fn wait_for(
    tx: &mut Tx,
    env: &Env<'_>,
    task: TaskId,
    id: &[u8],
) -> Result<()> {
    tx.guarded(Guard::edit(&env.actor), task, |tx| {
        let line = tx.current(task).ok_or_else(gone)?;
        if let Some(line) = with_dep(tx, task, &line, id)? {
            tx.set(task, line.raw(), Path::Action);
            let line_ref = tx.line_ref(task)?;
            tx.side.push(line_ref);
        }
        Ok(())
    })
}

/// `line` of `task` with the `dep:` that makes it wait for `id`, or
/// `None` when it waits already.
fn with_dep(
    tx: &mut Tx,
    task: TaskId,
    line: &Line,
    id: &[u8],
) -> Result<Option<Line>> {
    if line.tags().deps.contains(&id) {
        return Ok(None);
    }
    tx.append_tok(task, line, &[b"dep:", id].concat()).map(Some)
}

/// Stops each waiting task waiting for each prerequisite.
///
/// # Errors
///
/// `no_task` for a prerequisite that is not there, and whatever the
/// rules refuse.
pub fn undep(
    env: &Env<'_>,
    waiting: &[Target],
    prereqs: &[Target],
) -> Result<Outcome> {
    run(env, "undep", false, |tx| {
        let pres = prerequisites(tx, prereqs)?;
        tx.each_edit(waiting, &env.actor, |tx, task| {
            drop_deps(tx, task, &pres)
        })
    })
}

fn drop_deps(tx: &mut Tx, task: TaskId, pres: &[TaskId]) -> Result<()> {
    let handle = tx.handle(task);
    let mut line = tx.current(task).ok_or_else(gone)?;
    for &pre in pres {
        let prereq = tx.handle(pre);
        let id = tx.id_bytes(pre).unwrap_or_default();
        let (next, removed) = tx.remove_where(task, &line, |t| {
            t.is_tag(b"dep") && t.value == id
        })?;
        line = next;
        if removed == 0 {
            tx.say(format!("{handle} does not wait for {prereq}"));
        } else {
            tx.say(format!("{handle} no longer waits for {prereq}"));
        }
    }
    tx.set(task, line.raw(), Path::Action);
    Ok(())
}
