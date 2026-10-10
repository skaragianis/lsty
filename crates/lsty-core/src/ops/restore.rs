//! `restore`: move items of the done file back to the todo file.

use super::{
    env::Env,
    lifecycle::{ClaimOpts, emptied, hollowed},
    refs::Target,
    reopen::{reopened, unfinished},
    result::Outcome,
    run::run,
    tx::Tx,
};
use crate::{
    errs::{Code, Error, Result, Source},
    list::{Item, ItemId, State},
};

/// Moves each target back to the todo file: an item of the done file is
/// reopened, as `reopen` does, and appended; its done-file line is
/// blanked. An item that is only in the todo file is left alone, and one
/// that is in both files keeps its todo copy while the done-file line is
/// blanked.
///
/// # Errors
///
/// `usage` for a target that is a todo number, `wrong_state` for an item
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
        tx.dest(Source::Todo);
        tx.each_item(&targets, |tx, id| {
            restore_item(tx, env, id, &stamp, opts)
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

fn restore_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    stamp: &str,
    opts: ClaimOpts,
) -> Result<()> {
    let key = tx.id_bytes(id);
    let in_todo = tx.item(id).map(Item::source) == Some(Source::Todo);
    if in_todo {
        if let Some(left) = tx.copy_in(Source::Done, key.as_deref()) {
            finish_restore(tx, left);
            return Ok(());
        }
        let (handle, name) = (tx.handle(id), tx.todo_name());
        tx.say(format!("{handle} is already in {name}"));
        return Ok(());
    }
    if tx.copy_in(Source::Todo, key.as_deref()).is_some() {
        finish_restore(tx, id);
        return Ok(());
    }
    move_back(tx, env, id, stamp, opts)
}

/// Blanks the done-file line `left`, which a crash left behind.
fn finish_restore(tx: &mut Tx, left: ItemId) {
    tx.blank(left);
    if let Some(id) = tx.id_bytes(left) {
        tx.say_interrupted(&id);
    }
}

fn move_back(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    stamp: &str,
    opts: ClaimOpts,
) -> Result<()> {
    let Some(item) = tx.item(id) else {
        return Ok(());
    };
    let old = tx.file(item.source()).line_ref(item.nr().index());
    let (state, mut line) = (item.state(), tx.snapshot().line(item).clone());
    let mut suffix = Vec::new();
    if state == State::Done {
        let open = reopened(tx, id, &line);
        if hollowed(&line, &open) {
            return Err(emptied(tx, id));
        }
        (line, suffix) = unfinished(tx, &env.actor, id, open, (stamp, opts))?;
    }
    let line_ref = tx.insert_at_end(Source::Todo, line.raw());
    if let Some(row) = tx.row_at_mut(line_ref) {
        row.origin = Some(id);
    }
    tx.retarget(old, line_ref);
    tx.blank(id);
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
