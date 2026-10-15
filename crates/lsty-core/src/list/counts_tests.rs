use std::collections::BTreeMap;

use super::{Count, test_util::snapshot};
use crate::{list::Snapshot, todotxt::Kind};

const TODO: [&str; 13] = [
    "Send invoice +home @phone",
    "Call client +home",
    "Mow lawn +home",
    "x 2026-09-25 Paint fence +home",
    "Draft report +work",
    "Review contract +work",
    "Plan +work.acme",
    "Fix +work.acme",
    "x 2026-09-28 Done +work.acme",
    "Roadmap +work/q3",
    "Wiki +Work",
    "Call @phone",
    "Plain item",
];

const DONE: [&str; 6] = [
    "x 2026-09-01 One +work @phone",
    "x 2026-09-02 Two +work @phone",
    "x 2026-09-03 Three +work",
    "x 2026-09-04 Four +work",
    "x 2026-09-05 Five +gone",
    "x 2026-09-06 Six @gone",
];

fn count(open: usize, total: usize) -> Count {
    Count { open, total }
}

fn of(snap: &Snapshot, kind: Kind, name: &str) -> Option<Count> {
    snap.rolled_counts(kind).get(name.as_bytes()).copied()
}

fn fixture() -> Snapshot {
    snapshot(&TODO, &DONE)
}

fn names(snap: &Snapshot, kind: Kind) -> BTreeMap<String, Count> {
    snap.rolled_counts(kind)
        .into_iter()
        .map(|(name, c)| (String::from_utf8_lossy(&name).into_owned(), c))
        .collect()
}

fn map<const N: usize>(rows: [(&str, Count); N]) -> BTreeMap<String, Count> {
    rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[test]
fn counts_add_the_done_file_to_the_totals() {
    // 31.1
    let snap = fixture();
    let contexts = map([("phone", count(2, 4))]);
    assert_eq!(names(&snap, Kind::Context), contexts);
}

#[test]
fn a_name_twice_on_one_task_counts_once() {
    let snap =
        snapshot(&["X +home +home", "Y +home"], &["x 2026-09-01 Z +home"]);
    let want = map([("home", count(2, 3))]);
    assert_eq!(names(&snap, Kind::Project), want);
}

#[test]
fn rolled_counts_keep_the_exact_case() {
    let snap = fixture();
    let projects = map([
        ("home", count(3, 4)),
        ("work", count(5, 10)),
        ("work.acme", count(2, 3)),
        ("work/q3", count(1, 1)),
        ("Work", count(1, 1)),
    ]);
    assert_eq!(names(&snap, Kind::Project), projects);
}

#[test]
fn a_name_only_in_the_done_file_is_not_listed() {
    let snap = fixture();
    assert_eq!(of(&snap, Kind::Project, "gone"), None);
    assert_eq!(of(&snap, Kind::Context, "gone"), None);
}

#[test]
fn a_parent_is_rolled_up_over_everything_below_it() {
    // 31.2
    let snap = fixture();
    assert_eq!(of(&snap, Kind::Project, "work"), Some(count(5, 10)));
    assert_eq!(of(&snap, Kind::Project, "work.acme"), Some(count(2, 3)));
}

#[test]
fn punctuation_after_a_name_makes_no_phantom_name() {
    let snap =
        snapshot(&["Ship it for +work.", "Plan +work.acme, @phone."], &[]);
    assert_eq!(
        names(&snap, Kind::Project),
        map([("work", count(2, 2)), ("work.acme", count(1, 1))])
    );
    assert_eq!(names(&snap, Kind::Context), map([("phone", count(1, 1))]));
}

#[test]
fn a_name_may_end_in_plus_signs() {
    let snap = snapshot(&["p +c++ @c++", "q +c++."], &[]);
    assert_eq!(names(&snap, Kind::Project), map([("c++", count(2, 2))]));
    assert_eq!(names(&snap, Kind::Context), map([("c++", count(1, 1))]));
}

#[test]
fn a_doubled_separator_makes_one_flat_name() {
    let snap = snapshot(&["p +a..b", "y +c./d"], &[]);
    assert_eq!(
        names(&snap, Kind::Project),
        map([("a..b", count(1, 1)), ("c./d", count(1, 1)),])
    );
}

#[test]
fn a_task_under_two_levels_counts_once_in_the_parent() {
    // 31.3
    let snap = snapshot(&["Twice +work +work.acme"], &[]);
    assert_eq!(of(&snap, Kind::Project, "work"), Some(count(1, 1)));
    let snap = snapshot(&["Twice +work +work +work/q3"], &[]);
    assert_eq!(of(&snap, Kind::Project, "work"), Some(count(1, 1)));
}

#[test]
fn a_parent_nobody_carries_is_listed_with_its_subtree() {
    let snap = snapshot(&["One +a.b.c"], &[]);
    let counts = snap.rolled_counts(Kind::Project);
    let names: Vec<_> = counts.keys().map(Vec::as_slice).collect();
    assert_eq!(names, [b"a".as_slice(), b"a.b", b"a.b.c"]);
    assert!(counts.values().all(|c| *c == count(1, 1)));
}
