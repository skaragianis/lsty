use super::diff::common_pairs;

/// The length of a longest common subsequence, by the table.
fn lcs(a: &[u8], b: &[u8]) -> usize {
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

fn lines(codes: &[u8]) -> Vec<Vec<u8>> {
    codes.iter().map(|c| vec![b'a' + c]).collect()
}

/// The pairs keep equal lines in order, and as many as any diff can.
fn check(a: &[u8], b: &[u8]) {
    let (old, new) = (lines(a), lines(b));
    let pairs = common_pairs(&old, &new);
    let mut last: Option<(usize, usize)> = None;
    for &(i, j) in &pairs {
        assert_eq!(old[i], new[j], "{a:?} {b:?}");
        if let Some((li, lj)) = last {
            assert!(i > li && j > lj, "{a:?} {b:?}: {pairs:?}");
        }
        last = Some((i, j));
    }
    assert_eq!(pairs.len(), lcs(a, b), "{a:?} {b:?}: {pairs:?}");
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
fn lists_with_no_line_in_common_have_no_pairs() {
    let old: Vec<Vec<u8>> =
        (0..300).map(|i| format!("a{i}").into_bytes()).collect();
    let new: Vec<Vec<u8>> =
        (0..500).map(|i| format!("b{i}").into_bytes()).collect();
    assert_eq!(common_pairs(&old, &new), []);
    assert_eq!(common_pairs(&new, &old), []);
    assert_eq!(common_pairs(&old, &Vec::<Vec<u8>>::new()), []);
}

#[test]
fn one_line_in_common_is_still_found() {
    let mut old: Vec<Vec<u8>> =
        (0..50).map(|i| format!("a{i}").into_bytes()).collect();
    let mut new: Vec<Vec<u8>> =
        (0..60).map(|i| format!("b{i}").into_bytes()).collect();
    old[20] = b"shared".to_vec();
    new[33] = b"shared".to_vec();
    assert_eq!(common_pairs(&old, &new), [(20, 33)]);
}

#[test]
fn lists_that_share_only_their_ends_have_no_other_pairs() {
    let mut old: Vec<Vec<u8>> =
        (0..300).map(|i| format!("a{i}").into_bytes()).collect();
    let mut new: Vec<Vec<u8>> =
        (0..500).map(|i| format!("b{i}").into_bytes()).collect();
    old.insert(0, b"top".to_vec());
    new.insert(0, b"top".to_vec());
    old.push(b"end".to_vec());
    new.push(b"end".to_vec());
    assert_eq!(common_pairs(&old, &new), [(0, 0), (301, 501)]);
}

#[test]
fn lines_the_other_side_lacks_do_not_hide_the_common_subsequence() {
    let spread: Vec<u8> = (0..40).collect();
    let other: Vec<u8> = (60..110).collect();
    let mut a = spread.clone();
    a.insert(10, 150);
    a.insert(30, 151);
    let mut b = other.clone();
    b.insert(5, 151);
    b.insert(20, 150);
    check(&a, &b);
    check(&b, &a);
    check(&spread, &other);
}

#[test]
fn a_tie_goes_to_the_equal_line_nearest_the_shared_ones() {
    // `a` pairs with the first `a` of the new list: the lines only one
    // side holds are not searched.
    let (old, new) = (lines(&[0]), lines(&[1, 0, 0, 1]));
    assert_eq!(common_pairs(&old, &new), [(0, 1)]);
}

#[test]
fn repeated_lines_pair_in_order_on_both_sides() {
    let (old, new) = (lines(&[0, 1, 0]), lines(&[2, 0, 0, 1, 2]));
    assert_eq!(common_pairs(&old, &new), [(0, 1), (2, 2)]);
}
