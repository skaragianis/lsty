//! Where a new line goes beside a line of a reading, which the file may
//! have left since. This places a line and says nothing of the line it
//! goes beside.

use std::sync::Arc;

use crate::{lineid, list::Snapshot, todotxt::Line};

/// The lines of the todo file as a caller read them. They share their
/// bytes with the reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    lines: Arc<[Arc<[u8]>]>,
}

impl Seen {
    /// The lines of the todo file of `snap`.
    pub fn of(snap: &Snapshot) -> Self {
        Self {
            lines: snap.todo().lines.iter().map(Line::shared).collect(),
        }
    }

    /// The index among `now` of a new line beside the line at `at`: above
    /// it, or below it for `below`. A line changed in place is still that
    /// line. Once it is gone, the index just after the nearest earlier line
    /// that is still there, or the top.
    pub(super) fn beside<B: AsRef<[u8]>>(
        &self,
        now: &[B],
        at: usize,
        below: bool,
    ) -> usize {
        let map = lineid::remap(&self.lines, now);
        if let Some(to) = map.get(at).copied().flatten() {
            return to + usize::from(below);
        }
        let earlier = map.iter().take(at).rev().find_map(|&to| to);
        let slot = earlier.map_or(0, |to| to + 1);
        let changed = slot < now.len() && !map.contains(&Some(slot));
        slot + usize::from(changed && below)
    }
}
