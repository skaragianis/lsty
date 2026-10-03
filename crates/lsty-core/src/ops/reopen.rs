//! `reopen`: un-complete done items.

use super::env::{Actor, Env};
use super::lifecycle::{ClaimOpts, emptied, hollowed, is_pri_tag, with_claim};
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::Tx;
use crate::errs::Result;
use crate::list::{ItemId, State};
use crate::todotxt::{Kind, Line};

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
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    if tx.item(id).map(crate::list::Item::state) != Some(State::Done) {
        tx.say(format!("{handle} is already open"));
        return Ok(());
    }
    let open = reopened(tx, id, &line);
    if hollowed(&line, &open) {
        return Err(emptied(tx, id));
    }
    let open = if opts.takeover {
        with_claim(tx, id, &open, &env.actor, stamp)?
    } else {
        open
    };
    tx.set(id, open.raw(), Path::Action);
    let back = back_to(&env.actor, &line, opts);
    tx.say([handle.as_bytes(), b" reopened", &back].concat());
    Ok(())
}

/// What a reopened item says about whose claim it is back to.
fn back_to(actor: &Actor, line: &Line, opts: ClaimOpts) -> Vec<u8> {
    let tags = line.tags();
    match tags.owner.filter(|o| !o.is_empty()) {
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
fn reopened(tx: &Tx, id: ItemId, line: &Line) -> Line {
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
fn restored_priority(line: &Line) -> Option<u8> {
    if let Some(p) = line.priority() {
        return Some(p.letter());
    }
    line.tokens()
        .filter(|t| t.kind == Kind::Tag && t.key == b"pri")
        .find_map(|t| match t.value {
            [letter @ b'A'..=b'Z'] => Some(*letter),
            _ => None,
        })
}
