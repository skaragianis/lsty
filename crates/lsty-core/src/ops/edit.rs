//! `edit`: an item changed in an editor while no lock is held. The item
//! is picked first, followed through the editor session by its line, and
//! given the new text under the lock.

use super::{
    claim::{ClaimMode, Guard, check_item_claim},
    env::{Actor, Env},
    lifecycle::ClaimOpts,
    refs::{Target, resolve},
    result::Outcome,
    rules::edit_line,
    run::run,
    staged::Path,
    tx::{Picked, Tx, archived_error},
};
use crate::{
    errs::{Code, Error, Result, Source},
    lineid::follow,
    list::{ItemId, Snapshot},
    todotxt::{Line, trim},
};

/// An item picked for an edit, with the lines of the todo file as they
/// were when it was picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditPlan {
    given: String,
    handle: String,
    index: usize,
    raw: Vec<u8>,
    before: Vec<Vec<u8>>,
}

impl EditPlan {
    /// The line of the item as it was picked: what the editor starts
    /// with.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// How messages name the item.
    pub fn handle(&self) -> &str {
        &self.handle
    }
}

/// Resolves `target` in `snap` and applies the claim guard, so that a
/// refused edit fails before anyone types.
///
/// # Errors
///
/// `no_item`, a refused `--expect`, `wrong_state` for an archived item
/// and `claimed` for an agent on another actor's claim without
/// `takeover`.
pub fn prepare_edit(
    snap: &Snapshot,
    actor: &Actor,
    target: &Target,
    opts: ClaimOpts,
) -> Result<EditPlan> {
    let item = resolve(snap, target)?;
    let handle = target.handle();
    if item.source() == Source::Done {
        return Err(archived_error(snap, item, &handle));
    }
    check_item_claim(snap, actor, item, &handle, opts)?;
    let before = snap
        .file(Source::Todo)
        .lines
        .iter()
        .map(|line| line.raw().to_vec())
        .collect();
    let given = target.given().map_or_else(|| handle.clone(), str::to_owned);
    Ok(EditPlan {
        given,
        handle,
        index: item.nr().index(),
        raw: snap.line(item).raw().to_vec(),
        before,
    })
}

/// The text of an edited file: its lines without the whitespace around
/// them, the empty ones left out, joined with single spaces.
pub fn joined_lines(data: &[u8]) -> Vec<u8> {
    let lines: Vec<&[u8]> = data
        .split(|b| *b == b'\n')
        .map(trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines.join(&b' ')
}

/// Gives the planned item its new text, when the todo file still holds
/// the item. The text goes through the rule layer.
///
/// # Errors
///
/// `edit_conflict` when the item changed or is gone, and what the rules
/// refuse: `invalid_value`, `wrong_state` and `claimed`.
pub fn edit(
    env: &Env<'_>,
    plan: &EditPlan,
    text: &[u8],
    opts: ClaimOpts,
) -> Result<Outcome> {
    let text = edit_line(text, "edit", env.clock.today())?;
    run(env, "edit", false, |tx| {
        let id = followed(tx, plan)?;
        tx.mark_primary(id)?;
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Guarded,
            opts,
        };
        tx.guarded(guard, id, |tx| edit_item(tx, env, id, text))
    })
}

/// The item of the plan in the lines now on disk.
fn followed(tx: &mut Tx, plan: &EditPlan) -> Result<ItemId> {
    let now: Vec<&[u8]> = tx
        .snapshot()
        .file(Source::Todo)
        .lines
        .iter()
        .map(Line::raw)
        .collect();
    let at = follow(&plan.before, &now, plan.index)
        .ok_or_else(|| conflict(plan))?;
    let target = Target::todo_index(at).written(&plan.given);
    match tx.target(&target)? {
        Picked::Item(id) => Ok(id),
        Picked::Gone(_) => Err(conflict(plan)),
    }
}

fn conflict(plan: &EditPlan) -> Error {
    Error::new(
        Code::EditConflict,
        format!("{} changed while it was being edited", plan.handle),
    )
}

fn edit_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    raw: Vec<u8>,
) -> Result<()> {
    let old = tx.line_of(id)?;
    let unchanged = raw == old.raw();
    let handle = tx.handle(id);
    if unchanged {
        tx.say(format!("{handle} unchanged"));
        return Ok(());
    }
    let raw = tx.set_edited_on(&env.actor, id, raw, Path::Typed)?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    Ok(())
}
