//! The mapping from the lines of one reading to those of the next.

use std::collections::HashMap;

use super::diff::common_pairs;

/// Maps each line of `old` to its place in `new`.
///
/// Entry `i` is the index in `new`, or `None` when line `i` has no
/// partner, that is, when it changed or is gone. No index of `new` is
/// the partner of two lines, and a partner has the same text.
pub fn remap<A, B>(old: &[A], new: &[B]) -> Vec<Option<usize>>
where
    A: AsRef<[u8]>,
    B: AsRef<[u8]>,
{
    let mut mapping = vec![None; old.len()];
    let mut taken = vec![false; new.len()];

    for (i, j) in common_pairs(old, new) {
        mapping[i] = Some(j);
        taken[j] = true;
    }

    follow_unique(old, new, &mut mapping, &taken);
    mapping
}

/// Reports where line `i` of `old` is in `new`.
///
/// `None` means the line changed: it has no partner, or `i` is not a
/// line of `old`. It agrees with [`remap`] for every index.
pub fn follow<A, B>(old: &[A], new: &[B], i: usize) -> Option<usize>
where
    A: AsRef<[u8]>,
    B: AsRef<[u8]>,
{
    remap(old, new).get(i).copied().flatten()
}

/// Maps a line the diff left alone when its text is unmapped exactly
/// once on each side.
fn follow_unique<A, B>(
    old: &[A],
    new: &[B],
    mapping: &mut [Option<usize>],
    taken: &[bool],
) where
    A: AsRef<[u8]>,
    B: AsRef<[u8]>,
{
    let mut old_count: HashMap<&[u8], usize> = HashMap::new();
    for (line, _) in
        old.iter().zip(mapping.iter()).filter(|(_, m)| m.is_none())
    {
        *old_count.entry(line.as_ref()).or_default() += 1;
    }

    // First index and count of each unmapped text of `new`.
    let mut new_at: HashMap<&[u8], (usize, usize)> = HashMap::new();
    for (j, line) in new.iter().enumerate().filter(|(j, _)| !taken[*j]) {
        new_at.entry(line.as_ref()).or_insert((j, 0)).1 += 1;
    }

    for (line, slot) in old.iter().zip(mapping.iter_mut()) {
        if slot.is_some() {
            continue;
        }
        let text = line.as_ref();
        if old_count.get(text) != Some(&1) {
            continue;
        }
        if let Some(&(only, 1)) = new_at.get(text) {
            *slot = Some(only);
        }
    }
}
