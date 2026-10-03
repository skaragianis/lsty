//! What an undo needs to know about an operation.

use super::staged::{ChangeKind, StagedFile};
use crate::errs::{Nr, Source};
use crate::todotxt::File;

/// What an operation wrote: which files, and what changed in them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoRecord {
    /// The operation's name.
    pub action: String,
    /// The files the operation wrote, in the order it wrote them.
    pub files: Vec<FileRecord>,
}

/// One written file: all its lines before and after, the bytes written
/// and the lines the operation staged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRecord {
    /// Which file.
    pub source: Source,
    /// Every line before, without its ending.
    pub pre: Vec<Vec<u8>>,
    /// Every line after, without its ending.
    pub post: Vec<Vec<u8>>,
    /// The bytes written.
    pub data: Vec<u8>,
    /// The staged lines, side effects included, and no others.
    pub changes: Vec<Change>,
}

/// One staged line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// What was done to the line.
    pub kind: ChangeKind,
    /// The text as read; empty for a line that did not exist.
    pub before: Vec<u8>,
    /// The text written; empty for a line that is gone.
    pub after: Vec<u8>,
    /// The line number before; `None` when the line did not exist.
    pub nr_before: Option<Nr>,
    /// The line number after; `None` when the line is gone.
    pub nr_after: Option<Nr>,
}

fn raw_lines(file: &File) -> Vec<Vec<u8>> {
    file.lines.iter().map(|l| l.raw().to_vec()).collect()
}

impl StagedFile {
    /// The record of this file, which was read as `before` and is now
    /// written as `data`, the bytes of its sealed file.
    pub(super) fn record(&self, before: &File, data: &[u8]) -> FileRecord {
        let at = self.positions();
        let changes = self
            .all
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                let nr_after =
                    at.get(i).copied().flatten().and_then(Nr::from_index);
                let kind = row.kind(nr_after)?;
                Some(Change {
                    kind,
                    before: row.orig.clone(),
                    after: row.after().to_vec(),
                    nr_before: row.nr,
                    nr_after,
                })
            })
            .collect();
        FileRecord {
            source: self.source,
            pre: raw_lines(before),
            post: raw_lines(&File::parse(data)),
            data: data.to_vec(),
            changes,
        }
    }
}
