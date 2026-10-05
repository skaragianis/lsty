//! A Myers line diff in linear space: a longest common subsequence.

use std::collections::HashSet;

/// The lines of `a` and `b` a shortest edit script keeps, as
/// `(index in a, index in b)` in increasing order.
///
/// The common start and end are kept at once, and a line of the middle
/// that the other side lacks cannot be kept, so the search runs over the
/// lines both sides hold. Which of several equal lines pairs can differ
/// from a search over every line.
pub(super) fn common_pairs<A, B>(a: &[A], b: &[B]) -> Vec<(usize, usize)>
where
    A: AsRef<[u8]>,
    B: AsRef<[u8]>,
{
    let a: Vec<&[u8]> = a.iter().map(AsRef::as_ref).collect();
    let b: Vec<&[u8]> = b.iter().map(AsRef::as_ref).collect();
    let (prefix, suffix) = common_affixes(&a, &b);
    let mut pairs: Vec<(usize, usize)> = (0..prefix).map(|i| (i, i)).collect();

    let middle_a = &a[prefix..a.len() - suffix];
    let middle_b = &b[prefix..b.len() - suffix];
    let (index_a, lines_a) = shared(middle_a, middle_b, prefix);
    let (index_b, lines_b) = shared(middle_b, middle_a, prefix);
    let size = 2 * (lines_a.len() + lines_b.len()) + 3;
    let mut differ = Differ {
        forward: vec![0; size],
        backward: vec![0; size],
        off: len(&lines_a) + len(&lines_b) + 1,
        pairs: Vec::new(),
    };
    differ.solve(&lines_a, &lines_b, (0, 0));
    pairs.extend(differ.pairs.iter().map(|&(i, j)| (index_a[i], index_b[j])));

    let (end_a, end_b) = (a.len() - suffix, b.len() - suffix);
    pairs.extend((0..suffix).map(|i| (end_a + i, end_b + i)));
    pairs
}

/// Lines kept for the search, and the index of each in its list.
type Kept<'a> = (Vec<usize>, Vec<&'a [u8]>);

/// The lines of `lines` that `other` also holds, with their indexes
/// shifted by `from`.
fn shared<'a>(lines: Lines<'a>, other: Lines<'_>, from: usize) -> Kept<'a> {
    let held: HashSet<&[u8]> = other.iter().copied().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| held.contains(*line))
        .map(|(i, line)| (from + i, *line))
        .unzip()
}

