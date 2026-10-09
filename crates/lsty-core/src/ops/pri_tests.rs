use std::fs;
use std::time::{Duration, SystemTime};

use super::test_util::{field, folder, parts, said};
use super::{PriPair, Target, depri, parse_letter, pri};
use crate::errs::{Code, Source};
use crate::todotxt::Priority;

fn pair(nr: i64, letter: &str) -> PriPair {
    PriPair {
        target: Target::todo(nr),
        letter: (!letter.is_empty())
            .then(|| parse_letter(letter).expect("a letter")),
    }
}

/// Name, file, pairs, file after, messages, whether nothing changed.
type Row = (
    &'static str,
    &'static str,
    &'static [(i64, &'static str)],
    &'static str,
    &'static [&'static str],
    bool,
);

const PRI: &[Row] = &[
    (
        "12.1 set",
        "Buy milk\n",
        &[(1, "A")],
        "(A) Buy milk\n",
        &["1 prioritized (A)"],
        false,
    ),
    (
        "12.2 change",
        "(A) Buy\n",
        &[(1, "B")],
        "(B) Buy\n",
        &["1 re-prioritized from (A) to (B)"],
        false,
    ),
    (
        "12.3 already",
        "(A) Buy\n",
        &[(1, "a")],
        "(A) Buy\n",
        &["1 already has priority (A)"],
        true,
    ),
    (
        "12.8 gone line is skipped",
        "Buy\n\nCall\n",
        &[(2, "A"), (3, "B")],
        "Buy\n\n(B) Call\n",
        &["2 already deleted", "3 prioritized (B)"],
        false,
    ),
    (
        "12.12 crlf and no final newline",
        "Buy\r\nCall\r\nMail",
        &[(1, "A")],
        "(A) Buy\r\nCall\r\nMail",
        &["1 prioritized (A)"],
        false,
    ),
    (
        "same target twice with one letter",
        "Buy\n",
        &[(1, "A"), (1, "A")],
        "(A) Buy\n",
        &["1 prioritized (A)"],
        false,
    ),
];

fn old_time() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn modified(path: &std::path::Path) -> SystemTime {
    fs::metadata(path).expect("stat").modified().expect("mtime")
}

#[test]
fn pri_rows() {
    for (name, todo, pairs, want, msgs, noop) in PRI {
        let dir = folder(todo, None);
        let file = fs::File::options()
            .write(true)
            .open(&dir.found.todo)
            .expect("open");
        file.set_modified(old_time()).expect("set mtime");
        drop(file);
        let pairs: Vec<PriPair> =
            pairs.iter().map(|(nr, l)| pair(*nr, l)).collect();
        let out = pri(&dir.env(), &pairs).expect(name);
        assert_eq!(dir.todo(), *want, "{name}");
        assert_eq!(out.messages(), said(msgs), "{name}");
        assert_eq!(out.no_op(), *noop, "{name}");
        assert_eq!(out.undo().is_none(), *noop, "{name}");
        let untouched = modified(&dir.found.todo) == old_time();
        assert_eq!(untouched, *noop, "{name}: modification time");
    }
}

#[test]
fn result_items() {
    let dir = folder("Buy milk\n", None);
    let out = pri(&dir.env(), &[pair(1, "A")]).unwrap();
    let after: Vec<&[u8]> = out
        .items_after()
        .map(|i| out.after().line(i).raw())
        .collect();
    let before: Vec<&[u8]> = out
        .items_before()
        .map(|i| out.read().line(i).raw())
        .collect();
    assert_eq!(after, [b"(A) Buy milk".as_slice()], "12.1 items");
    assert_eq!(before, [b"Buy milk".as_slice()], "12.1 before");
    assert!(!out.renumbered(), "12.23");
}

/// Name, file, file after, message, whether nothing changed.
type DepriRow = (&'static str, &'static str, &'static str, &'static str, bool);

const DEPRI: &[DepriRow] = &[
    (
        "12.4 remove",
        "(A) 2026-09-20 Buy\n",
        "2026-09-20 Buy\n",
        "1 deprioritized",
        false,
    ),
    ("12.5 none", "Buy\n", "Buy\n", "1 has no priority", true),
];

#[test]
fn depri_rows() {
    for (name, todo, want, msg, noop) in DEPRI {
        let dir = folder(todo, None);
        let out = depri(&dir.env(), &[Target::todo(1)]).expect(name);
        assert_eq!(dir.todo(), *want, "{name}");
        assert_eq!(out.messages(), said(&[msg]), "{name}");
        assert_eq!(out.no_op(), *noop, "{name}");
    }
}

#[test]
fn depri_twice_acts_once() {
    let dir = folder("(A) Buy\n", None);
    let targets = [Target::todo(1), Target::todo(1)];
    let out = depri(&dir.env(), &targets).unwrap();
    assert_eq!(out.messages(), said(&["1 deprioritized"]));
    assert_eq!(dir.todo(), "Buy\n");
}

