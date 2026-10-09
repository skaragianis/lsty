//! Following a line of the todo file from one reading of the files to
//! the next, and to the lines on disk when a write happens.

use std::sync::Arc;

use crate::errs::Nr;
use crate::lineid;
use crate::list::Snapshot;
use crate::todotxt::Line;

/// The lines of the todo file as a caller read them. They share their
/// bytes with the reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The serial of the reading the lines are of.
    serial: u32,
    lines: Arc<[Arc<[u8]>]>,
}

impl Seen {
    /// The lines of the todo file of `snap`.
    pub fn of(snap: &Snapshot) -> Self {
        Self {
            serial: snap.serial(),
            lines: snap.todo().lines.iter().map(Line::shared).collect(),
        }
    }

    /// Whether these are the lines of the reading `snap`.
    pub(super) fn is_of(&self, snap: &Snapshot) -> bool {
        self.serial == snap.serial()
    }

    /// The raw line at `index`.
    pub(super) fn line(&self, index: usize) -> Option<&[u8]> {
        self.lines.get(index).map(|line| &**line)
    }

    /// Where the line at `index` stands among `now`, when it is still
    /// that line: it was followed, or it changed in place.
    pub(super) fn follow<B: AsRef<[u8]>>(
        &self,
        now: &[B],
        index: usize,
    ) -> Option<usize> {
        let pin = Pin {
            spot: Spot::Live(index),
            base: Vec::new(),
            settle: Settle::Line,
        };
        match pin.follow(&self.lines, now).spot {
            Spot::Live(at) => Some(at),
            Spot::Blank(_) | Spot::After(_) => None,
        }
    }
}

/// What a pin points at, as an index into the todo file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// The line, or the line that took its place.
    Live(usize),
    /// The blank line the item left.
    Blank(usize),
    /// The item is gone: the place just after this line, or the top.
    After(Option<usize>),
}

/// Where a pin settles when its line is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    /// On the line it follows, or just after it.
    Line,
    /// On the blank line the item left, when there is one.
    LineOrBlank,
}

/// A line of the todo file followed across readings. Once the line is
/// gone it points at the blank line it left, or at the line it follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    spot: Spot,
    base: Vec<u8>,
    settle: Settle,
}

impl Pin {
    /// Pins line `nr`, which reads `base`.
    pub fn new(nr: Nr, base: impl Into<Vec<u8>>, settle: Settle) -> Self {
        Self {
            spot: Spot::Live(nr.index()),
            base: base.into(),
            settle,
        }
    }

    /// What the pin points at.
    pub const fn spot(&self) -> Spot {
        self.spot
    }

    /// The line as it was when the pin was made or last rebased.
    pub fn base(&self) -> &[u8] {
        &self.base
    }

    /// The same pin with another base.
    #[must_use]
    pub fn rebased(self, base: impl Into<Vec<u8>>) -> Self {
        Self {
            base: base.into(),
            ..self
        }
    }

    /// The same pin, pointing somewhere else.
    #[cfg(test)]
    pub(super) fn pointing(self, spot: Spot) -> Self {
        Self { spot, ..self }
    }

    fn at(&self, spot: Spot) -> Self {
        Self {
            spot,
            ..self.clone()
        }
    }

    /// The `id:` of the base.
    fn base_id(&self) -> Option<Vec<u8>> {
        Line::parse(&self.base).tags().id.map(<[u8]>::to_vec)
    }

    /// Whether the id of the base is in `done`: the item was archived.
    pub(crate) fn archived<B: AsRef<[u8]>>(&self, done: &[B]) -> bool {
        self.base_id()
            .is_some_and(|id| index_of_id(done, &id).is_some())
    }

    /// Moves the pin from the lines `old` to the lines `now`.
    #[must_use]
    pub(crate) fn follow<A, B>(&self, old: &[A], now: &[B]) -> Self
    where
        A: AsRef<[u8]>,
        B: AsRef<[u8]>,
    {
        let map = lineid::remap(old, now);
        self.follow_mapped(&map, old, now)
    }