type Lines<'a> = &'a [&'a [u8]];

/// A run of equal lines in the middle of an edit: from `(x, y)` to
/// `(u, v)`, as indexes into the old and the new lines.
struct Snake {
    x: usize,
    y: usize,
    u: usize,
    v: usize,
}

struct Differ {
    forward: Vec<isize>,
    backward: Vec<isize>,
    off: isize,
    pairs: Vec<(usize, usize)>,
}

/// The lengths of the common start and end of two line lists.
fn common_affixes(old: Lines<'_>, new: Lines<'_>) -> (usize, usize) {
    let prefix = old.iter().zip(new).take_while(|(p, q)| p == q).count();
    let (rest_old, rest_new) = (&old[prefix..], &new[prefix..]);
    let suffix = rest_old
        .iter()
        .rev()
        .zip(rest_new.iter().rev())
        .take_while(|(p, q)| p == q)
        .count();
    (prefix, suffix)
}

fn len(lines: Lines<'_>) -> isize {
    isize::try_from(lines.len()).unwrap_or(isize::MAX)
}

/// The value of diagonal `k` in a table that is centred on `off`.
fn at(table: &[isize], off: isize, k: isize) -> isize {
    usize::try_from(off + k).map_or(0, |i| table[i])
}

fn put(table: &mut [isize], off: isize, k: isize, value: isize) {
    if let Ok(i) = usize::try_from(off + k) {
        table[i] = value;
    }
}

fn index(value: isize) -> usize {
    usize::try_from(value).unwrap_or(0)
}

impl Differ {
    fn keep(&mut self, at: (usize, usize), count: usize) {
        self.pairs.extend((0..count).map(|i| (at.0 + i, at.1 + i)));
    }

    fn solve(&mut self, old: Lines<'_>, new: Lines<'_>, at: (usize, usize)) {
        let (prefix, suffix) = common_affixes(old, new);
        self.keep(at, prefix);

        let old = &old[prefix..old.len() - suffix];
        let new = &new[prefix..new.len() - suffix];
        let at = (at.0 + prefix, at.1 + prefix);

        if !old.is_empty() && !new.is_empty() {
            let s = self.middle_snake(old, new);
            self.solve(&old[..s.x], &new[..s.y], at);
            self.keep((at.0 + s.x, at.1 + s.y), s.u - s.x);
            self.solve(&old[s.u..], &new[s.v..], (at.0 + s.u, at.1 + s.v));
        }

        self.keep((at.0 + old.len(), at.1 + new.len()), suffix);
    }

    /// The snake in the middle of a shortest edit of `old` into `new`.
    fn middle_snake(&mut self, old: Lines<'_>, new: Lines<'_>) -> Snake {
        put(&mut self.forward, self.off, 1, 0);
        put(&mut self.backward, self.off, 1, 0);
        for round in 0..=(old.len() + new.len()).div_ceil(2) {
            let round = isize::try_from(round).unwrap_or(isize::MAX);
            if let Some(s) = self.forward(old, new, round) {
                return s;
            }
            if let Some(s) = self.backward(old, new, round) {
                return s;
            }
        }
        Snake {
            x: 0,
            y: 0,
            u: 0,
            v: 0,
        }
    }

    fn forward(
        &mut self,
        old: Lines<'_>,
        new: Lines<'_>,
        round: isize,
    ) -> Option<Snake> {
        let delta = len(old) - len(new);
        for k in (-round..=round).step_by(2) {
            let start = next_x(&self.forward, self.off, round, k);
            let end = slide(old, new, start, k);
            put(&mut self.forward, self.off, k, end);
            let other = delta - k;
            let meets = delta % 2 != 0
                && other > -round
                && other < round
                && end + at(&self.backward, self.off, other) >= len(old);
            if meets {
                return Some(Snake {
                    x: index(start),
                    y: index(start - k),
                    u: index(end),
                    v: index(end - k),
                });
            }
        }
        None
    }

    fn backward(
        &mut self,
        old: Lines<'_>,
        new: Lines<'_>,
        round: isize,
    ) -> Option<Snake> {
        let (n_old, n_new) = (len(old), len(new));
        let delta = n_old - n_new;
        for k in (-round..=round).step_by(2) {
            let start = next_x(&self.backward, self.off, round, k);
            let end = slide_back(old, new, start, k);
            put(&mut self.backward, self.off, k, end);
            let other = delta - k;
            let meets = delta % 2 == 0
                && (-round..=round).contains(&other)
                && end + at(&self.forward, self.off, other) >= n_old;
            if meets {
                return Some(Snake {
                    x: index(n_old - end),
                    y: index(n_new - (end - k)),
                    u: index(n_old - start),
                    v: index(n_new - (start - k)),
                });
            }
        }
        None
    }
}

/// Where the furthest path on diagonal `k` of `round` starts.
fn next_x(table: &[isize], off: isize, round: isize, k: isize) -> isize {
    let below = at(table, off, k - 1);
    let above = at(table, off, k + 1);
    if k == -round || (k != round && below < above) {
        above
    } else {
        below + 1
    }
}

/// Follows equal lines forward from `x` on diagonal `k`.
fn slide(old: Lines<'_>, new: Lines<'_>, mut x: isize, k: isize) -> isize {
    while x < len(old) && x - k < len(new) && x >= k {
        if old[index(x)] != new[index(x - k)] {
            break;
        }
        x += 1;
    }
    x
}

/// Follows equal lines backward from the ends, `x` lines in.
fn slide_back(
    old: Lines<'_>,
    new: Lines<'_>,
    mut x: isize,
    k: isize,
) -> isize {
    let (n_old, n_new) = (len(old), len(new));
    while x < n_old && x - k < n_new && x >= k {
        if old[index(n_old - 1 - x)] != new[index(n_new - 1 - (x - k))] {
            break;
        }
        x += 1;
    }
    x
}
