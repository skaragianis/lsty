use super::{
    diff::common_pairs,
    fuzzing::{Lcg, lines, pairs_property},
};

/// The pairs keep equal lines in order, and as many as any diff can.
fn check(a: &[u8], b: &[u8]) {
    pairs_property(&lines(a), &lines(b));
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

#[test]
fn longer_pairs_keep_a_longest_common_subsequence() {
    let mut rng = Lcg(20_261_004);
    for round in 0..300_usize {
        let (la, lb) = (round % 37, (round * 7) % 41);
        let a = rng.codes(la, 4);
        let mut b = rng.codes(lb, 4);
        if round % 3 == 0 {
            // A near copy: the common part is long.
            b.clone_from(&a);
            b.truncate(la.saturating_sub(round % 5));
            b.extend(rng.codes(round % 4, 4));
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

#[test]
fn a_tie_between_lines_in_either_order_goes_to_the_last_of_the_new_ones() {
    // Both `a` and `b` can pair, but not both: the search keeps the `a`
    // with the last `a` of the new list.
    let (old, new) = (lines(&[0, 1]), lines(&[1, 0, 0]));
    assert_eq!(common_pairs(&old, &new), [(0, 2)]);
}
