//! What an undo needs to know about an operation.

use std::sync::{Arc, OnceLock};

use super::staged::{ChangeKind, StagedFile};
use crate::{
    errs::{Nr, Source},
    lineid,
    todotxt::{File, Line},
};

/// What an operation wrote: which files, and what changed in them.
#[derive(Clone, Debug)]
pub struct UndoRecord {
    /// The operation's name.
    pub action: String,
    /// The files the operation wrote, in the order it wrote them.
    pub files: Vec<FileRecord>,
}

/// One written file: all its lines before and after, the bytes written
/// and the lines the operation staged.
#[derive(Clone, Debug)]
pub struct FileRecord {
    /// Which file.
    pub source: Source,
    /// Every line before, without its ending. The lines share their
    /// bytes with the list they were read in.
    pre: Vec<Arc<[u8]>>,
    /// Every line after, without its ending. The lines share their
    /// bytes with the list the operation left.
    post: Vec<Arc<[u8]>>,
    /// The bytes written.
    pub data: Vec<u8>,
    /// The staged lines, side effects included, and no others.
    pub changes: Vec<Change>,
    /// Whether the file ended without a newline before the write.
    pub(crate) open_end: bool,
    /// Where each line of `pre` is in `post` as a line diff pairs them;
    /// worked out when first asked for.
    diff: OnceLock<Vec<Option<usize>>>,
}

impl FileRecord {
    /// A record of one written file.
    #[must_use]
    pub const fn new(
        source: Source,
        pre: Vec<Arc<[u8]>>,
        post: Vec<Arc<[u8]>>,
        data: Vec<u8>,
        changes: Vec<Change>,
    ) -> Self {
        Self {
            source,
            pre,
            post,
            data,
            changes,
            open_end: false,
            diff: OnceLock::new(),
        }
    }

    /// Every line before, without its ending.
    #[must_use]
    pub fn pre(&self) -> &[Arc<[u8]>] {
        &self.pre
    }

    /// Every line after, without its ending.
    #[must_use]
    pub fn post(&self) -> &[Arc<[u8]>] {
        &self.post
    }

    /// Where each line of `pre` is in `post`, as a line diff pairs
    /// them. The diff runs once per record.
    #[must_use]
    pub fn diff_pre_to_post(&self) -> &[Option<usize>] {
        self.diff
            .get_or_init(|| lineid::remap(&self.pre, &self.post))
    }
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

fn shared_lines(file: &File) -> Vec<Arc<[u8]>> {
    file.lines.iter().map(Line::shared).collect()
}

impl StagedFile {
    /// The record of this file, which was read as `before` and is now
    /// written as `data`, the bytes of its sealed file.
    pub(super) fn record(
        &self,
        before: &File,
        after: &File,
        data: &[u8],
    ) -> FileRecord {
        let at = self.positions();
        let changes = self
            .rows()
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                let nr_after =
                    at.get(i).copied().flatten().and_then(Nr::from_index);
                let kind = row.kind(nr_after)?;
                Some(Change {
                    kind,
                    before: row.orig.to_vec(),
                    after: row.after().to_vec(),
                    nr_before: row.nr,
                    nr_after,
                })
            })
            .collect();
        let mut rec = FileRecord::new(
            self.source,
            shared_lines(before),
            shared_lines(after),
            data.to_vec(),
            changes,
        );
        rec.open_end = self.last_has_no_eol;
        rec
    }
}
