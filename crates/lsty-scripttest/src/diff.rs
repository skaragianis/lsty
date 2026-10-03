//! Unified diffs for failed comparisons.

use similar::TextDiff;

/// A unified diff from `old` to `new`, labelled with their names.
pub(crate) fn unified(
    old_name: &str,
    old: &str,
    new_name: &str,
    new: &str,
) -> String {
    TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(old_name, new_name)
        .to_string()
}
