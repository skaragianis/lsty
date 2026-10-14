use super::merge::{Merged, StampWord, merge};

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
        "40.1a a quoted lookalike is not the project the draft drops",
        "(A) 2026-09-20 Call `a +work b` id:c1",
        "(A) 2026-09-20 Call `a +work b` +work id:c1",
        "(A) 2026-09-20 Call `a +work b` id:c1",
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
        "40.9a both add a different ended",
        "(A) 2026-09-20 Call client +work id:c1 ended:2026-10-01T0900Z",
        "(A) 2026-09-20 Call client +work id:c1 ended:2026-10-05T0900Z",
        "(A) 2026-09-20 Call client +work id:c1 ended:2026-10-01T0900Z",
        &["ended (2026-10-05T0900Z)"],
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
            .map(|c| format!("{} ({})", c.part.name(), c.disk))
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

/// Name, the base, the draft and the disk.
type Stamped = (&'static str, &'static str, &'static str, &'static str);

/// The draft's text, or dropping the priority in front of it, would
/// start the merged line with a stamp that the disk's line does not have.
const STAMPED_TEXT: &[Stamped] = &[
    ("a done mark over a word", "+p a", "+p x marks", "foo +p"),
    ("a done mark with no word", "+p a", "+p x marks", "+p"),
    ("a priority", "+p a", "+p (B) call", "foo +p"),
    ("a creation date", "+p a", "+p 2026-10-01 call", "foo +p"),
    (
        "a completion date",
        "+p a",
        "+p 2026-10-01 call",
        "x foo +p",
    ),
    (
        "a done mark under a dropped priority",
        "(A) a +p",
        "+p x call",
        "(B) a +p",
    ),
    (
        "a priority under a dropped one",
        "(A) a +p",
        "+p (C) call",
        "(B) a +p",
    ),
];

#[test]
fn text_that_would_become_a_stamp_refuses_the_merge() {
    for (name, base, draft, disk) in STAMPED_TEXT {
        let got = merge(base.as_bytes(), draft.as_bytes(), disk.as_bytes());
        assert_eq!(got, Err(StampWord), "{name}");
    }
}

#[test]
fn text_after_the_disk_stamps_merges() {
    let got = merged("+p a", "+p 2026-10-01 call", "(A) 2026-09-20 foo +p");
    assert_eq!(got.line, b"(A) 2026-09-20 2026-10-01 call +p");
}

#[test]
fn a_clash_quotes_invalid_bytes_of_the_disk_text_one_by_one() {
    let draft = b"(A) 2026-09-20 Ring client +work id:c1";
    let disk = b"(A) 2026-09-20 Caf\xe9 client +work id:c1";
    let got = merge(BASE.as_bytes(), draft, disk).unwrap();
    let texts: Vec<&str> = got.clashes.iter().map(|c| &*c.disk).collect();
    assert_eq!(texts, ["\"Caf\\xe9 client\""]);
}

/// Name, the base, the draft, the disk, the merged line and the clashes.
type Dated = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const DATED: &[Dated] = &[
    (
        "a date against a retitle",
        BASE,
        "(A) 2026-09-25 Call client +work id:c1",
        "(A) 2026-09-20 Call the client +work id:c1",
        "(A) 2026-09-25 Call the client +work id:c1",
        &[],
    ),
    (
        "both change the date",
        BASE,
        "(A) 2026-09-25 Call client +work id:c1",
        "(A) 2026-09-22 Call client +work id:c1",
        "(A) 2026-09-25 Call client +work id:c1",
        &["creation date (2026-09-22)"],
    ),
    (
        "a date and a token against a retitle",
        BASE,
        "(A) 2026-09-25 Call client +work id:c1 +home",
        "(A) 2026-09-20 Call the client +work id:c1",
        "(A) 2026-09-25 Call the client +work id:c1 +home",
        &[],
    ),
    (
        "a date made into text against a token",
        BASE,
        "(A) 2026-09-20x Call client +work id:c1",
        "(A) 2026-09-20 Call client +work id:c1 +home",
        "(A) 2026-09-20x Call client +work id:c1 +home",
        &[],
    ),
    (
        "text made into a date against a token",
        "2026-09-2x Call client +work id:c1",
        "2026-09-20 Call client +work id:c1",
        "2026-09-2x Call client +work id:c1 +home",
        "2026-09-20 Call client +work id:c1 +home",
        &[],
    ),
    (
        "a completion date deleted against a token",
        "x 2026-09-28 2026-09-01 Delta id:c1",
        "x 2026-09-01 Delta id:c1",
        "x 2026-09-28 2026-09-01 Delta id:c1 +home",
        "x 2026-09-01 Delta id:c1 +home",
        &[],
    ),
    (
        "a date typed onto a line without one",
        "Call client id:c1",
        "2026-09-10 Call client id:c1",
        "Call client id:c1 +home",
        "2026-09-10 Call client id:c1 +home",
        &[],
    ),
    (
        "a date deleted while the item was completed",
        "2026-09-01 Delta id:c1",
        "Delta id:c1",
        "x 2026-09-28 2026-09-01 Delta id:c1",
        "x 2026-09-28 Delta id:c1",
        &[],
    ),
    (
        "a clash on a date and a retitle",
        BASE,
        "(A) 2026-09-25 Ring client +work id:c1",
        "(A) 2026-09-22 Call the client +work id:c1",
        "(A) 2026-09-25 Ring client +work id:c1",
        &["creation date (2026-09-22)", "text (\"Call the client\")"],
    ),
    (
        "a completion date typed, reopened meanwhile",
        "x Delta id:c1",
        "x 2026-09-27 Delta id:c1",
        "Delta id:c1 +home",
        "Delta id:c1 +home",
        &[],
    ),
    (
        "a completion date changed, reopened meanwhile",
        "x 2026-09-28 Delta id:c1",
        "x 2026-09-27 Delta id:c1",
        "Delta id:c1",
        "Delta id:c1",
        &[],
    ),
    (
        "a completion date changed, reopened with a creation date",
        "x 2026-09-28 2026-09-01 Delta id:c1",
        "x 2026-09-27 2026-09-01 Delta id:c1",
        "2026-09-01 Delta id:c1",
        "2026-09-01 Delta id:c1",
        &[],
    ),
    (
        "both change the completion date",
        "x 2026-09-28 Delta id:c1",
        "x 2026-09-27 Delta id:c1",
        "x 2026-09-29 Delta id:c1",
        "x 2026-09-27 Delta id:c1",
        &["completion date (2026-09-29)"],
    ),
    (
        "a creation date onto a done line without a completion date",
        "Delta id:c1",
        "2026-09-27 Delta id:c1",
        "x Delta id:c1",
        "x 2026-09-27 Delta id:c1",
        &[],
    ),
    (
        "a completion date deleted, reopened meanwhile",
        "x 2026-09-28 2026-09-01 Delta id:c1",
        "x 2026-09-01 Delta id:c1",
        "2026-09-01 Delta id:c1",
        "2026-09-01 Delta id:c1",
        &[],
    ),
];

#[test]
fn the_dates_merge_like_the_priority() {
    for (name, base, draft, disk, want, clashes) in DATED {
        let got = merged(base, draft, disk);
        assert_eq!(String::from_utf8_lossy(&got.line), *want, "{name}");
        let said: Vec<_> = got
            .clashes
            .iter()
            .map(|c| format!("{} ({})", c.part.name(), c.disk))
            .collect();
        assert_eq!(said, *clashes, "{name}");
    }
}
