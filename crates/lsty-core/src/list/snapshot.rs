use std::{
    ops::Range,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use jiff::Timestamp;

use super::ids::{self, Warned};
use crate::{
    config::Config,
    dates::{Clock, format_elapsed},
    discover::Found,
    errs::{Nr, Ref, Source, lossy},
    todotxt::{File, Line, Tags},
};

/// Where an item stands in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Open and not underway.
    Todo,
    /// Open and underway: a `started:` token is present.
    Doing,
    /// Done.
    Done,
}

impl State {
    /// The state as written in output: `todo`, `doing` or `done`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Done => "done",
        }
    }
}

/// Who is working on an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// Nobody: the item is not underway.
    None,
    /// Underway without an `owner:`.
    Human,
    /// Underway with an `owner:`.
    Agent,
}

/// The position of an item in [`Snapshot::items`]. It is only meaningful
/// for the snapshot that gave it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId {
    index: usize,
    serial: u32,
}

impl ItemId {
    pub(super) const fn new(index: usize, serial: u32) -> Self {
        Self { index, serial }
    }

    /// The index into [`Snapshot::items`].
    pub const fn index(self) -> usize {
        self.index
    }
}

/// The serial the next snapshot gets. Every snapshot of the process has
/// its own, so an item used on the wrong snapshot is caught in debug
/// builds.
pub(super) fn next_serial() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A non-blank line with the state derived from the whole list.
///
/// An item keeps no borrow of its line: [`Snapshot::line`] gives the
/// line back, and its tags come from the line. Its fields are read
/// through methods, so what it says always agrees with the list it came
/// from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the flags are independent facts that readers filter on"
)]
pub struct Item {
    pub(super) nr: Nr,
    pub(super) source: Source,
    pub(super) state: State,
    pub(super) claim: Claim,
    pub(super) blockers: Vec<ItemId>,
    pub(super) dangling: Vec<Range<usize>>,
    pub(super) cycle: bool,
    pub(super) later: bool,
    pub(crate) overdue: bool,
    pub(super) stale: bool,
    pub(super) key: ItemId,
    pub(super) id: Option<Range<usize>>,
    pub(super) started: Option<Timestamp>,
}

impl Item {
    /// The line number in its file.
    pub const fn nr(&self) -> Nr {
        self.nr
    }

    /// The file the item lives in.
    pub const fn source(&self) -> Source {
        self.source
    }

    /// How a reference names the item by its line: `12`, or `done:12`
    /// in the done file.
    pub fn handle(&self) -> String {
        self.source.numbered(self.nr)
    }

    /// Todo, doing or done.
    pub const fn state(&self) -> State {
        self.state
    }

    /// Who is working on it.
    pub const fn claim(&self) -> Claim {
        self.claim
    }

    /// How `show` and the detail panel word the claim: `finished by X`,
    /// `finished (by you)`, `claimed (by you)` or `claimed by X`. `owner`
    /// is the line's `owner:` value.
    pub fn claim_text(&self, owner: Option<&[u8]>) -> Option<Vec<u8>> {
        match (self.state == State::Done, self.claim, owner) {
            (true, _, Some(owner)) => Some([b"finished by ", owner].concat()),
            (true, Claim::Human, None) => Some(b"finished (by you)".to_vec()),
            (false, Claim::Human, _) => Some(b"claimed (by you)".to_vec()),
            (false, Claim::Agent, Some(owner)) => {
                Some([b"claimed by ", owner].concat())
            }
            _ => None,
        }
    }

    /// This item's position in [`Snapshot::items`], valid only for the
    /// snapshot that gave the item out.
    pub const fn item_id(&self) -> ItemId {
        self.key
    }

    /// Whether the item is not done.
    pub fn is_open(&self) -> bool {
        self.state != State::Done
    }

    /// Open, and waiting on an open prerequisite or part of a cycle.
    pub fn blocked(&self) -> bool {
        self.is_open() && (self.cycle || !self.blockers.is_empty())
    }

    /// Whether a `dep:` names an id found in neither file.
    pub(crate) fn has_dangling(&self) -> bool {
        !self.dangling.is_empty()
    }

    /// The open prerequisites, once each, in the order of the `dep:`
    /// tokens. Empty for a done item.
    pub fn blockers(&self) -> &[ItemId] {
        &self.blockers
    }

    /// Part of a dependency cycle.
    pub const fn cycle(&self) -> bool {
        self.cycle
    }

    /// The `t:` value as `show` and the detail panel print it, with the
    /// mark of an item that starts later.
    pub fn threshold_text(&self, threshold: &str) -> String {
        if self.later {
            format!("{threshold} (later)")
        } else {
            threshold.to_owned()
        }
    }

    /// The `started:` instant in the clock's zone, as `2026-10-01 09:30`,
    /// and for an underway item the time since it, in parentheses, with
    /// `, stale` inside them when `mark_stale` is set and the item is.
    pub fn started_text(
        &self,
        clock: &Clock,
        mark_stale: bool,
    ) -> Option<String> {
        let at = self.started?.to_zoned(clock.zone().clone());
        let when = at.strftime("%Y-%m-%d %H:%M");
        let Some(elapsed) = self.elapsed(clock.now()) else {
            return Some(when.to_string());
        };
        let stale = if mark_stale && self.stale {
            ", stale"
        } else {
            ""
        };
        Some(format!("{when} ({}{stale})", format_elapsed(elapsed)))
    }

    /// Has a valid `t:` after today, whatever its state.
    pub const fn later(&self) -> bool {
        self.later
    }

    /// Open, claimed by an agent for more than the stale limit.
    pub const fn stale(&self) -> bool {
        self.stale
    }

