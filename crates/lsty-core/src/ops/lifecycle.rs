//! What `start`, `stop`, `do` and `reopen` share: their flags, the blocked
//! check and the edits of a line's claim and priority tokens.

use super::{
    env::{Actor, Toggle},
    rules::blocked_error,
    tokens::is_claim_tag,
    tx::{Tx, wrong_state},
};
use crate::{
    errs::{Code, Error, Result},
    list::{ItemId, State},
    todotxt::{Line, TokenView},
};

/// The flags that decide what happens to a claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaimOpts {
    /// Take the item over from whoever holds it.
    pub takeover: bool,
}

/// The flags of `start`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartOpts {
    /// The claim flags.
    pub claim: ClaimOpts,
    /// Start an item that waits for another.
    pub ignore_blocked: bool,
}

/// The flags of `do`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoOpts {
    /// The claim flags.
    pub claim: ClaimOpts,
    /// Complete an item that waits for another.
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

/// The refusal of a change that would leave an item with no description.
pub(super) fn emptied(tx: &Tx, id: ItemId) -> Error {
    let handle = tx.handle(id);
    match tx.item(id) {
        Some(item) => wrong_state(
            tx.snapshot(),
            item,
            item.state(),
            format!("{handle} would be empty"),
            format!("use lsty del {handle}"),
        ),
        None => {
            Error::new(Code::WrongState, format!("{handle} would be empty"))
        }
    }
}

/// The refusal to start a done item.
pub(super) fn is_done(tx: &Tx, id: ItemId) -> Error {
    let handle = tx.handle(id);
    match tx.item(id) {
        Some(item) => wrong_state(
            tx.snapshot(),
            item,
            State::Done,
            format!("{handle} is done"),
            format!("use lsty reopen {handle} first"),
        ),
        None => Error::new(Code::WrongState, format!("{handle} is done")),
    }
}

/// Refuses an item that waits for another open item.
pub(super) fn refuse_blocked(tx: &Tx, id: ItemId) -> Result<()> {
    let Some(item) = tx.item(id).filter(|i| i.blocked()) else {
        return Ok(());
    };
    let snap = tx.snapshot();
    let mut named: Vec<Vec<u8>> = Vec::new();
    for b in item.blockers().iter().filter_map(|b| snap.item(*b)) {
        let text = snap.line(b).text();
        named.push(
            [b.nr().to_string().as_bytes(), b" (", &text, b")"].concat(),
        );
    }
    let mut message = format!(
        "item {} is blocked by {}",
        tx.handle(id),
        String::from_utf8_lossy(&named.join(&b", "[..]))
    );
    if item.cycle() {
        message.push_str(" (dependency cycle)");
    }
    Err(blocked_error(snap, item, &message, BLOCKED_HINT))
}

/// `line` with a new claim for `actor`: `started:` at `stamp`, and
/// `owner:` when the actor is an agent.
pub(super) fn with_claim(
    tx: &Tx,
    id: ItemId,
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

/// Refuses an agent that completes an item it has not started.
pub(super) fn refuse_unstarted(
    tx: &Tx,
    actor: &Actor,
    id: ItemId,
) -> Result<()> {
    let Some(item) = tx.item(id) else {
        return Ok(());
    };
    if actor.is_human() || item.state() == State::Doing {
        return Ok(());
    }
    let handle = tx.handle(id);
    Err(wrong_state(
        tx.snapshot(),
        item,
        item.state(),
        format!("{handle} is not started"),
        format!("start it first: lsty start {}", tx.given(id)),
    ))
}

/// Says which items `id` was blocked by, if it is blocked.
pub(super) fn say_was_blocked(tx: &mut Tx, id: ItemId) {
    let Some(item) = tx.item(id).filter(|i| i.blocked()) else {
        return;
    };
    let nrs: Vec<String> = item
        .blockers()
        .iter()
        .filter_map(|b| tx.item(*b))
        .map(|b| b.nr().to_string())
        .collect();
    let message =
        format!("{} was blocked by {}", tx.handle(id), nrs.join(", "));
    tx.say(message);
}
