//! Where the lines of the reading an action was aimed from stand now.

use crate::{errs::Source, lineid, list::Snapshot, todotxt::Line};

/// What an action says when a line it was aimed at is not there to write.
pub const LIST_CHANGED: &str = "the list changed — look again";

/// The lines of one reading as they stand in a later one, file by file.
/// An unchanged file costs a comparison, and maps nothing.
#[derive(Clone, Debug)]
pub struct Landing {
    todo: Option<Vec<Option<usize>>>,
    done: Option<Vec<Option<usize>>>,
}

impl Landing {
    /// Compares the reading `from` with the reading `now`.
    pub fn of(from: &Snapshot, now: &Snapshot) -> Self {
        let map = |source| {
            let (old, new) =
                (&from.file(source).lines, &now.file(source).lines);
            let same = old.iter().map(Line::raw).eq(new.iter().map(Line::raw));
            (!same).then(|| lineid::kept(old, new))
        };
        Self {
            todo: map(Source::Todo),
            done: map(Source::Done),
        }
    }

    /// The index the line at `index` of `source` has now; `None` when an
    /// action aimed at it writes nothing (see [`lineid::kept`]).
    pub fn line(&self, source: Source, index: usize) -> Option<usize> {
        source
            .pick(&self.todo, &self.done)
            .as_ref()
            .map_or(Some(index), |map| map.get(index).copied().flatten())
    }
}
