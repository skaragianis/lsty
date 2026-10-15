//! What `start`, `stop`, `do` and `reopen` share: their flags, the blocked
//! check and the edits of a line's claim and priority tokens.

use super::{
    env::{Actor, Toggle},
    rules::blocked_error,
    tokens::is_claim_tag,
    tx::{Tx, gone, wrong_state},
};
use crate::{
    errs::{Error, Result, lossy},
    list::{State, Task, TaskId},
    todotxt::{Line, TokenView},
};

/// The flags that decide what happens to a claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaimOpts {
    /// Take the task over from whoever holds it.
    pub takeover: bool,
}

/// The flags of `start`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartOpts {
    /// The claim flags.
    pub claim: ClaimOpts,
    /// Start a task that waits for another.
    pub ignore_blocked: bool,
}

/// The flags of `do`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoOpts {
    /// The claim flags.
    pub claim: ClaimOpts,
    /// Complete a task that waits for another.
    pub ignore_blocked: bool,
    /// `--archive` or `--no-archive`.
    pub archive: Toggle,
}

const BLOCKED_HINT: &str =
    "finish what it waits for first, or use --ignore-blocked";

/// Whether `tok` is a `pri:` tag.
pub(super) fn is_pri_tag(tok: &TokenView<'_>) -> bool {
    tok.is_tag(b"pri")
}

/// Whether `new` has no description where `old` had one.
pub(super) fn hollowed(old: &Line, new: &Line) -> bool {
    old.has_text() && !new.has_text()
}

/// The refusal of a change that would leave a task with no description.
pub(super) fn emptied(tx: &Tx, id: TaskId) -> Error {
    let handle = tx.handle(id);
    let Some(task) = tx.task(id) else {
        return gone();
    };
    wrong_state(
        tx.snapshot(),
        task,
        task.state(),
        format!("{handle} would be empty"),
        format!("use lsty del {handle}"),
    )
}

/// Refuses a task that waits for another open task.
pub(super) fn refuse_blocked(tx: &Tx, id: TaskId, task: &Task) -> Result<()> {
    if !task.blocked() {
        return Ok(());
    }
    let snap = tx.snapshot();
    let mut named: Vec<Vec<u8>> = Vec::new();
    for b in task.blockers().iter().filter_map(|b| snap.task(*b)) {
        let text = snap.line(b).text();
        named.push(
            [b.nr().to_string().as_bytes(), b" (", &text, b")"].concat(),
        );
    }
    let mut message = format!(
        "task {} is blocked by {}",
        tx.handle(id),
        lossy(&named.join(&b", "[..]))
    );
    if task.cycle() {
        message.push_str(" (dependency cycle)");
    }
    Err(blocked_error(snap, task, &message, BLOCKED_HINT))
}

/// `line` with a new claim for `actor`: `started:` at `stamp`, and
/// `owner:` when the actor is an agent. Any `ended:` is dropped.
pub(super) fn with_claim(
    tx: &Tx,
    id: TaskId,
    line: &Line,
    actor: &Actor,
    stamp: &str,
) -> Result<Line> {
    let (bare, _) = tx.remove_where(id, line, is_claim_tag)?;
    let started =
        tx.append_tok(id, &bare, format!("started:{stamp}").as_bytes())?;
    let Actor::Agent(owner) = actor else {
        return Ok(started);
    };
    tx.append_tok(id, &started, format!("owner:{}", owner.as_str()).as_bytes())
}

/// `line` with its `ended:` replaced by one at `stamp`.
pub(super) fn with_end(
    tx: &Tx,
    id: TaskId,
    line: &Line,
    stamp: &str,
) -> Result<Line> {
    let (bare, _) = tx.remove_where(id, line, |t| t.is_tag(b"ended"))?;
    tx.append_tok(id, &bare, format!("ended:{stamp}").as_bytes())
}

/// Refuses an agent that completes a task it has not started.
pub(super) fn refuse_unstarted(
    tx: &Tx,
    actor: &Actor,
    id: TaskId,
    task: &Task,
) -> Result<()> {
    if actor.is_human() || task.state() == State::Doing {
        return Ok(());
    }
    let handle = tx.handle(id);
    Err(wrong_state(
        tx.snapshot(),
        task,
        task.state(),
        format!("{handle} is not started"),
        format!("start it first: lsty start {}", tx.given(id)),
    ))
}

/// Says which tasks `id` was blocked by, if it is blocked.
pub(super) fn say_was_blocked(tx: &mut Tx, id: TaskId) {
    let Some(task) = tx.task(id).filter(|i| i.blocked()) else {
        return;
    };
    let nrs: Vec<String> = task
        .blockers()
        .iter()
        .filter_map(|b| tx.task(*b))
        .map(|b| b.nr().to_string())
        .collect();
    let message =
        format!("{} was blocked by {}", tx.handle(id), nrs.join(", "));
    tx.say(message);
}
