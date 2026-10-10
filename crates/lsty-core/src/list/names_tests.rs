use super::Count;
use super::test_util::snapshot;
use crate::todotxt::Kind;

fn count(name: &str, open: usize, total: usize) -> (Vec<u8>, Count) {
    (name.as_bytes().to_vec(), Count { open, total })
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
        [
            count("b", 1, 1),
            count("work", 1, 2),
            count("work.acme", 0, 1)
        ]
    );
    assert_eq!(snap.names(&items, Kind::Context), [count("phone", 1, 2)]);
}
