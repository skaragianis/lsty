//! Where the lines of the reading an action was aimed from stand now.

use crate::{
    errs::{Code, Error, Source},
    lineid,
    list::Snapshot,
    todotxt::Line,
};

/// What an action says when a line it was aimed at is not there to write.
pub const LIST_CHANGED: &str = "the list changed — look again";

/// The refusal of an action aimed at a line that is not there to write.
pub(super) fn list_changed() -> Error {
    Error::new(Code::ExpectMismatch, LIST_CHANGED)
}

/// The lines `old` as they stand in `new`, for an action aimed at them:
/// nothing when the two are the same lines, which costs a comparison.
pub(super) fn kept_in<A: AsRef<[u8]>>(
    old: &[A],
    new: &[Line],
) -> Option<Vec<Option<usize>>> {
    let same = old.iter().map(AsRef::as_ref).eq(new.iter().map(Line::raw));
    (!same).then(|| lineid::kept(old, new))
}

/// The lines of one reading as they stand in a later one, file by file.
/// An unchanged file costs a comparison, and maps nothing.
#[derive(Debug)]
pub struct Landing {
    todo: Option<Vec<Option<usize>>>,
    done: Option<Vec<Option<usize>>>,
}

impl Landing {
    /// Compares the reading `from` with the reading `now`.
    pub fn of(from: &Snapshot, now: &Snapshot) -> Self {
        let map = |source| {
            kept_in(&from.file(source).lines, &now.file(source).lines)
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
