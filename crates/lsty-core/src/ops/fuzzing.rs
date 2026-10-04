//! Entry points for the fuzz targets, behind the `fuzzing` feature.

use super::merge::{Merged as Lines, merge as merge_lines};

/// A merged line.
#[derive(Debug)]
pub struct Merged {
    /// The merged line, with the draft's version of each clashing part.
    pub line: Vec<u8>,
    /// The parts that clash: `priority`, `text` or a key.
    pub clashes: Vec<&'static str>,
}

/// The merge of an edit with the disk's line, or `None` when removing or
/// appending a token would turn a word into a stamp.
pub fn merge(base: &[u8], draft: &[u8], disk: &[u8]) -> Option<Merged> {
    let Lines { line, clashes } = merge_lines(base, draft, disk).ok()?;
    let clashes = clashes.into_iter().map(|c| c.part).collect();
    Some(Merged { line, clashes })
}
