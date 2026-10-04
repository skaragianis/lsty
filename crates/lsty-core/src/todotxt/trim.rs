//! Trimming white space off bytes that are not always UTF-8.

use std::ops::Range;

/// The range of `bytes` without the Unicode white space at each end.
/// Bytes that are not UTF-8 are not white space: they stop the trimming
/// on their side.
pub fn trim_range(bytes: &[u8]) -> Range<usize> {
    let start = leading_white_space(bytes);
    let rest = bytes.get(start..).unwrap_or_default();
    start..bytes.len() - trailing_white_space(rest)
}

/// `bytes` without the Unicode white space at each end; see
/// [`trim_range`].
pub fn trim(bytes: &[u8]) -> &[u8] {
    bytes.get(trim_range(bytes)).unwrap_or_default()
}

fn leading_white_space(bytes: &[u8]) -> usize {
    let Some(first) = bytes.utf8_chunks().next() else {
        return 0;
    };
    let valid = first.valid();
    valid.len() - valid.trim_start().len()
}

fn trailing_white_space(bytes: &[u8]) -> usize {
    let Some(last) = bytes.utf8_chunks().last() else {
        return 0;
    };
    if !last.invalid().is_empty() {
        return 0;
    }
    let valid = last.valid();
    valid.len() - valid.trim_end().len()
}
