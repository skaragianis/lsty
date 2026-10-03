use super::expect::squash_bytes;
use super::test_util::{field, folder, parts};
use super::{Actor, PriPair, Target, pri, squash};
use crate::errs::{Code, Field, Source};
use crate::todotxt::Owner;

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
        assert_eq!(squash(text), *want, "{text:?}");
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

fn set_a(target: Target) -> PriPair {
    PriPair {
        target,
        letter: "A".to_owned(),
    }
}

#[test]
fn the_guard_holds_as_the_rules_say() {
    for (text, exact, nr, holds) in HOLDS {
        let dir = folder(FILE, None);
        let target = Target::todo(*nr).expecting(*text, *exact);
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
    let target = Target::todo(2).expecting("Add", true);
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
    assert_eq!(field(&err, "expected").as_deref(), Some("Add"));
    assert_eq!(field(&err, "actual").as_deref(), Some("Add CLI ls"));
}

#[test]
fn a_guarded_line_past_the_end_or_blank_is_a_mismatch() {
    for (file, nr) in [(FILE, 9), ("Call\n\nDocs\n", 2)] {
        let dir = folder(file, None);
        let target = Target::todo(nr).expecting("x", false);
        let err = pri(&dir.env(), &[set_a(target)]).unwrap_err();
        assert_eq!(
            err.message(),
            format!("item {nr} does not exist, expected \"x\"")
        );
        let actual = err.fields().find(|(k, _)| *k == "actual");
        assert_eq!(actual.map(|(_, v)| v), Some(&Field::Null));
        assert_eq!(err.place(), Some((nr, Source::Todo)));
    }
}

#[test]
fn an_unguarded_blank_line_is_still_gone_or_missing() {
    let dir = folder("Call\n\nDocs\n", None);
    let out = pri(&dir.env(), &[set_a(Target::todo(2))]).unwrap();
    assert!(out.no_op);
    let err = pri(&dir.env(), &[set_a(Target::todo(9))]).unwrap_err();
    assert_eq!(err.code(), Code::NoItem);
}

#[test]
fn the_second_guard_failing_changes_nothing() {
    let dir = folder(FILE, None);
    let pairs = [
        set_a(Target::todo(1).expecting("Call", false)),
        set_a(Target::todo(3).expecting("nope", false)),
    ];
    let err = pri(&dir.env(), &pairs).unwrap_err();
    assert_eq!(err.code(), Code::ExpectMismatch);
    assert_eq!(dir.todo(), FILE);
}

#[test]
fn a_guard_works_for_an_agent_too() {
    let dir = folder(FILE, None);
    let mut env = dir.env();
    env.actor = Actor::Agent(Owner::new("agent1").unwrap());
    let ok = Target::todo(1).expecting("Call client", true);
    assert!(pri(&env, &[set_a(ok)]).is_ok());
    let bad = Target::todo(3).expecting("docs", true);
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
