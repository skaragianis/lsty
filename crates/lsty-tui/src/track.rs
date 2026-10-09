//! Following lines from one reading of the files to the next.

use std::sync::Arc;

use lsty_core::errs::{Nr, Source};
use lsty_core::lineid;
use lsty_core::list::{Item, Snapshot};
use lsty_core::ops::{ChangeKind, FileRecord, UndoRecord};
use lsty_core::todotxt::{File, Line};

/// A line of one of the two files: its source and its NR.
pub(crate) type Place = (Source, Nr);

/// Where an item's line stands in its file.
pub(crate) fn place_of(item: &Item) -> Place {
    (item.source(), item.nr())
}

/// A line a write was about.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Touched {
    /// Where the line is after the write.
    pub(crate) now: Place,
    /// Where it stood when it was read, when that is known.
    pub(crate) was: Option<Place>,
}

/// What a write of this session did to the lines it was about.
pub(crate) struct Wrote<'a> {
    pub(crate) record: &'a UndoRecord,
    /// The lines the write was about.
    pub(crate) touched: &'a [Touched],
}

/// Where the lines of the old reading went in the new one.
pub(crate) struct Remap {
    files: [FileMap; 2],
}

/// Where the lines of one file went.
struct FileMap {
    /// Where each old line went, as a diff pairs them.
    plain: Vec<Option<usize>>,
    /// The same for the lines of a write, followed as it moved them.
    exact: Option<Vec<Option<usize>>>,
    /// Which lines of the file an operation took out of the list.
    gone: Vec<bool>,
    /// Which new lines an old line went to.
    taken: Vec<bool>,
    /// For each count of old lines, the new line that the nearest
    /// followed one among them went to.
    nearest: Vec<Option<usize>>,
}

impl FileMap {
    fn plain(plain: Vec<Option<usize>>) -> Self {
        let mut map = Self {
            plain,
            exact: None,
            gone: Vec::new(),
            taken: Vec::new(),
            nearest: Vec::new(),
        };
        map.index();
        map
    }

    /// The map of a file that `file` wrote: `old` was read before,
    /// `new` is the file now.
    fn written(old: &File, new: &File, file: &FileRecord) -> Self {
        let read_same = same(&old.lines, file.pre());
        let followed = exact(old, file, read_same);
        let diffed = read_same && same(&new.lines, file.post());
        let mut plain = if diffed {
            file.diff_pre_to_post().to_vec()
        } else {
            map_file(old, new)
        };
        let lines = followed.lines.iter().zip(&followed.moved);
        for (at, (to, moved)) in plain.iter_mut().zip(lines) {
            if *moved {
                *at = *to;
            }
        }
        let mut map = Self::plain(plain);
        map.gone = followed.gone;
        map.exact = Some(followed.lines);
        map
    }

    /// Works out the lookups `track` and `place` use.
    fn index(&mut self) {
        let size = self.plain.iter().flatten().max().map_or(0, |m| m + 1);
        self.taken = vec![false; size];
        let mut last = None;
        self.nearest = Vec::with_capacity(self.plain.len() + 1);
        for to in &self.plain {
            self.nearest.push(last);
            if let Some(j) = *to {
                self.taken[j] = true;
                last = Some(j);
            }
        }
        self.nearest.push(last);
    }

    fn exact(&self) -> &[Option<usize>] {
        self.exact.as_deref().unwrap_or(&self.plain)
    }

    fn is_taken(&self, index: usize) -> bool {
        self.taken.get(index) == Some(&true)
    }

    /// The 0-based index of the line with 0-based `index` now: its own
    /// when it was followed, and otherwise the place just after the
    /// nearest earlier line that was (the first line when there is
    /// none).
    fn place(&self, index: usize) -> usize {
        if let Some(&Some(j)) = self.plain.get(index) {
            return j;
        }
        let before = index.min(self.plain.len());
        self.nearest
            .get(before)
            .copied()
            .flatten()
            .map_or(0, |j| j + 1)
    }
}

impl Remap {
    /// Maps the lines of both files of `old` to those of `new`.
    pub(crate) fn new(old: &Snapshot, new: &Snapshot) -> Self {
        Self {
            files: [Source::Todo, Source::Done].map(|source| {
                let (old, new) =
                    (old_file(old, source), old_file(new, source));
                FileMap::plain(map_file(old, new))
            }),
        }
    }

