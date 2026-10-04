use super::pin::follow;
use super::test_util::{Dir, field, folder};
use super::{AddParams, Grant, Pinned, Seen, add, edit_typed};
use crate::errs::{Code, Nr, Source};
use crate::list::Snapshot;
use crate::todotxt::File;

/// The reading of the files of `dir` that held `todo`.
fn snapshot(dir: &Dir, todo: &str) -> Snapshot {
    Snapshot::build(
        dir.found.clone(),
        File::parse(todo.as_bytes()),
        File::default(),
        &dir.env().params(),
    )
}

/// The grant of `id` to line `nr` of `seen`, picked in a reading of
/// the files of `dir` that held `seen`.
fn grant(dir: &Dir, seen: &str, nr: u32, id: &str) -> Grant {
    let snap = snapshot(dir, seen);
    let item = snap.at(Source::Todo, Nr::new(nr).unwrap()).unwrap();
    Grant::new(&snap, &Seen::of(&snap), item.item_id(), id.to_owned()).unwrap()
}

/// Name, the file, the file the grant was picked in, the line it was
/// picked on, the file after, and the refusal.
type Row = (
    &'static str,
    &'static str,
    &'static str,
    u32,
    &'static str,
    Option<Code>,
);

const SAME: &str = "Call client\nWrite docs +spec01\n";

const ROWS: &[Row] = &[
    (
        "39.24 id on commit",
        SAME,
        SAME,
        2,
        "Call client dep:t001\nWrite docs +spec01 id:t001\n",
        None,
    ),
    (
        "moved up",
        "Call client\nWrite docs +spec01\n",
        "Intro\nCall client\nWrite docs +spec01\n",
        3,
        "Call client dep:t001\nWrite docs +spec01 id:t001\n",
        None,
    ),
    (
        "39.30 id given meanwhile",
        "Call client\nWrite docs +spec01 id:zz\n",
        SAME,
        2,
        "Call client\nWrite docs +spec01 id:zz\n",
        Some(Code::WrongState),
    ),
    (
        "changed meanwhile",
        "Call client\nWrite docs +spec01 more\n",
        SAME,
        2,
        "Call client\nWrite docs +spec01 more\n",
        Some(Code::ExpectMismatch),
    ),
    (
        "same text with another stamp",
        "Call client\nWrite docs +spec02\n",
        SAME,
        2,
        "Call client\nWrite docs +spec02\n",
        Some(Code::ExpectMismatch),
    ),
    (
        "gone",
        "Call client\n",
        SAME,
        2,
        "Call client\n",
        Some(Code::NoItem),
    ),
];

#[test]
fn a_typed_edit_gives_the_id_in_the_same_write() {
    for (name, disk, seen, nr, want, refused) in ROWS {
        let dir = folder(disk, None);
        let g = grant(&dir, seen, *nr, "t001");
        let at = Pinned::fixed(Nr::new(1).unwrap());
        let got = edit_typed(
            &dir.env(),
            &at,
            b"Call client",
            b"Call client dep:t001",
            &[g],
        );
        match (got, refused) {
            (Ok(out), None) => {
                assert!(out.undo().is_some(), "{name}");
            }
            (Err(e), Some(code)) => {
                assert_eq!(e.code(), *code, "{name}");
                assert_eq!(field(&e, "chosen").as_deref(), Some("t001"));
            }
            (Ok(_), Some(code)) => panic!("{name}: wanted {code:?}"),
            (Err(e), None) => panic!("{name}: {}", e.message()),
        }
        assert_eq!(dir.todo(), *want, "{name}");
    }
}

#[test]
fn an_id_given_meanwhile_is_named_in_the_refusal() {
    let dir = folder("Call client\nWrite docs +spec01 id:zz\n", None);
    let g = grant(&dir, SAME, 2, "t001");
    let err = edit_typed(
        &dir.env(),
        &Pinned::fixed(Nr::new(1).unwrap()),
        b"Call client",
        b"Call client dep:t001",
        &[g],
    )
    .unwrap_err();
    assert_eq!(err.message(), "the prerequisite has id:zz now");
    assert_eq!(field(&err, "id").as_deref(), Some("zz"));
}

#[test]
fn a_changed_item_is_named_with_both_lines() {
    let dir = folder("Call client\nWrite docs +spec01 more\n", None);
    let g = grant(&dir, SAME, 2, "t001");
    let err = edit_typed(
        &dir.env(),
        &Pinned::fixed(Nr::new(1).unwrap()),
        b"Call client",
        b"Call client dep:t001",
        &[g],
    )
    .unwrap_err();
    assert_eq!(
        err.message(),
        "item 2 is \"Write docs +spec01 more\", \
         expected \"Write docs +spec01\""
    );
}

#[test]
fn a_new_item_gives_the_id_too() {
    let dir = folder("Write docs\n", None);
    let g = grant(&dir, "Write docs\n", 1, "t001");
    let params = AddParams {
        texts: vec!["Call client dep:t001".to_owned()],
        grants: vec![g],
        ..AddParams::default()
    };
    add(&dir.env(), &params).unwrap();
    assert_eq!(
        dir.todo(),
        "Write docs id:t001\n2026-09-29 Call client dep:t001\n"
    );
}

/// Name, the lines seen, the lines now, the index, where it is.
type Pin = (
    &'static str,
    &'static str,
    &'static str,
    usize,
    Option<usize>,
);

const PINS: &[Pin] = &[
    ("a line that maps", "a|b|c", "x|a|b|c", 1, Some(2)),
    ("changed in place", "a|b|c", "a|B|c", 1, Some(1)),
    (
        "pairs with the first new line",
        "a|b|c",
        "a|B|N|c",
        1,
        Some(1),
    ),
    ("a deleted line leaves a blank", "a|b|c", "a||c", 1, None),
    ("a deleted line, compacted", "a|b|c", "a|c", 1, None),
    ("a deleted first line", "a|b", "b", 0, None),
    (
        "after a compacted blank",
        "a||b|c",
        "a|B owner:x|c",
        2,
        Some(1),
    ),
    ("past the lines seen", "a", "a", 3, None),
];

fn bars(text: &str) -> Vec<&str> {
    text.split('|').collect()
}

#[test]
fn a_line_is_followed_to_where_it_stands_now() {
    for (name, old, now, at, want) in PINS {
        let got = follow(&bars(old), &bars(now), *at);
        assert_eq!(got, *want, "{name}");
    }
}

#[test]
fn a_pinned_line_is_edited_where_it_stands_now() {
    let dir = folder("Intro\nBeta\n", None);
    let seen = Seen::of(&snapshot(&dir, "Alpha\nIntro\nBeta\n"));
    let line = Pinned::new(Nr::new(2).unwrap(), seen);
    edit_typed(&dir.env(), &line, b"Intro", b"Intro more", &[]).unwrap();
    assert_eq!(dir.todo(), "Intro more\nBeta\n");
}

#[test]
fn a_pinned_line_that_is_gone_is_no_item() {
    let dir = folder("Intro\n", None);
    let seen = Seen::of(&snapshot(&dir, "Intro\nCall client\n"));
    let line = Pinned::new(Nr::new(2).unwrap(), seen);
    let err = edit_typed(&dir.env(), &line, b"Call client", b"Call", &[])
        .unwrap_err();
    assert_eq!(err.code(), Code::NoItem);
}
