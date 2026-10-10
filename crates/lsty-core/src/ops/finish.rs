//! `do`: complete items.

use super::claim::{ClaimMode, Guard};
use super::env::{Actor, Env};
use super::lifecycle::{
    DoOpts, is_pri_tag, refuse_blocked, refuse_unstarted, say_was_blocked,
};
use super::refs::{Target, find};
use super::result::Outcome;
use super::run::run;
use super::spawn::{Pending, say_nexts, spawn_next};
use super::staged::Path;
use super::tx::Tx;
use crate::errs::{Result, Source};
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
        let archive = opts.archive.resolve(env.config.auto_archive)
            && !spawns_from_done(tx, &targets);
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Ends,
            opts: opts.claim,
        };
        let mut pending = Vec::new();
        let done = tx.each_guarded(&targets, guard, |tx, id| {
            finish_item(tx, env, id, (opts, archive), &mut pending)
        });
        say_nexts(tx, pending);
        done
    })
}

fn finish_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    (opts, archive): (DoOpts, bool),
    pending: &mut Vec<Pending>,
) -> Result<()> {
    let handle = tx.handle(id);
    let Some(item) = tx.item(id) else {
        return Ok(());
    };
    let (state, source) = (item.state(), item.source());
    let line = tx.snapshot().line(item).clone();
    if state == State::Done {
        tx.say(format!("{handle} is already done"));
        return Ok(());
    }
    refuse_unstarted(tx, &env.actor, id)?;
    if !opts.ignore_blocked {
        refuse_blocked(tx, id)?;
    }
    let done = completion(tx, env, id, &line)?;
    tx.say([handle.as_bytes(), b" done: ", &line.text()].concat());
    let moves = archive && source == Source::Todo;
    if moves {
        archive_done(tx, id, done.raw());
    } else {
        tx.set(id, done.raw(), Path::Action);
    }
    spawn_next(tx, env, id, pending)?;
    if opts.ignore_blocked {
        say_was_blocked(tx, id);
    }
    Ok(())
}

/// Whether a target is an open line of the done file that spawns. The
/// run then completes todo items in place.
fn spawns_from_done(tx: &Tx, targets: &[Target]) -> bool {
    targets.iter().any(|target| {
        let Ok(Some(item)) = find(tx.snapshot(), target) else {
            return false;
        };
        item.source() == Source::Done
            && item.state() != State::Done
            && tx.snapshot().tags(item).rec.is_some()
    })
}

/// Puts the completed line of the item `id` into the done file and blanks
/// the todo line.
fn archive_done(tx: &mut Tx, id: ItemId, done: &[u8]) {
    tx.dest(Source::Done);
    let Some((line, over)) = tx.send_to_done(id, done) else {
        return;
    };
    let (name, nr) = (tx.done_name(), tx.written_nr(line));
    tx.say(format!("archived to {name} as done:{nr}"));
    if over && let Some(key) = tx.id_bytes(id) {
        tx.say_interrupted(&key);
    }
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
    let (bare, _) = tx.remove_where(id, &done, |t| t.is_tag(b"owner"))?;
    if actor.is_human() {
        return Ok(bare);
    }
    tx.append_tok(id, &bare, format!("owner:{name}").as_bytes())
}
