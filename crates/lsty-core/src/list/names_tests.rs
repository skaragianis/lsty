use std::collections::BTreeMap;

use super::{Count, test_util::snapshot};
use crate::todotxt::Kind;

fn count(name: &str, open: usize, total: usize) -> (&[u8], Count) {
    (name.as_bytes(), Count { open, total })
}

#[test]
fn counts_exact_names_once_per_item() {
    let snap = snapshot(
        &[
            "A +work +work @phone",
            "x 2026-09-28 B +work.acme +work @phone",
            "C +b",
        ],
        &[],
    );
    let items: Vec<_> = snap.items().iter().collect();
    assert_eq!(
        snap.names(&items, Kind::Project),
        BTreeMap::from([
            count("b", 1, 1),
            count("work", 1, 2),
            count("work.acme", 0, 1)
        ])
    );
    assert_eq!(
        snap.names(&items, Kind::Context),
        BTreeMap::from([count("phone", 1, 2)])
    );
}
