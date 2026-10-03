use std::ops::Range;
use std::sync::OnceLock;
use std::time::Duration;

use jiff::Timestamp;

use super::ids::{self, Warned};
use crate::dates::Clock;
use crate::discover::Found;
use crate::errs::{Nr, Source};
use crate::todotxt::{File, Line, Tags};

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

impl Claim {
    /// The claim as written in output: `none`, `human` or `agent`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Human => "human",
            Self::Agent => "agent",
        }
    }
}

/// The position of an item in [`Snapshot::items`]. It is only meaningful
/// for the snapshot that gave it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(usize);

impl ItemId {
    pub(super) const fn new(index: usize) -> Self {
        Self(index)
    }

    /// The index into [`Snapshot::items`].
    pub const fn index(self) -> usize {
        self.0
    }
}

/// A non-blank line with the state derived from the whole list.
///
/// An item keeps no borrow of its line: [`Snapshot::line`] and
/// [`Snapshot::tags`] give it back. Its fields are read through methods,
/// so what it says always agrees with the list it came from.
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
    pub(super) overdue: bool,
    pub(super) stale: bool,
    pub(super) handle: ItemId,
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

    /// Todo, doing or done.
    pub const fn state(&self) -> State {
        self.state
    }

    /// Who is working on it.
    pub const fn claim(&self) -> Claim {
        self.claim
    }

    /// The item's handle.
    pub const fn handle(&self) -> ItemId {
        self.handle
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
    pub fn has_dangling(&self) -> bool {
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

    /// Has a valid `t:` after today, whatever its state.
    pub const fn later(&self) -> bool {
        self.later
    }

    /// Open with a valid `due:` before today.
    pub const fn overdue(&self) -> bool {
        self.overdue
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

/// Both files as read at one moment, with derived state.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub(super) files: Found,
    pub(super) todo: File,
    pub(super) done: File,
    pub(super) items: Vec<Item>,
    pub(super) warnings: Vec<String>,
    pub(super) todo_items: usize,
    pub(super) by_id: Vec<usize>,
    pub(super) warned: Vec<Warned>,
}

/// The line `line` falls back to; no item of a snapshot points at it.
fn blank() -> &'static Line {
    static BLANK: OnceLock<Line> = OnceLock::new();
    BLANK.get_or_init(|| Line::parse(Vec::new()))
}

impl Snapshot {
    /// The files the snapshot was read from.
    pub const fn files(&self) -> &Found {
        &self.files
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
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Takes the snapshot apart, so a rebuild can reuse the files.
    pub fn into_files(self) -> (Found, File, File) {
        (self.files, self.todo, self.done)
    }

    /// The item behind `id`.
    pub fn item(&self, id: ItemId) -> Option<&Item> {
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
        match source {
            Source::Todo => todo,
            Source::Done => done,
        }
    }

    /// The line of `item`, which must come from this snapshot.
    pub fn line(&self, item: &Item) -> &Line {
        let file = match item.source {
            Source::Todo => &self.todo,
            Source::Done => &self.done,
        };
        file.line(item.nr).unwrap_or_else(|| blank())
    }

    /// The typed tags of `item`'s line.
    pub fn tags(&self, item: &Item) -> Tags<'_> {
        self.line(item).tags()
    }

    /// The `id:` of `item`, if it has one.
    pub fn id_of(&self, item: &Item) -> Option<&[u8]> {
        let range = item.id.clone()?;
        self.line(item).raw().get(range)
    }

    /// The `dep:` ids of `item` that are found in neither file.
    pub fn dangling(&self, item: &Item) -> Vec<&[u8]> {
        let raw = self.line(item).raw();
        let found = item.dangling.iter().filter_map(|r| raw.get(r.clone()));
        found.collect()
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
