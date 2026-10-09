//! `next`: the item an agent should work on. Its own underway item comes
//! first, so that a restarted agent resumes; otherwise the first ready
//! item. With a claim, the choice and the claim happen under one lock.

use super::env::{Actor, Env};
use super::ids::Taken;
use super::lifecycle::with_claim;
use super::result::Outcome;
use super::run::run;
use super::staged::Path;
use super::tx::Tx;
use crate::errs::{Ref, Result, Source};
use crate::list::{Item, ItemId, Snapshot, SortField, State};
use crate::query::{Context, Query};

/// What the picked item is told when nothing is ready.
const NOTHING_READY: &str = "nothing ready";

/// The flags of `next`.
#[derive(Clone, Debug)]
pub struct NextParams {
    /// Only items the filter matches are chosen.
    pub filter: Query,
    /// How the candidates are ordered; the first is chosen.
    pub sort: SortField,
}

impl Default for NextParams {
    fn default() -> Self {
        Self {
            filter: Query::default(),
            sort: SortField::Priority,
        }
    }
}

/// What a read of `next` found.
#[derive(Debug)]
pub struct Reading {
    /// The lists as read, which `item` points into.
    pub snapshot: Snapshot,
    /// The chosen item, if there is one.
    pub item: Option<ItemId>,
    /// Underway for another agent, stale and matching the filter, in
    /// file order.
    pub stale: Vec<Ref>,
    /// Items matching the filter that are skipped because they are
    /// later.
    pub later: usize,
    /// The item is already the actor's own claim.
    pub resumed: bool,
    /// What to tell the caller.
    pub messages: Vec<Vec<u8>>,
}

/// What `next` with a claim did.
#[derive(Debug)]
pub struct Started {
    /// What was written; its items are the claimed item, if there was one.
    pub outcome: Outcome,
    /// As [`Reading::stale`].
    pub stale: Vec<Ref>,
    /// As [`Reading::later`].
    pub later: usize,
    /// As [`Reading::resumed`]: no claim was added, though an id may
    /// have been.
    pub resumed: bool,
}

/// The choice: an item, and what was seen on the way.
#[derive(Debug, Default)]
struct Chosen {
    item: Option<ItemId>,
    /// The item is the actor's own claim.
    own: bool,
    /// The own claim does not match the filter.
    mismatch: bool,
    stale: Vec<Ref>,
    later: usize,
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
    let item = chosen.item.and_then(|id| snapshot.item(id));
    match item {
        Some(item) => messages.extend(notes(&snapshot, item, &chosen)),
        None => messages.push(NOTHING_READY.as_bytes().to_vec()),
    }
    messages.extend(stale_note(chosen.stale.len()));
    messages.extend(
        snapshot
            .warnings_for(item)
            .into_iter()
            .map(String::into_bytes),
    );
    Ok(Reading {
        resumed: item.is_some() && chosen.own,
        item: item.map(Item::item_id),
        stale: chosen.stale,
        later: chosen.later,
        messages,
        snapshot,
    })
}

/// Chooses and claims under one exclusive lock. The caller's own claim is
/// returned again without a change; an item without an id gets one.
///
/// # Errors
///
/// Whatever the operation refuses with.
pub fn next_start(env: &Env<'_>, params: &NextParams) -> Result<Started> {
    let mut seen = Chosen::default();
    let outcome = run(env, "next", false, |tx| {
        let chosen = choose(tx.snapshot(), env, params);
        let planned = plan(tx, env, &chosen);
        seen = chosen;
        planned
    })?;
    Ok(Started {
        outcome,
        stale: seen.stale,
        later: seen.later,
        resumed: seen.item.is_some() && seen.own,
    })
}

fn plan(tx: &mut Tx, env: &Env<'_>, chosen: &Chosen) -> Result<()> {
    match chosen.item {
        None => tx.say(NOTHING_READY),
        Some(id) => {
            tx.mark_primary(id);
            claim(tx, env, chosen, id)?;
        }
    }
    if let Some(note) = stale_note(chosen.stale.len()) {
        tx.say(note);
    }
    Ok(())
}

fn claim(
    tx: &mut Tx,
    env: &Env<'_>,
    chosen: &Chosen,
    id: ItemId,
) -> Result<()> {
    let Some(mut line) = tx.line(id).cloned() else {
        return Err(vanished());
    };
    let text = line.text();
    let Some(item) = tx.item(id) else {
        return Err(vanished());
    };
    let (nr, has_id) = (item.nr(), tx.snapshot().ref_of(item).id.is_some());
    let mut handle = handle_of(tx.snapshot(), item);
    if !has_id {
        let fresh =
            Taken::new([]).draw(tx.snapshot(), || tx.edited_ids(), env.ids)?;
        line = tx.append_tok(id, &line, format!("id:{fresh}").as_bytes())?;
        tx.say(format!("{nr} is id:{fresh}"));
        handle = fresh;
    }
    if !chosen.own {
        line = with_claim(tx, id, &line, &env.actor, &env.clock.stamp())?;
        let said = [
            format!("{handle} started by {}: ", env.actor.name()).as_bytes(),
            &text,
        ]
        .concat();
        tx.say(said);
    }
    tx.set(id, line.raw(), Path::Action);
    let notes = match tx.item(id) {
        Some(item) => self_notes(tx.snapshot(), item, chosen, &handle),
        None => Vec::new(),
    };
    for note in notes {
        tx.say(note);
    }
    Ok(())
}

