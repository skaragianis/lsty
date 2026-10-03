//! `do`: complete items.

use super::claim::{ClaimMode, Guard};
use super::env::{Actor, Env};
use super::lifecycle::{
    DoOpts, is_pri_tag, refuse_blocked, refuse_unstarted, say_was_blocked,
};
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::spawn::{Pending, say_nexts, spawn_next};
use super::staged::Path;
use super::tokens::is_key;
use super::tx::Tx;
use crate::errs::Result;
use crate::list::{ItemId, State};
use crate::todotxt::Line;

/// Completes each target. An item of the done file is accepted, and is a
/// no-op.
///
/// # Errors
///
/// `claimed` for an agent on another actor's claim without `takeover`,
/// `wrong_state` for an agent that has not started the item, `blocked`
/// for an item that waits for another unless `ignore_blocked`, and
/// whatever the rules refuse.
pub fn finish(
    env: &Env<'_>,
    targets: &[Target],
    opts: DoOpts,
) -> Result<Outcome> {
    let targets: Vec<Target> =
        targets.iter().cloned().map(Target::archived).collect();
    // The done file is locked for writing: completing can move the item
    // there once archiving is applied.
    run(env, "do", true, |tx| {
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Ends,
            opts: opts.claim,
        };
        let mut pending = Vec::new();
        let done = tx.each_guarded(&targets, guard, |tx, id| {
            finish_item(tx, env, id, opts, &mut pending)
        });
        say_nexts(tx, pending);
        done
    })
}

fn finish_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    opts: DoOpts,
    pending: &mut Vec<Pending>,
) -> Result<()> {
    let handle = tx.handle(id);
    if tx.item(id).map(crate::list::Item::state) == Some(State::Done) {
        tx.say(format!("{handle} is already done"));
        return Ok(());
    }
    refuse_unstarted(tx, &env.actor, id)?;
    if !opts.ignore_blocked {
        refuse_blocked(tx, id)?;
    }
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    let done = completion(tx, env, id, &line)?;
    tx.say([handle.as_bytes(), b" done: ", &line.text()].concat());
    tx.set(id, done.raw(), Path::Action);
    spawn_next(tx, env, id, pending)?;
    if opts.ignore_blocked {
        say_was_blocked(tx, id);
    }
    Ok(())
}

/// `line` completed: the mark and the date in front, the creation date
/// after them, and the priority as a `pri:` tag when it is kept.
fn completion(
    tx: &Tx,
    env: &Env<'_>,
    id: ItemId,
    line: &Line,
) -> Result<Line> {
    let today = env.clock.today();
    let mut head = format!("x {today} ").into_bytes();
    if let Some(created) = line.created() {
        head.extend_from_slice(created.as_bytes());
        head.push(b' ');
    }
    head.extend_from_slice(line.description());
    let done = finished_by(tx, &env.actor, id, line, Line::parse(head))?;
    let Some(letter) = line.priority().filter(|_| env.config.keep_priority)
    else {
        return Ok(done);
    };
    let (bare, _) = tx.remove_where(id, &done, is_pri_tag)?;
    let tag = format!("pri:{letter}");
    tx.append_tok(id, &bare, tag.as_bytes())
}

/// `done` with the `owner:` of whoever finished the item: kept when it is
/// the actor's, otherwise dropped and, for an agent, replaced.
fn finished_by(
    tx: &Tx,
    actor: &Actor,
    id: ItemId,
    line: &Line,
    done: Line,
) -> Result<Line> {
    let owner = line.tags().owner.unwrap_or_default();
    let name = actor.name();
    if owner == name.as_bytes() {
        return Ok(done);
    }
    let (bare, _) = tx.remove_where(id, &done, |t| is_key(t, b"owner"))?;
    if actor.is_human() {
        return Ok(bare);
    }
    tx.append_tok(id, &bare, format!("owner:{name}").as_bytes())
}