    /// The instant of the first `started:` token, when it is valid.
    pub const fn started(&self) -> Option<Timestamp> {
        self.started
    }

    /// Now minus `started:` for an underway item with a valid `started:`,
    /// and `None` otherwise. A `started:` in the future gives zero.
    pub fn elapsed(&self, now: Timestamp) -> Option<Duration> {
        if self.state != State::Doing {
            return None;
        }
        let since = now.duration_since(self.started?);
        Some(Duration::try_from(since).unwrap_or(Duration::ZERO))
    }
}

/// What [`Snapshot::build`] needs besides the files.
#[derive(Clone, Debug)]
pub struct Params {
    /// The reading of time for `later`, `overdue` and `stale`.
    pub clock: Clock,
    /// How long an agent may hold a claim before it is stale.
    pub stale_after: Duration,
}

impl Params {
    /// The parameters for reading time from `clock` with the claim
    /// timeout of `config`.
    #[must_use]
    pub fn new(clock: Clock, config: &Config) -> Self {
        Self {
            clock,
            stale_after: config.claim_stale_after,
        }
    }
}

/// Both files as read at one moment, with derived state.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub(super) serial: u32,
    pub(super) found: Found,
    pub(super) todo: File,
    pub(super) done: File,
    pub(super) items: Vec<Item>,
    pub(super) todo_items: usize,
    pub(super) by_id: Vec<usize>,
    /// The values of second and later `id:` tags on a line, sorted.
    pub(super) extra_ids: Vec<Vec<u8>>,
    pub(super) warned: Vec<Warned>,
}

/// The line `line` falls back to; no item of a snapshot points at it.
fn blank() -> &'static Line {
    static BLANK: OnceLock<Line> = OnceLock::new();
    BLANK.get_or_init(|| Line::parse([]))
}

impl Snapshot {
    /// The number that tells this snapshot from every other one of the
    /// process.
    pub const fn serial(&self) -> u32 {
        self.serial
    }

    /// The files the snapshot was read from.
    pub const fn found(&self) -> &Found {
        &self.found
    }

    /// The todo file.
    pub const fn todo(&self) -> &File {
        &self.todo
    }

    /// The done file.
    pub const fn done(&self) -> &File {
        &self.done
    }

    /// The items of the todo file in file order, then those of the done
    /// file.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Ids found in both files or on several lines of one.
    #[cfg(test)]
    pub(super) fn warnings(&self) -> impl ExactSizeIterator<Item = &str> {
        self.warned.iter().map(Warned::text)
    }

    /// Takes the snapshot apart, so a rebuild can reuse the files.
    pub fn into_files(self) -> (Found, File, File) {
        (self.found, self.todo, self.done)
    }

    /// The item behind `id`.
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.check(id);
        self.items.get(id.index())
    }

    /// The item on line `nr` of `source`; `None` for a blank or missing
    /// line.
    pub fn at(&self, source: Source, nr: Nr) -> Option<&Item> {
        let items = self.in_source(source);
        let at = items.binary_search_by_key(&nr, |i| i.nr).ok()?;
        items.get(at)
    }

    /// The items of one file, in file order.
    pub fn in_source(&self, source: Source) -> &[Item] {
        let (todo, done) = self
            .items
            .split_at_checked(self.todo_items)
            .unwrap_or((&self.items, &[]));
        source.pick(todo, done)
    }

    /// The file of `source`.
    pub fn file(&self, source: Source) -> &File {
        source.pick(&self.todo, &self.done)
    }

    /// The line of `item`, which must come from this snapshot.
    pub fn line(&self, item: &Item) -> &Line {
        self.check(item.key);
        self.file(item.source)
            .line(item.nr)
            .unwrap_or_else(|| blank())
    }

    /// The reference to `item` that errors carry: its place, and its `id:`
    /// when it has one.
    pub fn ref_of(&self, item: &Item) -> Ref {
        Ref {
            nr: item.nr,
            source: item.source,
            id: self.id_of(item).map(lossy),
        }
    }

    /// The typed tags of `item`'s line.
    pub(crate) fn tags(&self, item: &Item) -> Tags<'_> {
        self.line(item).tags()
    }

    /// The `id:` of `item`, if it has one.
    pub fn id_of(&self, item: &Item) -> Option<&[u8]> {
        self.line(item).raw().get(item.id.clone()?)
    }

    /// The `dep:` ids of `item` that are found in neither file.
    pub fn dangling<'a>(
        &'a self,
        item: &'a Item,
    ) -> impl Iterator<Item = &'a [u8]> {
        let raw = self.line(item).raw();
        item.dangling.iter().filter_map(|r| raw.get(r.clone()))
    }

    /// Catches an id that belongs to another snapshot.
    fn check(&self, id: ItemId) {
        debug_assert_eq!(id.serial, self.serial, "item of another snapshot");
    }

    /// The id of the item at `index`, which must be in range.
    #[cfg(test)]
    pub(super) const fn id_for(&self, index: usize) -> ItemId {
        ItemId::new(index, self.serial)
    }

    pub(super) fn id_at(&self, index: usize) -> &[u8] {
        self.items
            .get(index)
            .and_then(|item| self.id_of(item))
            .unwrap_or_default()
    }

    /// The warnings about the ids that `items` carry, in warning order.
    pub fn warnings_for<'a>(
        &self,
        items: impl IntoIterator<Item = &'a Item>,
    ) -> Vec<String> {
        let ids: Vec<&[u8]> =
            items.into_iter().filter_map(|i| self.id_of(i)).collect();
        ids::warnings_for(&self.warned, &ids)
    }
}
