//! `next`: the task an agent should work on. Its own underway task comes
//! first, so that a restarted agent resumes; otherwise the first ready
//! task. With a claim, the choice and the claim happen under one lock.

use super::{
    claim::owner_of,
    env::{Actor, Env},
    ids::Taken,
    lifecycle::with_claim,
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Tx, gone},
};
use crate::{
    errs::{Ref, Result, Source},
    list::{Snapshot, SortField, State, Task, TaskId},
    query::{Context, Query},
};

/// What the caller is told when nothing is ready: the reasons matching
/// tasks were skipped, in the order blocked, later, claimed.
fn nothing_ready(chosen: &Chosen) -> String {
    let reasons: Vec<String> = [
        (chosen.seen.blocked, "blocked"),
        (chosen.seen.later, "later"),
        (chosen.seen.claimed, "claimed"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}"))
    .collect();
    if reasons.is_empty() {
        "nothing ready".to_owned()
    } else {
        format!("nothing ready ({})", reasons.join(", "))
    }
}

/// The flags of `next`.
#[derive(Clone, Debug)]
pub struct NextParams {
    /// Only tasks the filter matches are chosen.
    pub filter: Query,
    /// How the candidates are ordered; the first is chosen.
    pub sort: SortField,
}

/// What the choice saw on the way, and whether it resumed.
#[derive(Debug, Default)]
pub struct Survey {
    /// Underway for another agent, stale and matching the filter, in
    /// file order.
    pub stale: Vec<Ref>,
    /// Open, unclaimed tasks matching the filter that are skipped because
    /// they start later.
    pub later: usize,
    /// Open, unclaimed tasks matching the filter that are blocked.
    pub blocked: usize,
    /// Tasks matching the filter that are underway for another worker and
    /// not stale.
    pub claimed: usize,
    /// The task is already the actor's own claim: no claim is added,
    /// though an id may be.
    pub resumed: bool,
}

/// What a read of `next` found.
#[derive(Debug)]
pub struct Reading {
    /// The lists as read, which `task` points into.
    pub snapshot: Snapshot,
    /// The chosen task, if there is one.
    pub task: Option<TaskId>,
    /// What was seen on the way.
    pub seen: Survey,
    /// What to tell the caller.
    pub messages: Vec<Vec<u8>>,
}

/// What `next` with a claim did.
#[derive(Debug)]
pub struct Started {
    /// What was written; its tasks are the claimed task, if there was one.
    pub outcome: Outcome,
    /// What was seen on the way.
    pub seen: Survey,
}

/// The choice: a task, and what was seen on the way.
#[derive(Debug)]
struct Chosen {
    task: Option<TaskId>,
    /// The own claim does not match the filter.
    mismatch: bool,
    seen: Survey,
}

/// Reads the lists under shared locks and chooses; nothing is written.
///
/// # Errors
///
/// Whatever reading the lists fails with.
pub fn next_read(env: &Env<'_>, params: &NextParams) -> Result<Reading> {
    let snapshot = env.load()?;
    let chosen = choose(&snapshot, env, params);
    let mut messages = Vec::new();
    let task = chosen.task.and_then(|id| snapshot.task(id));
    match task {
        Some(task) => messages.extend(self_notes(
            &snapshot,
            task,
            &chosen,
            &handle_of(&snapshot, task),
        )),
        None => messages.push(nothing_ready(&chosen).into_bytes()),
    }
    messages.extend(stale_note(chosen.seen.stale.len()));
    messages.extend(filter_notes(&params.filter));
    messages.extend(
        snapshot
            .warnings_for(task)
            .into_iter()
            .map(String::into_bytes),
    );
    Ok(Reading {
        task: task.map(Task::task_id),
        seen: chosen.seen,
        messages,
        snapshot,
    })
}

/// Chooses and claims under one exclusive lock. The caller's own claim is
/// returned again without a change; a task without an id gets one.
///
/// # Errors
///
/// Whatever the operation refuses with.
pub fn next_start(env: &Env<'_>, params: &NextParams) -> Result<Started> {
    let mut seen = Survey::default();
    let outcome = run(env, "next", false, |tx| {
        let chosen = choose(tx.snapshot(), env, params);
        let planned = plan(tx, env, &chosen);
        seen = chosen.seen;
        for note in filter_notes(&params.filter) {
            tx.say(note);
        }
        planned
    })?;
    Ok(Started { outcome, seen })
}

fn plan(tx: &mut Tx, env: &Env<'_>, chosen: &Chosen) -> Result<()> {
    match chosen.task {
        None => tx.say(nothing_ready(chosen)),
        Some(id) => {
            tx.mark_primary(id)?;
            claim(tx, env, chosen, id)?;
        }
    }
    if let Some(note) = stale_note(chosen.seen.stale.len()) {
        tx.say(note);
    }
    Ok(())
}

fn filter_notes(filter: &Query) -> impl Iterator<Item = Vec<u8>> {
    filter.warnings().iter().map(|w| w.clone().into_bytes())
}

fn claim(
    tx: &mut Tx,
    env: &Env<'_>,
    chosen: &Chosen,
    id: TaskId,
) -> Result<()> {
    let task = tx.task(id).ok_or_else(gone)?;
    let snap = tx.snapshot();
    let mut line = snap.line(task).clone();
    let text = line.text();
    let nr = task.nr();
    let fresh = if snap.id_of(task).is_some() {
        None
    } else {
        Some(Taken::new(tx.edited_ids()).draw(snap, env.ids)?)
    };
    let handle = fresh
        .as_ref()
        .map_or_else(|| handle_of(snap, task), ToString::to_string);
    let notes = self_notes(snap, task, chosen, &handle);
    if let Some(fresh) = fresh {
        line = tx.append_tok(id, &line, format!("id:{fresh}").as_bytes())?;
        tx.say(format!("{nr} is id:{fresh}"));
    }
    if !chosen.seen.resumed {
        line = with_claim(tx, id, &line, &env.actor, &env.clock.stamp())?;
        let said = [
            format!("{handle} started by {}: ", env.actor.name()).as_bytes(),
            &text,
        ]
        .concat();
        tx.say(said);
    }
    tx.set(id, line.raw(), Path::Action);
    for note in notes {
        tx.say(note);
    }
    Ok(())
}

/// The id of the task when it has one, else its number.
fn handle_of(snap: &Snapshot, task: &Task) -> String {
    snap.ref_of(task)
        .id
        .unwrap_or_else(|| task.nr().to_string())
}

/// What is said about a task that is the caller's own claim.
fn self_notes(
    snap: &Snapshot,
    task: &Task,
    chosen: &Chosen,
    handle: &str,
) -> Vec<Vec<u8>> {
    if !chosen.seen.resumed {
        return Vec::new();
    }
    let text = snap.line(task).text();
    let mut notes =
        vec![[format!("{handle} resumed: ").as_bytes(), &text].concat()];
    if task.blocked() {
        notes.push(blocked_note(snap, task, handle));
    }
    if task.later() {
        let until = snap.tags(task).threshold.unwrap_or_default();
        notes.push(format!("{handle} starts on {until}").into_bytes());
    }
    if chosen.mismatch {
        notes.push(
            format!("{handle} no longer matches the filter").into_bytes(),
        );
    }
    notes
}

fn blocked_note(snap: &Snapshot, task: &Task, handle: &str) -> Vec<u8> {
    let parts: Vec<Vec<u8>> = task
        .blockers()
        .iter()
        .filter_map(|b| snap.task(*b))
        .map(|b| {
            [
                b.nr().to_string().as_bytes(),
                b" (",
                &snap.line(b).text(),
                b")",
            ]
            .concat()
        })
        .collect();
    [
        format!("{handle} is blocked by ").as_bytes(),
        &parts.join(&b", "[..]),
        b"; work on the blocker or stop the claim",
    ]
    .concat()
}

fn stale_note(count: usize) -> Option<Vec<u8>> {
    (count > 0).then(|| format!("{count} stale claims skipped").into_bytes())
}

/// What a walk over the todo file collects.
struct Scan<'a> {
    actor: &'a Actor,
    params: &'a NextParams,
    ctx: Context<'a>,
    /// The oldest own claim, and whether the filter matches it.
    own: Option<(&'a Task, bool)>,
    seen: Survey,
    ready: Vec<&'a Task>,
}

fn choose(snap: &Snapshot, env: &Env<'_>, params: &NextParams) -> Chosen {
    let mut scan = Scan {
        actor: &env.actor,
        params,
        ctx: Context::from_clock(env.clock),
        own: None,
        seen: Survey::default(),
        ready: Vec::new(),
    };
    for task in snap.in_source(Source::Todo) {
        scan.visit(snap, task);
    }
    scan.finish(snap)
}

impl<'a> Scan<'a> {
    fn visit(&mut self, snap: &'a Snapshot, task: &'a Task) {
        let own = is_own_claim(snap, self.actor, task);
        let matches = self.params.filter.matches(snap, task, &self.ctx);
        if own && self.own.is_none_or(|(held, _)| older(task, held)) {
            self.own = Some((task, matches));
        }
        if !matches {
            return;
        }
        if task.stale() && !own {
            self.seen.stale.push(snap.ref_of(task));
        } else if task.state() == State::Doing && !own {
            self.seen.claimed += 1;
        }
        if task.state() == State::Todo {
            if task.later() {
                self.seen.later += 1;
            }
            if task.blocked() {
                self.seen.blocked += 1;
            } else if !task.later() {
                self.ready.push(task);
            }
        }
    }

    fn finish(self, snap: &'a Snapshot) -> Chosen {
        let Self {
            own,
            mut seen,
            mut ready,
            params,
            ..
        } = self;
        let (task, mismatch) = if let Some((own, matches)) = own {
            seen.resumed = true;
            (Some(own.task_id()), !matches)
        } else {
            snap.sort(&mut ready, params.sort);
            (ready.first().map(|i| i.task_id()), false)
        };
        Chosen {
            task,
            mismatch,
            seen,
        }
    }
}

fn is_own_claim(snap: &Snapshot, actor: &Actor, task: &Task) -> bool {
    !actor.is_human()
        && task.state() == State::Doing
        && owner_of(snap, task) == actor.name().as_bytes()
}

/// Whether `a` was started before `b`; a bad `started:` is the oldest and
/// ties keep file order.
fn older(a: &Task, b: &Task) -> bool {
    match (a.started(), b.started()) {
        (None, other) => other.is_some(),
        (Some(_), None) => false,
        (Some(x), Some(y)) => x < y,
    }
}
