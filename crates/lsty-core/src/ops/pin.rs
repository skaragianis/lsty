//! Following a line of the todo file from the reading a caller saw to
//! the lines on disk now.

use std::sync::Arc;

use crate::errs::Nr;
use crate::lineid;
use crate::list::Snapshot;
use crate::todotxt::Line;

/// The lines of the todo file as a caller read them.
#[derive(Clone, Debug)]
pub struct Seen(Arc<[Vec<u8>]>);

impl Seen {
    /// The lines of the todo file of `snap`.
    pub fn of(snap: &Snapshot) -> Self {
        Self(snap.todo().lines.iter().map(|l| l.raw().to_vec()).collect())
    }

    /// The raw line at `index`.
    pub(super) fn line(&self, index: usize) -> Option<&[u8]> {
        self.0.get(index).map(Vec::as_slice)
    }

    /// Where the line at `index` stands among `now`.
    pub(super) fn follow<B: AsRef<[u8]>>(
        &self,
        now: &[B],
        index: usize,
    ) -> Option<usize> {
        follow(&self.0, now, index)
    }
}

/// A line of the todo file that a caller means, wherever it stands when
/// the write happens.
#[derive(Clone, Debug)]
pub struct Pinned {
    nr: Nr,
    seen: Option<Seen>,
}

impl Pinned {
    /// Line `nr` as it stood in the reading `seen`.
    pub const fn new(nr: Nr, seen: Seen) -> Self {
        Self {
            nr,
            seen: Some(seen),
        }
    }

    /// Line `nr` of the file as it is when the write happens.
    pub const fn fixed(nr: Nr) -> Self {
        Self { nr, seen: None }
    }

    /// The line number the caller saw.
    pub const fn nr(&self) -> Nr {
        self.nr
    }

    /// The line number now, or `None` when the line is gone.
    pub(super) fn now<B: AsRef<[u8]>>(&self, now: &[B]) -> Option<Nr> {
        let Some(seen) = &self.seen else {
            return Some(self.nr);
        };
        let at = seen.follow(now, self.nr.index())?;
        Nr::from_index(at)
    }
}

/// Where the line at index `at` of the lines `old` is among the lines
/// `now`, when the line is still there.
///
/// A line that a diff pairs with a line of `now` is followed to it. A
/// line that changed has no partner, so it is paired with the new line
/// that stands where it was: the changed lines around it and the new
/// lines in their place are matched in order, blank lines left out. A
/// line that is gone gives `None`.
pub(super) fn follow<A, B>(old: &[A], now: &[B], at: usize) -> Option<usize>
where
    A: AsRef<[u8]>,
    B: AsRef<[u8]>,
{
    let map = lineid::remap(old, now);
    let &here = map.get(at)?;
    if here.is_some() {
        return here;
    }
    let (lo, hi) = hunk(&map, at);
    let rank = old
        .get(lo..at)?
        .iter()
        .filter(|line| !Line::is_blank_text(line.as_ref()))
        .count();
    let from = bound(&map, lo.checked_sub(1)).map_or(0, |j| j + 1);
    let to = bound(&map, Some(hi + 1)).unwrap_or(now.len());
    (from..to)
        .filter(|j| !map.contains(&Some(*j)))
        .filter(|&j| {
            now.get(j)
                .is_some_and(|line| !Line::is_blank_text(line.as_ref()))
        })
        .nth(rank)
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
fn bound(map: &[Option<usize>], i: Option<usize>) -> Option<usize> {
    map.get(i?).copied().flatten()
}
