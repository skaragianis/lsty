//! The property the fuzz targets check, behind the `fuzzing` feature. The
//! tests run it over small lists.

use super::diff::common_pairs;

/// The length of a longest common subsequence, by the table.
fn lcs(a: &[&[u8]], b: &[&[u8]]) -> usize {
    let mut table = vec![vec![0_usize; b.len() + 1]; a.len() + 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            table[i + 1][j + 1] = if x == y {
                table[i][j] + 1
            } else {
                table[i][j + 1].max(table[i + 1][j])
            };
        }
    }
    table[a.len()][b.len()]
}

/// The pairs of a line diff are strictly increasing on both sides, pair
/// equal lines only, and are as many as a longest common subsequence.
///
/// # Panics
///
/// When the property fails.
pub fn pairs_property(old: &[&[u8]], new: &[&[u8]]) {
    let pairs = common_pairs(old, new);
    let mut last: Option<(usize, usize)> = None;
    for &(i, j) in &pairs {
        assert_eq!(old[i], new[j], "{old:?} {new:?}");
        if let Some((li, lj)) = last {
            assert!(i > li && j > lj, "{old:?} {new:?}: {pairs:?}");
        }
        last = Some((i, j));
    }
    assert_eq!(pairs.len(), lcs(old, new), "{old:?} {new:?}: {pairs:?}");
}

/// One line per byte, from a small alphabet that includes a blank line,
/// so that repeats are common; at most 300 lines, so the table stays
/// small.
#[must_use]
pub fn lines(data: &[u8]) -> Vec<&'static [u8]> {
    const ALPHABET: [&[u8]; 6] = [b"", b"a", b"b", b"c", b"d", b"e"];
    data.iter()
        .take(300)
        .map(|b| ALPHABET[usize::from(b % 6)])
        .collect()
}
