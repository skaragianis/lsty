//! The properties the fuzz targets check, and the helpers the tests share
//! with them.

use super::{diff::common_pairs, follow, remap};

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
/// so that repeats are common.
#[must_use]
pub fn lines(data: &[u8]) -> Vec<&'static [u8]> {
    const ALPHABET: [&[u8]; 6] = [b"", b"a", b"b", b"c", b"d", b"e"];
    data.iter().map(|b| ALPHABET[usize::from(b % 6)]).collect()
}

/// Every line of the new list is the partner of at most one old line, a
/// partner has the same text, and following line `pick` agrees with the
/// whole mapping.
///
/// # Panics
///
/// When the property fails.
pub fn remap_property(old: &[&[u8]], new: &[&[u8]], pick: usize) {
    let mapping = remap(old, new);
    let mut seen = vec![false; new.len()];
    for (i, j) in mapping.iter().enumerate() {
        let Some(j) = *j else { continue };
        assert!(!seen[j]);
        seen[j] = true;
        assert_eq!(old[i], new[j]);
    }
    assert_eq!(follow(old, new, pick), mapping.get(pick).copied().flatten());
}

/// A small deterministic generator for the tests.
#[cfg(test)]
pub struct Lcg(pub u64);

#[cfg(test)]
impl Lcg {
    /// A number below `n`.
    pub fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize % n
    }

    /// `len` line codes below `alphabet`.
    pub fn codes(&mut self, len: usize, alphabet: usize) -> Vec<u8> {
        (0..len)
            .map(|_| self.below(alphabet).to_le_bytes()[0])
            .collect()
    }
}
