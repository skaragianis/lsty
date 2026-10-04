//! Following lines from one reading of the files to the next.

use lsty_core::errs::Source;
use lsty_core::lineid;
use lsty_core::list::Snapshot;
use lsty_core::ops::{FileRecord, UndoRecord};
use lsty_core::todotxt::File;

/// A line of one of the two files: its source and its NR.
pub(crate) type Place = (Source, u32);

/// What a write of this session did to the lines it was about.
pub(crate) struct Wrote<'a> {
    pub(crate) record: &'a UndoRecord,
    /// The lines the write was about, as they are after it.
    pub(crate) touched: &'a [Place],
    /// The same lines as they were read.
    pub(crate) from: &'a [Place],
}

/// Where the lines of the old reading went in the new one.
pub(crate) struct Remap {
    todo: Vec<Option<usize>>,
    done: Vec<Option<usize>>,
    /// Which lines of each file an operation took out of the list.
    gone: [Vec<bool>; 2],
}

impl Remap {
    /// Maps the lines of both files of `old` to those of `new`.
    pub(crate) fn new(old: &Snapshot, new: &Snapshot) -> Self {
        Self {
            todo: map_file(old.todo(), new.todo()),
            done: map_file(old.done(), new.done()),
            gone: [Vec::new(), Vec::new()],
        }
    }

    /// Maps the lines of `old` to those of `new`, which is `old` after
    /// the operation of `record`: the lines that the operation staged are
    /// followed as it moved them, not as a diff would guess.
    pub(crate) fn written(
        old: &Snapshot,
        new: &Snapshot,
        record: &UndoRecord,
    ) -> Self {
        let mut map = Self {
            todo: Vec::new(),
            done: Vec::new(),
            gone: [Vec::new(), Vec::new()],
        };
        for source in [Source::Todo, Source::Done] {
            let lines = match record.files.iter().find(|f| f.source == source)
            {
                Some(file) => {
                    let (lines, gone) = exact(old_file(old, source), file);
                    map.gone[slot(source)] = gone;
                    lines
                }
                None => map_file(old_file(old, source), old_file(new, source)),
            };
            match source {
                Source::Todo => map.todo = lines,
                Source::Done => map.done = lines,
            }
        }
        map
    }

    /// The place of `at` now when the line is still in the list: it was
    /// followed, or it changed in place and no other line took its
    /// place. `None` for a line that an operation took out.
    pub(crate) fn track(&self, at: Place) -> Option<Place> {
        let (source, nr) = at;
        let to = self.of(source);
        let line = usize::try_from(nr).unwrap_or(usize::MAX).checked_sub(1)?;
        let followed = to.get(line)?.is_some();
        let gone = self.gone[slot(source)].get(line) == Some(&true);
        let now = self.place(at);
        let index = usize::try_from(now.1)
            .unwrap_or(usize::MAX)
            .saturating_sub(1);
        let free = !to.contains(&Some(index));
        (!gone && (followed || free)).then_some(now)
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

const fn slot(source: Source) -> usize {
    match source {
        Source::Todo => 0,
        Source::Done => 1,
    }
}

fn old_file(snap: &Snapshot, source: Source) -> &File {
    match source {
        Source::Todo => snap.todo(),
        Source::Done => snap.done(),
    }
}

/// Where each line of `old` went through the operation that wrote
/// `file`, and which of them the operation took out.
fn exact(old: &File, file: &FileRecord) -> (Vec<Option<usize>>, Vec<bool>) {
    let read = if same(old, &file.pre) {
        (0..old.lines.len()).map(Some).collect()
    } else {
        lineid::remap(&old.lines, &file.pre)
    };
    let through = file.pre_to_post();
    let lines: Vec<Option<usize>> = read
        .iter()
        .map(|at| at.and_then(|i| through.get(i).copied().flatten()))
        .collect();
    let gone = read
        .iter()
        .map(|at| at.is_some_and(|i| vanished(file, through.get(i))))
        .collect();
    (lines, gone)
}

fn same(old: &File, pre: &[Vec<u8>]) -> bool {
    old.lines.len() == pre.len()
        && old
            .lines
            .iter()
            .zip(pre)
            .all(|(l, p)| l.raw() == p.as_slice())
}

/// Whether a line of the record went nowhere, or to a blank line.
fn vanished(file: &FileRecord, to: Option<&Option<usize>>) -> bool {
    match to.copied().flatten() {
        None => true,
        Some(t) => file.post.get(t).is_some_and(Vec::is_empty),
    }
}
