//! `do`: complete tasks.

use std::sync::Arc;

use super::{
    claim::{ClaimMode, Guard},
    env::{Actor, Env},
    ids::Taken,
    lifecycle::{
        DoOpts, is_pri_tag, refuse_blocked, refuse_unstarted, say_was_blocked,
        with_end,
    },
    refs::{Target, find},
    result::Outcome,
    run::run,
    spawn::{Pending, say_nexts, spawn_next},
    staged::Path,
    tx::{Tx, gone},
};
use crate::{
    errs::{Result, Source},
    list::{State, TaskId},
    todotxt::{Line, Parts, build},
};

/// Completes each target. A task of the done file is accepted, and is a
/// no-op.
///
/// # Errors
///
/// `claimed` for an agent on another actor's claim without `takeover`,
/// `wrong_state` for an agent that has not started the task, `blocked`
/// for a task that waits for another unless `ignore_blocked`, and
/// whatever the rules refuse.
pub fn finish(
    env: &Env<'_>,
    targets: &[Target],
    opts: DoOpts,
) -> Result<Outcome> {
    let targets: Vec<Target> =
        targets.iter().cloned().map(Target::archived).collect();
    // The done file is locked for writing: completing can move the task
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
        let mut taken = Taken::new(tx.edited_ids());
        let done = tx.each_guarded(&targets, guard, |tx, id| {
            finish_task(
                tx,
                env,
                id,
                (opts, archive),
                (&mut pending, &mut taken),
            )
        });
        say_nexts(tx, pending);
        done
    })
}

fn finish_task(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    (opts, archive): (DoOpts, bool),
    (pending, taken): (&mut Vec<Pending>, &mut Taken),
) -> Result<()> {
    let handle = tx.handle(id);
    let task = tx.task(id).ok_or_else(gone)?;
    let (state, source) = (task.state(), task.source());
    let line = tx.snapshot().line(task).clone();
    if state == State::Done {
        tx.say(format!("{handle} is already done"));
        return Ok(());
    }
    refuse_unstarted(tx, &env.actor, id, task)?;
    if !opts.ignore_blocked {
        refuse_blocked(tx, id, task)?;
    }
    let done = completion(tx, env, id, &line)?;
    tx.say([handle.as_bytes(), b" done: ", &line.text()].concat());
    if archive && source == Source::Todo {
        archive_done(tx, id, done.shared())?;
    } else {
        tx.set(id, done.raw(), Path::Action);
    }
    spawn_next(tx, env, id, pending, taken)?;
    if opts.ignore_blocked {
        say_was_blocked(tx, id);
    }
    Ok(())
}

/// Whether a target is an open line of the done file that spawns. The
/// run then completes todo tasks in place.
fn spawns_from_done(tx: &Tx, targets: &[Target]) -> bool {
    targets.iter().any(|target| {
        let Ok(Some(task)) = find(tx.snapshot(), target) else {
            return false;
        };
        task.source() == Source::Done
            && task.state() != State::Done
            && tx.snapshot().tags(task).rec.is_some()
    })
}

/// Puts the completed line of the task `id` into the done file and blanks
/// the todo line.
fn archive_done(tx: &mut Tx, id: TaskId, done: Arc<[u8]>) -> Result<()> {
    tx.dest = Some(Source::Done);
    let (line, over) = tx.send_to(Source::Done, id, done)?;
    let (name, nr) = (tx.file_name(Source::Done), tx.written_nr(line));
    tx.say(format!(
        "archived to {name} as {}",
        Source::Done.numbered(nr)
    ));
    if over && let Some(key) = tx.id_bytes(id) {
        tx.say_interrupted(&key);
    }
    Ok(())
}

/// `line` completed: the mark and the date in front, the creation date
/// after them, and the priority as a `pri:` tag when it is kept.
fn completion(
    tx: &Tx,
    env: &Env<'_>,
    id: TaskId,
    line: &Line,
) -> Result<Line> {
    let today = env.clock.today().to_string();
    let head = Line::parse(build(&Parts {
        done: true,
        completed: Some(&today),
        created: line.created(),
        description: line.description(),
        ..Parts::default()
    }));
    let mut done = finished_by(tx, &env.actor, id, line, head)?;
    if line.tags().underway {
        done = with_end(tx, id, &done, &env.clock.stamp())?;
    }
    let Some(priority) = line.priority().filter(|_| env.config.keep_priority)
    else {
        return Ok(done);
    };
    let (bare, _) = tx.remove_where(id, &done, is_pri_tag)?;
    let tag = format!("pri:{priority}");
    tx.append_tok(id, &bare, tag.as_bytes())
}

/// `done` with the `owner:` of whoever finished the task: kept when it is
/// the actor's, otherwise dropped and, for an agent, replaced.
fn finished_by(
    tx: &Tx,
    actor: &Actor,
    id: TaskId,
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
