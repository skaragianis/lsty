//! `next`: the item an agent should work on. Its own underway item comes
//! first, so that a restarted agent resumes; otherwise the first ready
//! item. With a claim, the choice and the claim happen under one lock.

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
    list::{Item, ItemId, Snapshot, SortField, State},
    query::{Context, Query},
};

/// What the caller is told when nothing is ready: the reasons matching
/// items were skipped, in the order blocked, later, claimed.
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

/// What the choice saw on the way, and whether it resumed.
#[derive(Debug, Default)]
pub struct Survey {
    /// Underway for another agent, stale and matching the filter, in
    /// file order.
    pub stale: Vec<Ref>,
    /// Open, unclaimed items matching the filter that are skipped because
    /// they start later.
    pub later: usize,
    /// Open, unclaimed items matching the filter that are blocked.
    pub blocked: usize,
    /// Items matching the filter that are underway for another worker and
    /// not stale.
    pub claimed: usize,
    /// The item is already the actor's own claim: no claim is added,
    /// though an id may be.
    pub resumed: bool,
}

/// What a read of `next` found.
#[derive(Debug)]
pub struct Reading {
    /// The lists as read, which `item` points into.
    pub snapshot: Snapshot,
    /// The chosen item, if there is one.
    pub item: Option<ItemId>,
    /// What was seen on the way.
    pub seen: Survey,
    /// What to tell the caller.
    pub messages: Vec<Vec<u8>>,
}

/// What `next` with a claim did.
#[derive(Debug)]
pub struct Started {
    /// What was written; its items are the claimed item, if there was one.
    pub outcome: Outcome,
    /// What was seen on the way.
    pub seen: Survey,
}

/// The choice: an item, and what was seen on the way.
#[derive(Debug)]
struct Chosen {
    item: Option<ItemId>,
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
    let item = chosen.item.and_then(|id| snapshot.item(id));
    match item {
        Some(item) => messages.extend(notes(&snapshot, item, &chosen)),
        None => messages.push(nothing_ready(&chosen).into_bytes()),
    }
    messages.extend(stale_note(chosen.seen.stale.len()));
    messages.extend(filter_notes(&params.filter));
    messages.extend(
        snapshot
            .warnings_for(item)
            .into_iter()
            .map(String::into_bytes),
    );
    Ok(Reading {
        item: item.map(Item::item_id),
        seen: chosen.seen,
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
    match chosen.item {
        None => tx.say(nothing_ready(chosen)),
        Some(id) => {
            tx.mark_primary(id);
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
    id: ItemId,
) -> Result<()> {
    let item = tx.item(id).ok_or_else(gone)?;
    let snap = tx.snapshot();
    let mut line = snap.line(item).clone();
    let text = line.text();
    let nr = item.nr();
    let fresh = if snap.ref_of(item).id.is_some() {
        None
    } else {
        Some(Taken::new([]).draw(snap, || tx.edited_ids(), env.ids)?)
    };
    let handle = fresh
        .as_ref()
        .map_or_else(|| handle_of(snap, item), ToString::to_string);
    let notes = self_notes(snap, item, chosen, &handle);
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
    if !chosen.seen.resumed {
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
    /// The oldest own claim, and whether the filter matches it.
    own: Option<(&'a Item, bool)>,
    seen: Survey,
    ready: Vec<&'a Item>,
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
    for item in snap.in_source(Source::Todo) {
        scan.visit(snap, item);
    }
    scan.finish(snap)
}

impl<'a> Scan<'a> {
    fn visit(&mut self, snap: &'a Snapshot, item: &'a Item) {
        let own = is_own_claim(snap, self.actor, item);
        let matches = self.params.filter.matches(snap, item, &self.ctx);
        if own && self.own.is_none_or(|(held, _)| older(item, held)) {
            self.own = Some((item, matches));
        }
        if !matches {
            return;
        }
        if item.stale() && !own {
            self.seen.stale.push(snap.ref_of(item));
        } else if item.state() == State::Doing && !own {
            self.seen.claimed += 1;
        }
        if item.state() == State::Todo {
            if item.later() {
                self.seen.later += 1;
            }
            if item.blocked() {
                self.seen.blocked += 1;
            } else if !item.later() {
                self.ready.push(item);
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
        let (item, mismatch) = if let Some((own, matches)) = own {
            seen.resumed = true;
            (Some(own.item_id()), !matches)
        } else {
            snap.sort(&mut ready, params.sort);
            (ready.first().map(|i| i.item_id()), false)
        };
        Chosen {
            item,
            mismatch,
            seen,
        }
    }
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
