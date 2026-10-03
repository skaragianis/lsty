//! `do`: complete items.

use super::env::Env;
use super::lifecycle::{DoOpts, is_pri_tag, refuse_blocked, say_was_blocked};
use super::refs::Target;
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::Tx;
use crate::errs::Result;
use crate::list::{ItemId, State};
use crate::todotxt::Line;

/// Completes each target. An item of the done file is accepted, and is a
/// no-op.
///
/// # Errors
///
/// `blocked` for an item that waits for another unless `ignore_blocked`,
/// and whatever the rules refuse.
pub fn finish(
    env: &Env<'_>,
    targets: &[Target],
    opts: DoOpts,
) -> Result<Outcome> {
    let targets: Vec<Target> =
        targets.iter().cloned().map(Target::archived).collect();
    let today = env.clock.today().to_string();
    // The done file is locked for writing: completing can move the item
    // there once archiving is applied.
    run(env, "do", true, |tx| {
        tx.each_item(&targets, |tx, id| finish_item(tx, env, id, &today, opts))
    })
}

fn finish_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    today: &str,
    opts: DoOpts,
) -> Result<()> {
    let handle = tx.handle(id);
    if tx.item(id).map(crate::list::Item::state) == Some(State::Done) {
        tx.say(format!("{handle} is already done"));
        return Ok(());
    }
    if !opts.ignore_blocked {
        refuse_blocked(tx, id)?;
    }
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    let done = completion(tx, env, id, &line, today)?;
    tx.say([handle.as_bytes(), b" done: ", &line.text()].concat());
    tx.set(id, done.raw(), Path::Action);
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
    today: &str,
) -> Result<Line> {
    let mut head = format!("x {today} ").into_bytes();
    if let Some(created) = line.created() {
        head.extend_from_slice(created.as_bytes());
        head.push(b' ');
    }
    head.extend_from_slice(line.description());
    let done = Line::parse(head);
    let Some(letter) = line.priority().filter(|_| env.config.keep_priority)
    else {
        return Ok(done);
    };
    let (bare, _) = tx.remove_where(id, &done, is_pri_tag)?;
    let tag = format!("pri:{letter}");
    tx.append_tok(id, &bare, tag.as_bytes())
}
