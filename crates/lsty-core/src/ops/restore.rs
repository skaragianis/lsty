//! `restore`: move tasks of the done file back to the todo file.

use super::{
    env::Env,
    lifecycle::ClaimOpts,
    refs::Target,
    reopen::unfinished,
    result::Outcome,
    run::run,
    tx::{Tx, gone},
};
use crate::{
    errs::{Code, Error, Result, Source},
    list::{State, Task, TaskId},
};

/// Moves each target back to the todo file: a task of the done file is
/// reopened, as `reopen` does, and appended; its done-file line is
/// blanked. A task that is only in the todo file is left alone, and one
/// that is in both files keeps its todo copy while the done-file line is
/// blanked.
///
/// # Errors
///
/// `usage` for a target that is a todo number, `wrong_state` for a task
/// that would be left without a description, and whatever the rules
/// refuse.
pub fn restore(
    env: &Env<'_>,
    targets: &[Target],
    opts: ClaimOpts,
) -> Result<Outcome> {
    need_archived(targets)?;
    let targets: Vec<Target> =
        targets.iter().cloned().map(Target::archived).collect();
    let stamp = env.clock.stamp();
    run(env, "restore", true, |tx| {
        tx.dest = Some(Source::Todo);
        tx.each_task(&targets, |tx, id| {
            restore_task(tx, env, id, &stamp, opts)
        })
    })
}

/// Refuses a target that names a line of the todo file by number.
fn need_archived(targets: &[Target]) -> Result<()> {
    let Some(target) = targets
        .iter()
        .find(|t| matches!(t.line(), Some((Source::Todo, _))))
    else {
        return Ok(());
    };
    Err(Error::new(
        Code::Usage,
        format!("{} is not an archived item", target.handle()),
    )
    .with_hint("use done:NR or id:"))
}

fn restore_task(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    stamp: &str,
    opts: ClaimOpts,
) -> Result<()> {
    if tx.task(id).map(Task::source) == Some(Source::Todo) {
        if let Some(left) = tx.copy_in(Source::Done, id) {
            finish_restore(tx, left);
            return Ok(());
        }
        let (handle, name) = (tx.handle(id), tx.file_name(Source::Todo));
        tx.say(format!("{handle} is already in {name}"));
        return Ok(());
    }
    if tx.copy_in(Source::Todo, id).is_some() {
        finish_restore(tx, id);
        return Ok(());
    }
    move_back(tx, env, id, stamp, opts)
}

/// Blanks the done-file line `left`, which a crash left behind.
fn finish_restore(tx: &mut Tx, left: TaskId) {
    tx.blank(left);
    if let Some(id) = tx.id_bytes(left) {
        tx.say_interrupted(&id);
    }
}

fn move_back(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    stamp: &str,
    opts: ClaimOpts,
) -> Result<()> {
    let task = tx.task(id).ok_or_else(gone)?;
    let (state, mut line) = (task.state(), tx.snapshot().line(task).clone());
    let mut suffix = Vec::new();
    if state == State::Done {
        (line, suffix) = unfinished(tx, &env.actor, (id, &line), stamp, opts)?;
    }
    let (line_ref, _) = tx.send_to(Source::Todo, id, line.shared())?;
    let handle = tx.handle(id);
    let nr = tx.written_nr(line_ref);
    tx.say(
        [
            handle.as_bytes(),
            b" restored as ",
            nr.to_string().as_bytes(),
            &suffix,
        ]
        .concat(),
    );
    Ok(())
}