    /// Maps the lines of `old` to those of `new`, which is `old` after
    /// the operation of `record`. Places follow a diff, except that a
    /// line the operation moved is followed to where it went, which a
    /// diff cannot tell when the line has a twin. Tracked lines of a
    /// history follow the operation exactly.
    pub(crate) fn written(
        old: &Snapshot,
        new: &Snapshot,
        record: &UndoRecord,
    ) -> Self {
        Self {
            files: [Source::Todo, Source::Done].map(|source| {
                let (before, after) =
                    (old_file(old, source), old_file(new, source));
                match record.files.iter().find(|f| f.source == source) {
                    Some(file) => FileMap::written(before, after, file),
                    None => FileMap::plain(map_file(before, after)),
                }
            }),
        }
    }

    fn of(&self, source: Source) -> &FileMap {
        &self.files[slot(source)]
    }

    /// The place of `at` now when the line is still in the list: it was
    /// followed, or it changed in place and no other line took its
    /// place. `None` for a line that an operation took out.
    pub(crate) fn track(&self, at: Place) -> Option<Place> {
        let (source, nr) = at;
        let map = self.of(source);
        let line = nr.index();
        let followed = map.plain.get(line)?.is_some();
        let gone = map.gone.get(line) == Some(&true);
        let now = self.place(at);
        let free = !map.is_taken(now.1.index());
        (!gone && (followed || free)).then_some(now)
    }

    /// Where each line of the old todo file went, as a diff pairs them.
    pub(crate) fn todo_map(&self) -> &[Option<usize>] {
        &self.of(Source::Todo).plain
    }

    /// The place the line at `at` was followed to, when it was.
    pub(crate) fn followed(&self, at: Place) -> Option<Place> {
        let (source, nr) = at;
        let to = (*self.of(source).plain.get(nr.index())?)?;
        Some((source, Nr::from_index(to)?))
    }

    /// Where lines that stood at `at` in the old reading are in the new
    /// one.
    pub(crate) fn compose(
        &self,
        source: Source,
        at: &[Option<usize>],
    ) -> Vec<Option<usize>> {
        let to = self.of(source).exact();
        at.iter()
            .map(|i| i.and_then(|i| to.get(i).copied().flatten()))
            .collect()
    }

    /// The place of `at` now: its own when the line was followed, and
    /// otherwise the place just after the nearest earlier line that was
    /// (the first line when there is none). A changed line is thereby
    /// the first line at its old place.
    pub(crate) fn place(&self, at: Place) -> Place {
        let (source, nr) = at;
        let index = self.of(source).place(nr.index());
        (source, Nr::from_index(index).unwrap_or(nr))
    }
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

/// Where each line of an old reading went through the operation that
/// wrote a file.
struct Through {
    /// The place of each line, or `None`.
    lines: Vec<Option<usize>>,
    /// Which lines the operation took out of the list.
    gone: Vec<bool>,
    /// Which lines the operation moved.
    moved: Vec<bool>,
}

fn exact(old: &File, file: &FileRecord, read_same: bool) -> Through {
    let read = if read_same {
        (0..old.lines.len()).map(Some).collect()
    } else {
        lineid::remap(&old.lines, file.pre())
    };
    let through = file.pre_to_post();
    let mut moved_pre = vec![false; file.pre().len()];
    for at in file
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Moved)
        .filter_map(|c| c.nr_before)
        .map(Nr::index)
    {
        if let Some(slot) = moved_pre.get_mut(at) {
            *slot = true;
        }
    }
    Through {
        lines: read
            .iter()
            .map(|at| at.and_then(|i| through.get(i).copied().flatten()))
            .collect(),
        gone: read
            .iter()
            .map(|at| at.is_some_and(|i| vanished(file, through.get(i))))
            .collect(),
        moved: read
            .iter()
            .map(|at| at.is_some_and(|i| moved_pre.get(i) == Some(&true)))
            .collect(),
    }
}

fn same(lines: &[Line], kept: &[Arc<[u8]>]) -> bool {
    lines.len() == kept.len()
        && lines.iter().zip(kept).all(|(l, k)| l.raw() == &**k)
}

/// Whether a line of the record went nowhere, or to a blank line.
fn vanished(file: &FileRecord, to: Option<&Option<usize>>) -> bool {
    match to.copied().flatten() {
        None => true,
        Some(t) => file.post().get(t).is_some_and(|line| line.is_empty()),
    }
}
