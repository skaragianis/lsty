//! The pairs of a line diff are strictly increasing on both sides, pair
//! equal lines only, and are as many as a longest common subsequence.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::lineid::fuzzing::common_pairs;

/// The most lines on a side, so the table below stays small.
const MOST: usize = 300;

fuzz_target!(|data: &[u8]| {
    let mut parts = data.splitn(2, |b| *b == b'|');
    let old = lines(parts.next().unwrap_or_default());
    let new = lines(parts.next().unwrap_or_default());
    let pairs = common_pairs(&old, &new);
    let mut last: Option<(usize, usize)> = None;
    for &(i, j) in &pairs {
        assert_eq!(old[i], new[j]);
        if let Some((li, lj)) = last {
            assert!(i > li && j > lj);
        }
        last = Some((i, j));
    }
    assert_eq!(pairs.len(), lcs(&old, &new));
});

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

/// One line per byte, from a small alphabet that includes a blank line,
/// so that repeats are common.
fn lines(data: &[u8]) -> Vec<&'static [u8]> {
    const ALPHABET: [&[u8]; 6] = [b"", b"a", b"b", b"c", b"d", b"e"];
    data.iter()
        .take(MOST)
        .map(|b| ALPHABET[usize::from(b % 6)])
        .collect()
}
