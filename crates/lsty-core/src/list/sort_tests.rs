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
];

#[test]
fn orders_by_each_field() {
    let snap = snapshot(TODO, DONE);
    for (name, want) in ROWS {
        let field = SortField::parse(name).expect(name);
        let mut items: Vec<_> = snap.items().iter().rev().collect();
        snap.sort(&mut items, SortField::File);
        snap.sort(&mut items, field);
        let got: Vec<String> = items
            .iter()
            .map(|i| match i.source() {
                Source::Todo => i.nr().to_string(),
                Source::Done => format!("d{}", i.nr()),
            })
            .collect();
        assert_eq!(&got, want, "{name}");
    }
}

#[test]
fn field_names() {
    assert_eq!(SortField::parse("bogus"), None);
    for name in SortField::NAMES {
        assert!(SortField::parse(name).is_some(), "{name}");
    }
}