/// The chosen item is not in the transaction it was chosen from.
fn vanished() -> crate::errs::Error {
    crate::errs::Error::new(
        crate::errs::Code::Io,
        "the chosen item is missing from the list",
    )
}

/// The id of the item when it has one, else its number.
fn handle_of(snap: &Snapshot, item: &Item) -> String {
    snap.ref_of(item)
        .id
        .unwrap_or_else(|| item.nr().to_string())
}

fn notes(snap: &Snapshot, item: &Item, chosen: &Chosen) -> Vec<Vec<u8>> {
    self_notes(snap, item, chosen, &handle_of(snap, item))
}

/// What is said about an item that is the caller's own claim.
fn self_notes(
    snap: &Snapshot,
    item: &Item,
    chosen: &Chosen,
    handle: &str,
) -> Vec<Vec<u8>> {
    if !chosen.own {
        return Vec::new();
    }
    let text = snap.line(item).text();
    let mut notes =
        vec![[format!("{handle} resumed: ").as_bytes(), &text].concat()];
    if item.blocked() {
        notes.push(blocked_note(snap, item, handle));
    }
    if item.later() {
        let until = snap.tags(item).threshold.unwrap_or_default();
        notes.push(format!("{handle} starts on {until}").into_bytes());
    }
    if chosen.mismatch {
        notes.push(
            format!("{handle} no longer matches the filter").into_bytes(),
        );
    }
    notes
}

fn blocked_note(snap: &Snapshot, item: &Item, handle: &str) -> Vec<u8> {
    let parts: Vec<Vec<u8>> = item
        .blockers()
        .iter()
        .filter_map(|b| snap.item(*b))
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
    own: Option<&'a Item>,
    later: usize,
    stale: Vec<&'a Item>,
    ready: Vec<&'a Item>,
}

fn choose(snap: &Snapshot, env: &Env<'_>, params: &NextParams) -> Chosen {
    let mut scan = Scan {
        actor: &env.actor,
        params,
        ctx: Context::from_clock(env.clock),
        own: None,
        later: 0,
        stale: Vec::new(),
        ready: Vec::new(),
    };
    for item in snap.in_source(Source::Todo) {
        scan.visit(snap, item);
    }
    scan.finish(snap)
}

impl<'a> Scan<'a> {
    fn visit(&mut self, snap: &'a Snapshot, item: &'a Item) {
        if is_own_claim(snap, self.actor, item)
            && self.own.is_none_or(|held| older(item, held))
        {
            self.own = Some(item);
        }
        if !self.params.filter.matches(snap, item, &self.ctx) {
            return;
        }
        if item.later() {
            self.later += 1;
        }
        if item.stale() && owner_of(snap, item) != self.actor.name().as_bytes()
        {
            self.stale.push(item);
        }
        if item.state() == State::Todo && !item.blocked() && !item.later() {
            self.ready.push(item);
        }
    }

    fn finish(mut self, snap: &'a Snapshot) -> Chosen {
        let stale = self.stale.iter().map(|i| snap.ref_of(i)).collect();
        let mut chosen = Chosen {
            stale,
            later: self.later,
            ..Chosen::default()
        };
        if let Some(own) = self.own {
            chosen.item = Some(own.item_id());
            chosen.own = true;
            chosen.mismatch =
                !self.params.filter.matches(snap, own, &self.ctx);
        } else {
            snap.sort(&mut self.ready, self.params.sort);
            chosen.item = self.ready.first().map(|i| i.item_id());
        }
        chosen
    }
}

fn owner_of<'s>(snap: &'s Snapshot, item: &Item) -> &'s [u8] {
    snap.tags(item).owner.unwrap_or_default()
}

fn is_own_claim(snap: &Snapshot, actor: &Actor, item: &Item) -> bool {
    !actor.is_human()
        && item.state() == State::Doing
        && owner_of(snap, item) == actor.name().as_bytes()
}

/// Whether `a` was started before `b`; a bad `started:` is the oldest and
/// ties keep file order.
fn older(a: &Item, b: &Item) -> bool {
    match (a.started(), b.started()) {
        (None, other) => other.is_some(),
        (Some(_), None) => false,
        (Some(x), Some(y)) => x < y,
    }
}
