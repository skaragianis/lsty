//! Following lines from one reading of the files to the next.

use lsty_core::errs::Source;
use lsty_core::lineid;
use lsty_core::list::Snapshot;
use lsty_core::todotxt::File;

/// A line of one of the two files: its source and its NR.
pub(crate) type Place = (Source, u32);

/// Where the lines of the old reading went in the new one.
pub(crate) struct Remap {
    todo: Vec<Option<usize>>,
    done: Vec<Option<usize>>,
}

impl Remap {
    /// Maps the lines of both files of `old` to those of `new`.
    pub(crate) fn new(old: &Snapshot, new: &Snapshot) -> Self {
        Self {
            todo: map_file(old.todo(), new.todo()),
            done: map_file(old.done(), new.done()),
        }
    }

    fn of(&self, source: Source) -> &[Option<usize>] {
        match source {
            Source::Todo => &self.todo,
            Source::Done => &self.done,
        }
    }

    /// The place of `at` now: its own when the line was followed, and
    /// otherwise the place just after the nearest earlier line that was
    /// (the first line when there is none). A changed line is thereby
    /// the first line at its old place.
    pub(crate) fn place(&self, at: Place) -> Place {
        let (source, nr) = at;
        let to = self.of(source);
        let line = usize::try_from(nr).unwrap_or(usize::MAX).saturating_sub(1);
        if let Some(&Some(j)) = to.get(line) {
            return (source, nr_of(j));
        }
        let before = to.get(..line.min(to.len())).unwrap_or_default();
        let after = before.iter().rev().find_map(|&j| j);
        (source, after.map_or(1, |j| nr_of(j + 1)))
    }
}

fn nr_of(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

/// Files that did not change map every line to itself without a diff.
fn map_file(old: &File, new: &File) -> Vec<Option<usize>> {
    if old == new {
        return (0..old.lines.len()).map(Some).collect();
    }
    lineid::remap(&old.lines, &new.lines)
}