#[test]
fn depri_done() {
    let dir = folder("x 2026-09-28 (A) Fix\n", None);
    let err = depri(&dir.env(), &[Target::todo(1)]).unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            "1 is done; reopen it first",
            "use lsty reopen 1"
        )
    );
    assert_eq!(field(&err, "state").as_deref(), Some("done"));
}

#[test]
fn conflicting_pairs() {
    let rows = [
        [pair(1, ""), pair(1, "B")],
        [pair(1, "B"), pair(1, "")],
        [pair(1, "A"), pair(1, "B")],
    ];
    for pairs in rows {
        let dir = folder("(A) Buy\n", None);
        let err = pri(&dir.env(), &pairs).unwrap_err();
        assert_eq!(err.code(), Code::Usage, "{pairs:?}");
        assert_eq!(dir.todo(), "(A) Buy\n", "{pairs:?}");
    }
}

#[test]
fn pairs_set_and_clear() {
    let dir = folder("(C) Buy\nCall\n", None);
    let out = pri(&dir.env(), &[pair(1, ""), pair(2, "A")]).unwrap();
    assert_eq!(dir.todo(), "Buy\n(A) Call\n");
    assert_eq!(
        out.messages(),
        said(&["1 deprioritized", "2 prioritized (A)"])
    );
    let undo = out.undo().expect("undo");
    assert_eq!(undo.files.len(), 1);
    assert_eq!(undo.files[0].data, b"Buy\n(A) Call\n");
}

#[test]
fn an_empty_letter_is_invalid() {
    let err = parse_letter("").unwrap_err();
    assert_eq!(err.code(), Code::InvalidValue);
}

#[test]
fn letters_in_either_case() {
    assert_eq!(parse_letter("b").ok(), Priority::new(b'B'));
    assert_eq!(parse_letter("Z").ok(), Priority::new(b'Z'));
    for bad in ["1", "ab", "(A)", " A"] {
        let err = parse_letter(bad).unwrap_err();
        let message = format!("priority {bad} is not a letter from A to Z");
        assert_eq!(parts(&err), (Code::InvalidValue, message.as_str(), ""));
        assert_eq!(field(&err, "field").as_deref(), Some("priority"));
        assert_eq!(field(&err, "value").as_deref(), Some(bad));
    }
}

/// Name, file, done file, target, letter, code, message, hint, state.
type ErrRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    (Source, i64),
    &'static str,
    Code,
    &'static str,
    &'static str,
    Option<&'static str>,
);

const ERRORS: &[ErrRow] = &[
    (
        "12.10 done item",
        "x 2026-09-28 Fix\n",
        None,
        (Source::Todo, 1),
        "A",
        Code::WrongState,
        "1 is done; reopen it first",
        "use lsty reopen 1",
        Some("done"),
    ),
    (
        "12.11 done file",
        "Buy\n",
        Some("x 2026-09-28 Fix\n"),
        (Source::Done, 1),
        "A",
        Code::WrongState,
        "done:1 is read-only history",
        "done:1 is archived; use restore",
        Some("done"),
    ),
    (
        "12.9 nr zero",
        "Buy\n",
        None,
        (Source::Todo, 0),
        "A",
        Code::NoItem,
        "no item 0",
        "",
        None,
    ),
    (
        "past the end",
        "Buy\n",
        None,
        (Source::Todo, 2),
        "A",
        Code::NoItem,
        "no item 2",
        "",
        None,
    ),
];

#[test]
fn pri_errors() {
    for (name, todo, done, (source, nr), letter, code, msg, hint, state) in
        ERRORS
    {
        let dir = folder(todo, *done);
        let target = Target::at(*source, *nr);
        let pairs = [PriPair {
            target,
            letter: (!letter.is_empty())
                .then(|| parse_letter(letter).expect("a letter")),
        }];
        let err = pri(&dir.env(), &pairs).unwrap_err();
        assert_eq!(parts(&err), (*code, *msg, *hint), "{name}");
        assert_eq!(field(&err, "state").as_deref(), *state, "{name}");
        assert_eq!(dir.todo(), *todo, "{name}: file");
    }
}

#[test]
fn invalid_value_names_the_field() {
    let err = parse_letter("AB").unwrap_err();
    assert_eq!(field(&err, "field").as_deref(), Some("priority"));
    assert_eq!(field(&err, "value").as_deref(), Some("AB"));
}

#[test]
fn a_missing_later_target_changes_nothing() {
    let dir = folder("a\nb\nc\n", None);
    let err = pri(&dir.env(), &[pair(1, "A"), pair(9, "B")]).unwrap_err();
    assert_eq!(parts(&err).0, Code::NoItem, "12.7");
    assert_eq!(dir.todo(), "a\nb\nc\n", "12.7");
}

#[test]
fn different_letters_for_one_target() {
    let dir = folder("Buy\n", None);
    let err = pri(&dir.env(), &[pair(1, "A"), pair(1, "B")]).unwrap_err();
    assert_eq!(err.code(), Code::Usage);
    assert_eq!(dir.todo(), "Buy\n");
}
