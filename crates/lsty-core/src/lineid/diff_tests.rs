use super::diff::common_pairs;
use super::fuzzing;

fn lines(codes: &[u8]) -> Vec<&'static [u8]> {
    fuzzing::lines(codes)
}

/// The pairs keep equal lines in order, and as many as any diff can.
fn check(a: &[u8], b: &[u8]) {
    fuzzing::pairs_property(&lines(a), &lines(b));
}

/// Every list of up to `max` lines over three different lines.
fn every_list(max: usize) -> Vec<Vec<u8>> {
    let mut all = vec![Vec::new()];
    let mut from = 0;
    for _ in 0..max {
        let to = all.len();
        for at in from..to {
            for code in 0..3 {
                let mut longer = all[at].clone();
                longer.push(code);
                all.push(longer);
            }
        }
        from = to;
    }
    all
}

#[test]
fn every_short_pair_keeps_a_longest_common_subsequence() {
    let all = every_list(5);
    for a in &all {
        for b in &all {
            check(a, b);
        }
    }
}

/// A list of `len` lines over `alphabet` different ones, from a seed.
fn sample(seed: &mut u64, len: usize, alphabet: u8) -> Vec<u8> {
    (0..len)
        .map(|_| {
            *seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            u8::try_from((*seed >> 33) % u64::from(alphabet)).unwrap_or(0)
        })
        .collect()
}

#[test]
fn longer_pairs_keep_a_longest_common_subsequence() {
    let mut seed = 20_261_004;
    for round in 0..300_usize {
        let (la, lb) = (round % 37, (round * 7) % 41);
        let a = sample(&mut seed, la, 4);
        let mut b = sample(&mut seed, lb, 4);
        if round % 3 == 0 {
            // A near copy: the common part is long.
            b.clone_from(&a);
            b.truncate(la.saturating_sub(round % 5));
            b.extend(sample(&mut seed, round % 4, 4));
        }
        check(&a, &b);
    }
}

#[test]
fn a_tie_goes_to_the_first_equal_line() {
    // `a` pairs with the first `a` of the new list.
    let (old, new) = (lines(&[0]), lines(&[1, 0, 0, 1]));
    assert_eq!(common_pairs(&old, &new), [(0, 1)]);
}

#[test]
fn repeated_lines_pair_in_order_on_both_sides() {
    let (old, new) = (lines(&[0, 1, 0]), lines(&[2, 0, 0, 1, 2]));
    assert_eq!(common_pairs(&old, &new), [(0, 1), (2, 2)]);
}
