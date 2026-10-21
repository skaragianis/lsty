use super::{SortField, test_util::snapshot};
use crate::errs::Source;

const TODO: &[&str] = &[
    "(B) 2026-09-21 beta due:2026-09-27",
    "Zed",
    "x 2026-09-28 2026-09-20 Alpha pri:A",
    "(A) 2026-09-20 apple due:2026-09-29",
    "alpha again",
];
const DONE: &[&str] = &["x 2026-09-01 2026-08-30 Aardvark"];

/// The field and the expected order: line numbers, `d` for the done file.
type Row = (&'static str, &'static [&'static str]);

const ROWS: &[Row] = &[
    ("manual", &["1", "2", "3", "4", "5", "d1"]),
    ("file", &["1", "2", "3", "4", "5", "d1"]),
    ("priority", &["3", "4", "1", "2", "5", "d1"]),
    ("due", &["1", "4", "2", "3", "5", "d1"]),
    ("created", &["3", "4", "1", "2", "5", "d1"]),
    ("alpha", &["5", "3", "4", "1", "2", "d1"]),
    ("done", &["3", "d1", "1", "2", "4", "5"]),
];

/// The order `field` gives the tasks of `todo` and `done`.
fn sorted(todo: &[&str], done: &[&str], field: SortField) -> Vec<String> {
    let snap = snapshot(todo, done);
    let mut tasks: Vec<_> = snap.tasks().iter().rev().collect();
    snap.sort(&mut tasks, SortField::File);
    snap.sort(&mut tasks, field);
    tasks
        .iter()
        .map(|i| match i.source() {
            Source::Todo => i.nr().to_string(),
            Source::Done => format!("d{}", i.nr()),
        })
        .collect()
}

#[test]
fn orders_by_each_field() {
    for (name, want) in ROWS {
        let field = SortField::parse(name).expect(name);
        assert_eq!(&sorted(TODO, DONE, field), want, "{name}");
    }
}

#[test]
fn done_mixes_both_files_newest_first() {
    let todo = [
        "x 2026-10-02 a",
        "open",
        "x 2026-10-02 b ended:2026-10-02T0900Z",
        "x 2026-10-01 c ended:2026-10-01T2300Z",
        "x 2026-10-02 d ended:2026-10-02T1000Z",
    ];
    let done = [
        "x 2026-10-03 e",
        "unmarked",
        "x 2026-10-02 g ended:2026-10-02T0930Z",
        "x h",
    ];
    let want = ["d1", "5", "d3", "3", "1", "4", "d2", "d4", "2"];
    assert_eq!(sorted(&todo, &done, SortField::Done), want);
}

#[test]
fn field_names() {
    assert_eq!(SortField::parse("bogus"), None);
    for name in SortField::NAMES {
        assert!(SortField::parse(name).is_some(), "{name}");
    }
}
