//! Entry points for the fuzz targets, behind the `fuzzing` feature.

use super::merge::merge as merge_lines;

/// The merged line and the clashing parts.
type Merged = (Vec<u8>, Vec<&'static str>);

/// The merged line, with the draft's version of each clashing part, and the
/// parts that clash (`priority`, `text` or a key); `None` when removing or
/// appending a token would turn a word into a stamp.
pub fn merge(base: &[u8], draft: &[u8], disk: &[u8]) -> Option<Merged> {
    let merged = merge_lines(base, draft, disk).ok()?;
    let clashes = merged.clashes.into_iter().map(|c| c.part.name()).collect();
    Some((merged.line, clashes))
}
