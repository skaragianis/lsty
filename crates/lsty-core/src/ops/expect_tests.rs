use super::{
    Actor, PriPair, Target,
    expect::{Expect, squash_bytes},
    parse_one, pri,
    test_util::{folder, parts},
};
use crate::{
    errs::{Code, Field, Source},
    todotxt::Priority,
};

const FILE: &str = "Call client +work\nAdd CLI ls id:k3f9\nWrite docs\n";

/// Text and what squashing leaves.
const SQUASH: &[(&str, &str)] = &[
    ("", ""),
    ("   ", ""),
    ("a", "a"),
    (" a  b\t\tc\n", "a b c"),
    ("a\u{a0}b\u{3000}c", "a b c"),
];

#[test]
fn squash_collapses_whitespace_and_trims() {
    for (text, want) in SQUASH {
        assert_eq!(squash_bytes(text.as_bytes()), want.as_bytes(), "{text:?}");
    }
}

/// Guard text, exact, line, whether it holds.
type Row = (&'static str, bool, i64, bool);

const HOLDS: &[Row] = &[
    ("Call client", true, 1, true),
    ("Call  client", true, 1, true),
    ("  Call client ", true, 1, true),
    ("Call", true, 1, false),
    ("call client", true, 1, false),
    ("Call client +work", true, 1, false),
    ("Add CLI ls", true, 2, true),
    ("client", false, 1, true),
    ("client +work", false, 1, true),
    ("Call  client   +work", false, 1, true),
    ("CLIENT", false, 1, false),
    ("invoice", false, 1, false),
    ("id:k3f9", false, 2, true),
    ("docs", false, 3, true),
];

/// Line `nr` of the todo file, guarded.
fn guarded(nr: i64, text: &str, exact: bool) -> Target {
    let mut target = Target::todo(nr);
    target.set_expect(text, exact);
    target
}

fn set_a(target: Target) -> PriPair {
    PriPair {
        target,
        priority: Some(Priority::A),
    }
}

#[test]
fn the_guard_holds_as_the_rules_say() {
    for (text, exact, nr, holds) in HOLDS {
        let dir = folder(FILE, None);
        let target = guarded(*nr, text, *exact);
        let got = pri(&dir.env(), &[set_a(target)]);
        assert_eq!(got.is_ok(), *holds, "{text:?} exact={exact} on {nr}");
        if !holds {
            let err = got.unwrap_err();
            assert_eq!(err.code(), Code::ExpectMismatch);
            assert_eq!(dir.todo(), FILE, "nothing changed");
        }
    }
}

#[test]
fn a_mismatch_names_the_item_and_both_texts() {
    let dir = folder(FILE, None);
    let target = guarded(2, "Add", true);
    let err = pri(&dir.env(), &[set_a(target)]).unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::ExpectMismatch,
            "item 2 is \"Add CLI ls\", expected \"Add\"",
            ""
        )
    );
    let item = err.item().unwrap();
    assert_eq!(
        (item.nr.get(), item.source, item.id.as_deref()),
        (2, Source::Todo, Some("k3f9"))
    );
    assert_eq!(err.text("expected"), Some("Add"));
    assert_eq!(err.text("actual"), Some("Add CLI ls"));
}

#[test]
fn a_guarded_line_past_the_end_or_blank_is_a_mismatch() {
    for (file, nr) in [(FILE, 9), ("Call\n\nDocs\n", 2)] {
        let dir = folder(file, None);
        let target = guarded(nr, "x", false);
        let err = pri(&dir.env(), &[set_a(target)]).unwrap_err();
        assert_eq!(
            err.message(),
            format!("item {nr} does not exist, expected \"x\"")
        );
        assert_eq!(err.field("actual"), Some(&Field::Null));
        assert_eq!(err.place(), Some((nr, Source::Todo)));
    }
}

#[test]
fn an_unguarded_blank_line_is_still_gone_or_missing() {
    let dir = folder("Call\n\nDocs\n", None);
    let out = pri(&dir.env(), &[set_a(Target::todo(2))]).unwrap();
    assert!(out.no_op());
    let err = pri(&dir.env(), &[set_a(Target::todo(9))]).unwrap_err();
    assert_eq!(err.code(), Code::NoItem);
}

#[test]
fn the_second_guard_failing_changes_nothing() {
    let dir = folder(FILE, None);
    let pairs = [
        set_a(guarded(1, "Call", false)),
        set_a(guarded(3, "nope", false)),
    ];
    let err = pri(&dir.env(), &pairs).unwrap_err();
    assert_eq!(err.code(), Code::ExpectMismatch);
    assert_eq!(dir.todo(), FILE);
}

#[test]
fn a_guard_works_for_an_agent_too() {
    let dir = folder(FILE, None);
    let mut env = dir.env();
    env.actor = Actor::named("agent1").expect("actor");
    let ok = guarded(1, "Call client", true);
    assert!(pri(&env, &[set_a(ok)]).is_ok());
    let bad = guarded(3, "docs", true);
    assert_eq!(
        pri(&env, &[set_a(bad)]).unwrap_err().code(),
        Code::ExpectMismatch
    );
}

#[test]
fn bytes_that_are_not_utf8_stay_in_their_word() {
    assert_eq!(squash_bytes(b"  a\xff \t b\xfe\xfd  "), b"a\xff b\xfe\xfd");
    assert_eq!(squash_bytes(b"\xff"), b"\xff");
    assert_eq!(squash_bytes(b" \n "), b"");
}

#[test]
fn an_id_reference_takes_no_guard() {
    let mut target = parse_one("pri", "id:k3f9").expect("a reference");
    target.set_expect("Add", false);
    assert!(target.expect().is_none());
    assert_eq!(target.line(), None);
    let mut line = parse_one("pri", "done:2").expect("a reference");
    line.set_expect("Add", true);
    assert_eq!(line.line(), Some((Source::Done, 2)));
    assert_eq!(line.expect(), Some(&Expect::new("Add", true)));
}

#[test]
fn a_guard_holds_on_text_that_is_not_utf8() {
    let dir = folder(b"caf\xe9 A\nB\n", None);
    let mut line = Target::todo(1);
    line.set_expect(&b"caf\xe9 A"[..], true);
    assert!(pri(&dir.env(), &[set_a(line)]).is_ok());
    let mut stale = Target::todo(2);
    stale.set_expect(&b"caf\xe9 A"[..], true);
    let err = pri(&dir.env(), &[set_a(stale)]).unwrap_err();
    assert_eq!(err.code(), Code::ExpectMismatch);
    assert_eq!(err.message(), "item 2 is \"B\", expected \"caf\\xe9 A\"");
}
