//! `reopen`: un-complete done items.

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
    list::{ItemId, State},
    todotxt::Line,
};

/// Un-completes each done target of the todo file. There is no blocked
/// check.
///
/// # Errors
///
/// `wrong_state` for an item of the done file, and for an item that would
/// be left without a description.
pub fn reopen(
    env: &Env<'_>,
    targets: &[Target],
    opts: ClaimOpts,
) -> Result<Outcome> {
    let stamp = env.clock.stamp();
    run(env, "reopen", false, |tx| {
        tx.each_item(targets, |tx, id| reopen_item(tx, env, id, &stamp, opts))
    })
}

fn reopen_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    stamp: &str,
    opts: ClaimOpts,
) -> Result<()> {
    let handle = tx.handle(id);
    let item = tx.item(id).ok_or_else(gone)?;
    let (state, line) = (item.state(), tx.snapshot().line(item).clone());
    if state != State::Done {
        tx.say(format!("{handle} is already open"));
        return Ok(());
    }
    let open = reopened(tx, id, &line);
    if hollowed(&line, &open) {
        return Err(emptied(tx, id));
    }
    let (open, back) = unfinished(tx, &env.actor, id, open, (stamp, opts))?;
    tx.set(id, open.raw(), Path::Action);
    tx.say([handle.as_bytes(), b" reopened", &back].concat());
    if line.tags().rec.is_some() {
        tx.say(format!(
            "{handle} has rec:; its next occurrence stays in the list"
        ));
    }
    Ok(())
}

/// `open`, the line of the item `id` without its mark, taken over when
/// the options say so, and what it says about whose claim it is back to.
pub(super) fn unfinished(
    tx: &Tx,
    actor: &Actor,
    id: ItemId,
    open: Line,
    (stamp, opts): (&str, ClaimOpts),
) -> Result<(Line, Vec<u8>)> {
    let back = back_to(actor, tx.line_of(id)?, opts);
    if !opts.takeover {
        return Ok((open, back));
    }
    Ok((with_claim(tx, id, &open, actor, stamp)?, back))
}

/// What a reopened item says about whose claim it is back to.
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
/// and its `pri:` tags gone (all kept when removing them would change a
/// stamp).
pub(super) fn reopened(tx: &Tx, id: ItemId, line: &Line) -> Line {
    let mut head = Vec::new();
    if let Some(letter) = restored_priority(line) {
        head.extend_from_slice(
            format!("({}) ", char::from(letter)).as_bytes(),
        );
    }
    if let Some(created) = line.created() {
        head.extend_from_slice(created.as_bytes());
        head.push(b' ');
    }
    head.extend_from_slice(line.description());
    let open = Line::parse(head);
    match tx.remove_where(id, &open, is_pri_tag) {
        Ok((bare, _)) => bare,
        Err(_) => open,
    }
}

/// The letter of the `(X)` field, or else of the first valid `pri:`.
pub(super) fn restored_priority(line: &Line) -> Option<u8> {
    if let Some(p) = line.priority() {
        return Some(p.letter());
    }
    line.tokens()
        .filter(is_pri_tag)
        .find_map(|t| match t.value {
            [letter @ b'A'..=b'Z'] => Some(*letter),
            _ => None,
        })
}
