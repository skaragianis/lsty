//! The mapping from the lines of one reading to those of the next.

use std::collections::{HashMap, HashSet};

use super::diff::common_pairs;
use crate::todotxt::Line;

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
    let (mut mapping, taken) = common(old, new);
    follow_unique(old, new, &mut mapping, &taken);
    mapping
}

/// The lines the two hold in common, in order: each line of `old` to its
/// place in `new`, and which places of `new` are so taken.
fn common<A, B>(old: &[A], new: &[B]) -> (Vec<Option<usize>>, Vec<bool>)
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
    (mapping, taken)
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

/// Maps each line of `old` to its place in `new` for an action aimed at it
/// from `old`, which takes no line for another.
///
/// Entry `i` is `None` when line `i` changed or went, or when the count of
/// its text differs between the two, so that it cannot be told from a
/// twin. Every entry is `None` when the file was reordered: a line both
/// hold could not be kept in order. A blank line is no task, and moving one
/// reorders nothing.
pub fn kept<A, B>(old: &[A], new: &[B]) -> Vec<Option<usize>>
where
    A: AsRef<[u8]>,
    B: AsRef<[u8]>,
{
    let (mut mapping, taken) = common(old, new);
    let went: HashSet<&[u8]> = old
        .iter()
        .zip(&mapping)
        .filter(|(_, m)| m.is_none())
        .map(|(line, _)| line.as_ref())
        .collect();
    let came: HashSet<&[u8]> = new
        .iter()
        .zip(&taken)
        .filter(|(_, t)| !**t)
        .map(|(line, _)| line.as_ref())
        .collect();
    if went
        .intersection(&came)
        .any(|line| !Line::is_blank_text(line))
    {
        return vec![None; old.len()];
    }
    for (line, slot) in old.iter().zip(&mut mapping) {
        let text = line.as_ref();
        if went.contains(text) || came.contains(text) {
            *slot = None;
        }
    }
    mapping
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
