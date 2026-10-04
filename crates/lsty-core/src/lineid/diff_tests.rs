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
    let all = every_list(4);
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
