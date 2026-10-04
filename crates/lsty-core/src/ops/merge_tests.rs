use super::merge::{Merged, merge};

const BASE: &str = "(A) 2026-09-20 Call client +work id:c1";

/// Name, the draft, the disk, the merged line and the clashes.
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const ROWS: &[Row] = &[
    (
        "40.1 retitle with a claim on disk",
        "(A) 2026-09-20 Ring client +work id:c1",
        "(A) 2026-09-20 Call client +work id:c1 started:2026-09-29T0900Z owner:agent1",
        "(A) 2026-09-20 Ring client +work id:c1 started:2026-09-29T0900Z owner:agent1",
        &[],
    ),
    (
        "40.2 both add a token",
        "(A) 2026-09-20 Call client +work id:c1 @phone",
        "(A) 2026-09-20 Call client +work id:c1 +urgent",
        "(A) 2026-09-20 Call client +work id:c1 +urgent @phone",
        &[],
    ),
    (
        "40.3 draft removes a project, disk adds a date",
        "(A) 2026-09-20 Call client id:c1",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        "(A) 2026-09-20 Call client id:c1 due:2026-10-01",
        &[],
    ),
    (
        "40.4 both add the same token",
        "(A) 2026-09-20 Call client +work id:c1 @phone",
        "(A) 2026-09-20 Call client +work id:c1 @phone",
        "(A) 2026-09-20 Call client +work id:c1 @phone",
        &[],
    ),
    (
        "40.5 priority against a retitle",
        "(B) 2026-09-20 Call client +work id:c1",
        "(A) 2026-09-20 Call the client +work id:c1",
        "(B) 2026-09-20 Call the client +work id:c1",
        &[],
    ),
    (
        "40.6 both retitle",
        "(A) 2026-09-20 Ring client +work id:c1",
        "(A) 2026-09-20 Call the client +work id:c1",
        "(A) 2026-09-20 Ring client +work id:c1",
        &["text (\"Call the client\")"],
    ),
    (
        "40.7 both retitle the same",
        "(A) 2026-09-20 Ring client +work id:c1",
        "(A) 2026-09-20 Ring client +work id:c1",
        "(A) 2026-09-20 Ring client +work id:c1",
        &[],
    ),
    (
        "40.8 both set a priority",
        "(B) 2026-09-20 Call client +work id:c1",
        "(C) 2026-09-20 Call client +work id:c1",
        "(B) 2026-09-20 Call client +work id:c1",
        &["priority (C)"],
    ),
    (
        "40.9 both add a different due date",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-05",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        &["due (2026-10-05)"],
    ),
    (
        "40.10 different keys do not clash",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        "(A) 2026-09-20 Call client +work id:c1 t:2026-09-30",
        "(A) 2026-09-20 Call client +work id:c1 t:2026-09-30 due:2026-10-01",
        &[],
    ),
    (
        "40.11 completed meanwhile",
        "(A) 2026-09-20 Ring client +work id:c1",
        "x 2026-09-29 2026-09-20 Call client +work id:c1 pri:A",
        "x 2026-09-29 2026-09-20 Ring client +work id:c1 pri:A",
        &[],
    ),
    (
        "40.12 an id given meanwhile is kept",
        "(A) 2026-09-20 Ring client +work",
        "(A) 2026-09-20 Call client +work id:k3f9",
        "(A) 2026-09-20 Ring client +work id:k3f9",
        &[],
    ),
    (
        "40.13 words moved around a token",
        "(A) 2026-09-20 Call +work client id:c1",
        "(A) 2026-09-20 Call client +work id:c1 @x",
        "(A) 2026-09-20 Call client +work id:c1 @x",
        &[],
    ),
    (
        "40.15 no free text left on disk",
        "(A) 2026-09-20 Ring client +work id:c1",
        "(A) 2026-09-20 +work id:c1",
        "(A) 2026-09-20 Ring client +work id:c1",
        &["text (\"\")"],
    ),
    (
        "a due date and a tag against another due date",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01 +x",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-02",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01 +x",
        &["due (2026-10-02)"],
    ),
    (
        "both change the same key to the same value",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01",
        &[],
    ),
    (
        "repeatable keys never clash",
        "(A) 2026-09-20 Call client +work id:c1 dep:a",
        "(A) 2026-09-20 Call client +work id:c1 dep:b",
        "(A) 2026-09-20 Call client +work id:c1 dep:b dep:a",
        &[],
    ),
    (
        "a due date the draft removed and disk changed",
        "(A) 2026-09-20 Call client +work id:c1",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-05",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-05",
        &[],
    ),
];

fn merged(base: &str, draft: &str, disk: &str) -> Merged {
    merge(base.as_bytes(), draft.as_bytes(), disk.as_bytes()).unwrap()
}

#[test]
fn the_changes_of_the_draft_are_applied_to_the_disk() {
    for (name, draft, disk, want, clashes) in ROWS {
        let got = merged(BASE, draft, disk);
        assert_eq!(String::from_utf8_lossy(&got.line), *want, "{name}");
        let names: Vec<String> = got
            .clashes
            .iter()
            .map(|c| format!("{} ({})", c.part, c.disk))
            .collect();
        assert_eq!(names, *clashes, "{name}");
    }
}

/// Name, the base, the draft, the disk and the merged line.
type Multiset = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const MULTISETS: &[Multiset] = &[
    (
        "40.14 one of two projects removed",
        "Call +work +work",
        "Call +work",
        "Call +work +work",
        "Call +work",
    ),
    (
        "a token added twice is one change each",
        "Call",
        "Call +x +x",
        "Call",
        "Call +x +x",
    ),
    ("the priority is raised", "(B) ", "(A) ", "(B) ", "(A) "),
    ("the priority is kept", "(B) ", "(B) ", "(B) ", "(B) "),
    ("the priority is dropped", "(B) ", "", "(B) ", ""),
];

#[test]
fn tokens_are_counted_as_a_multiset() {
    for (name, base, draft, disk, want) in MULTISETS {
        let got = merged(base, draft, disk);
        assert_eq!(String::from_utf8_lossy(&got.line), *want, "{name}");
    }
}

#[test]
fn a_word_that_would_become_a_stamp_refuses_the_merge() {
    let got = merge(
        b"+a Call 2026-10-01",
        b"Call 2026-10-01",
        b"+a 2026-10-01 Call",
    );
    assert!(got.is_err());
}

#[test]
fn a_clash_quotes_invalid_bytes_of_the_disk_text_one_by_one() {
    let draft = b"(A) 2026-09-20 Ring client +work id:c1";
    let disk = b"(A) 2026-09-20 Caf\xe9 client +work id:c1";
    let got = merge(BASE.as_bytes(), draft, disk).unwrap();
    let texts: Vec<&str> = got.clashes.iter().map(|c| &*c.disk).collect();
    assert_eq!(texts, ["\"Caf\\xe9 client\""]);
}
