//! Entry points for the fuzz targets, behind the `fuzzing` feature.

/// The lines the diff of `old` and `new` keeps, as `(index in old,
/// index in new)` in increasing order.
pub fn common_pairs(old: &[&[u8]], new: &[&[u8]]) -> Vec<(usize, usize)> {
    super::diff::common_pairs(old, new)
}
