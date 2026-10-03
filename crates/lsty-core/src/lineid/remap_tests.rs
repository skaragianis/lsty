//! Table tests for the line mapping.

use super::{follow, remap};
use crate::todotxt::File;

/// Lines are written separated by spaces; `_` is a blank line and `.`
/// no line at all.
fn lines(text: &str) -> Vec<&str> {
    text.split(' ')
        .filter(|f| *f != ".")
        .map(|f| if f == "_" { "" } else { f })
        .collect()
}

const N: Option<usize> = None;

struct Case {
    name: &'static str,
    old: &'static str,
    new: &'static str,
    want: &'static [Option<usize>],
}

const CASES: &[Case] = &[
    Case {
        name: "26a.1 line added",
        old: "a b c",
        new: "a x b c",
        want: &[Some(0), Some(2), Some(3)],
    },
    Case {
        name: "26a.2 line removed",
        old: "a b c",
        new: "a c",
        want: &[Some(0), N, Some(1)],
    },
    Case {
        name: "26a.3 line moved",
        old: "a b c",
        new: "c a b",
        want: &[Some(1), Some(2), Some(0)],
    },
    Case {
        name: "26a.4 line changed",
        old: "a b",
        new: "a B",
        want: &[Some(0), N],
    },
    Case {
        name: "26a.5 two new partners",
        old: "a m b c",
        new: "a b c m m",
        want: &[Some(0), N, Some(1), Some(2)],
    },
    Case {
        name: "26a.6 two old partners",
        old: "a b m m",
        new: "m a b",
        want: &[Some(1), Some(2), N, N],
    },
    Case {
        name: "26a.7 blank compacted",
        old: "a _ b",
        new: "a b",
        want: &[Some(0), N, Some(1)],
    },
    Case {
        name: "26a.8 blank and move",
        old: "a _ b",
        new: "_ b a",
        want: &[Some(2), Some(0), Some(1)],
    },
    Case {
        name: "26a.9 identical",
        old: "a b c",
        new: "a b c",
        want: &[Some(0), Some(1), Some(2)],
    },
    Case {
        name: "26a.9 empty new",
        old: "a b",
        new: ".",
        want: &[N, N],
    },
    Case {
        name: "26a.9 empty old",
        old: ".",
        new: "a b",
        want: &[],
    },
];

#[test]
fn remap_table() {
    for c in CASES {
        let got = remap(&lines(c.old), &lines(c.new));
        assert_eq!(got, c.want, "{}", c.name);
    }
}

#[test]
fn follow_table() {
    let rows = [
        ("26a.10 added", "a b c", "a x b c", 1, Some(2)),
        ("26a.11 changed", "a b", "a B", 1, N),
        ("26a.11 removed", "a b", "a", 1, N),
        ("out of range", "a b", "a b", 2, N),
    ];
    for (name, old, new, i, want) in rows {
        assert_eq!(follow(&lines(old), &lines(new), i), want, "{name}");
    }
}

#[test]
fn follow_agrees_with_remap() {
    for c in CASES {
        let (old, new) = (lines(c.old), lines(c.new));
        for (i, want) in remap(&old, &new).into_iter().enumerate() {
            assert_eq!(follow(&old, &new, i), want, "{} line {i}", c.name);
        }
    }
}

#[test]
fn large_file_with_a_line_added_on_top() {
    let old: Vec<String> = (0..10_000).map(|i| format!("line {i}")).collect();
    let mut new = vec![String::from("top")];
    new.extend(old.iter().cloned());
    for (i, j) in remap(&old, &new).into_iter().enumerate() {
        assert_eq!(j, Some(i + 1));
    }
}

/// A small deterministic generator, enough to shuffle and to draw.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).map_or(0, |v| v % n)
    }
}

#[test]
fn shuffled_file_maps_to_equal_lines() {
    let old: Vec<String> = (0..3000).map(|i| format!("line {i}")).collect();
    let mut new = old.clone();
    let mut rng = Lcg(7);
    for i in (1..new.len()).rev() {
        new.swap(i, rng.below(i + 1));
    }
    let got = remap(&old, &new);
    for (i, j) in got.iter().enumerate() {
        if let Some(j) = j {
            assert_eq!(old[i], new[*j]);
        }
    }
}

/// The length of the longest common subsequence by dynamic programming.
fn dp_lcs(a: &[&str], b: &[&str]) -> usize {
    let mut row = vec![0; b.len() + 1];
    for x in a {
        let mut prev = 0;
        for (j, y) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = if x == y { prev + 1 } else { row[j].max(cur) };
            prev = cur;
        }
    }
    row[b.len()]
}

fn draw<'a>(rng: &mut Lcg, alphabet: &[&'a str], n: usize) -> Vec<&'a str> {
    (0..n)
        .map(|_| alphabet[rng.below(alphabet.len())])
        .collect()
}

#[test]
fn diff_keeps_a_longest_common_subsequence() {
    let alphabet = ["a", "b", "c", "d"];
    let mut rng = Lcg(3);
    for _ in 0..3000 {
        let (na, nb) = (rng.below(14), rng.below(14));
        let a = draw(&mut rng, &alphabet, na);
        let b = draw(&mut rng, &alphabet, nb);
        let pairs = super::diff::common_pairs(&a, &b);
        assert_eq!(pairs.len(), dp_lcs(&a, &b), "{a:?} vs {b:?}");
        for (k, &(i, j)) in pairs.iter().enumerate() {
            assert_eq!(a[i], b[j], "{a:?} vs {b:?}");
            if k > 0 {
                let (pi, pj) = pairs[k - 1];
                assert!(i > pi && j > pj, "{a:?} vs {b:?}");
            }
        }
    }
}

#[test]
fn partners_are_unique_and_equal() {
    let alphabet = ["a", "b", "c"];
    let mut rng = Lcg(11);
    for _ in 0..2000 {
        let (na, nb) = (rng.below(12), rng.below(12));
        let old = draw(&mut rng, &alphabet, na);
        let new = draw(&mut rng, &alphabet, nb);
        let mut seen = vec![false; new.len()];
        for (i, j) in remap(&old, &new).into_iter().enumerate() {
            let Some(j) = j else { continue };
            assert!(!seen[j]);
            seen[j] = true;
            assert_eq!(old[i], new[j]);
        }
    }
}

#[test]
fn line_endings_play_no_part() {
    let crlf = File::parse(b"a\r\nb\r\n");
    let lf = File::parse(b"a\nb\n");
    let want = vec![Some(0), Some(1)];
    assert_eq!(remap(&crlf.lines, &lf.lines), want, "26a.14");
    assert_eq!(remap(&lf.lines, &crlf.lines), want, "26a.14");
}