    /// Moves the pin from the lines `old` to the lines `now`, with the
    /// place of each old line among the new ones already worked out.
    #[must_use]
    pub fn follow_mapped<A, B>(
        &self,
        map: &[Option<usize>],
        old: &[A],
        now: &[B],
    ) -> Self
    where
        A: AsRef<[u8]>,
        B: AsRef<[u8]>,
    {
        if let Spot::Live(at) = self.spot
            && let Some(to) = mapped(map, at)
        {
            return self.at(Spot::Live(to));
        }
        if let Some(found) =
            self.base_id().and_then(|id| index_of_id(now, &id))
        {
            return self.at(Spot::Live(found));
        }
        match self.spot {
            Spot::Live(at) => self.vanished(old, now, map, at),
            Spot::Blank(at) => match mapped(map, at) {
                Some(to) => self.at(Spot::Blank(to)),
                None => self.at(Spot::After(earlier(map, at))),
            },
            Spot::After(Some(at)) => self.at(Spot::After(earlier(map, at))),
            Spot::After(None) => self.clone(),
        }
    }

    /// Pairs the removed lines around the tracked one with the lines
    /// that were inserted there.
    fn vanished<A, B>(
        &self,
        old: &[A],
        now: &[B],
        map: &[Option<usize>],
        at: usize,
    ) -> Self
    where
        A: AsRef<[u8]>,
        B: AsRef<[u8]>,
    {
        if at >= map.len() {
            return self.at(Spot::After(None));
        }
        let (lo, hi) = hunk(map, at);
        let rank = old
            .get(lo..at)
            .unwrap_or_default()
            .iter()
            .filter(|line| !Line::is_blank_text(line.as_ref()))
            .count();
        let from = lo
            .checked_sub(1)
            .and_then(|i| mapped(map, i))
            .map_or(0, |j| j + 1);
        let to = mapped(map, hi + 1).unwrap_or(now.len());
        let mut taken = vec![false; now.len()];
        for j in map.iter().flatten() {
            if let Some(slot) = taken.get_mut(*j) {
                *slot = true;
            }
        }
        let blank = |j: usize| {
            now.get(j)
                .is_some_and(|line| Line::is_blank_text(line.as_ref()))
        };
        let inserted: Vec<usize> = (from..to)
            .filter(|j| !taken.get(*j).copied().unwrap_or(true))
            .collect();
        let filled = self.base_id().is_none().then(|| {
            inserted.iter().copied().filter(|j| !blank(*j)).nth(rank)
        });
        if let Some(Some(j)) = filled {
            return self.at(Spot::Live(j));
        }
        match inserted.get(rank) {
            Some(&j) if self.settle == Settle::LineOrBlank && blank(j) => {
                self.at(Spot::Blank(j))
            }
            _ => self.at(Spot::After(earlier(map, at))),
        }
    }
}

/// A pin and the lines its place refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracked {
    pin: Pin,
    seen: Seen,
}

impl Tracked {
    /// `pin`, whose place is in the reading `seen`.
    pub const fn new(pin: Pin, seen: Seen) -> Self {
        Self { pin, seen }
    }

    /// The pin as it stands among the lines `now`.
    pub(super) fn follow<B: AsRef<[u8]>>(&self, now: &[B]) -> Pin {
        self.pin.follow(&self.seen.lines, now)
    }
}

/// The index of the first line that has the tag `id:ID`.
pub(super) fn index_of_id<B: AsRef<[u8]>>(
    lines: &[B],
    id: &[u8],
) -> Option<usize> {
    let tag = [b"id:".as_slice(), id].concat();
    lines.iter().position(|raw| {
        let raw = raw.as_ref();
        raw.windows(tag.len()).any(|w| w == tag.as_slice())
            && Line::parse(raw).tags().id == Some(id)
    })
}

/// The place of the nearest line at or before `i` that was followed, or
/// `None` for the top.
fn earlier(map: &[Option<usize>], i: usize) -> Option<usize> {
    map.iter().take(i.saturating_add(1)).rev().find_map(|&j| j)
}

/// The first and last index of the run of changed lines around `at`.
fn hunk(map: &[Option<usize>], at: usize) -> (usize, usize) {
    let mut lo = at;
    while lo > 0 && map.get(lo - 1) == Some(&None) {
        lo -= 1;
    }
    let mut hi = at;
    while map.get(hi + 1) == Some(&None) {
        hi += 1;
    }
    (lo, hi)
}

/// The place of line `i` among the new lines; `None` when there is no
/// such line.
fn mapped(map: &[Option<usize>], i: usize) -> Option<usize> {
    map.get(i).copied().flatten()
}
