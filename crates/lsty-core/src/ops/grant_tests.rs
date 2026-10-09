use super::test_util::{Dir, folder, snapshot, typed};
use super::{AddParams, Grant, Seen, add};
use crate::errs::{Code, Nr, Source};
use crate::list::Snapshot;
use crate::todotxt::Id;

/// The grant of `id` to line `nr` of `seen`, picked in a reading of
/// the files of `dir` that held `seen`.
fn grant(dir: &Dir, seen: &str, nr: u32, id: &str) -> Grant {
    let snap = snapshot(dir, seen.as_bytes(), b"");
    let item = snap.at(Source::Todo, Nr::new(nr).unwrap()).unwrap();
    Grant::new(
        &snap,
        &Seen::of(&snap),
        item.item_id(),
        Id::new(id).unwrap(),
    )
    .unwrap()
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
        let got = typed(
            &dir,
            Nr::new(1).unwrap(),
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
                assert_eq!(e.text("chosen"), Some("t001"));
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
    let err = typed(
        &dir,
        Nr::new(1).unwrap(),
        b"Call client",
        b"Call client dep:t001",
        &[g],
    )
    .unwrap_err();
    assert_eq!(err.message(), "the prerequisite has id:zz now");
    assert_eq!(err.text("id"), Some("zz"));
}

#[test]
fn a_changed_item_is_named_with_both_lines() {
    let dir = folder("Call client\nWrite docs +spec01 more\n", None);
    let g = grant(&dir, SAME, 2, "t001");
    let err = typed(
        &dir,
        Nr::new(1).unwrap(),
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

#[test]
fn a_grant_tells_its_id_line_and_text_and_where_it_moved() {
    let dir = folder("A\nB +p\n", None);
    let mut g = grant(&dir, "A\nB +p\n", 2, "t007");
    assert_eq!(g.id().as_str(), "t007");
    assert_eq!(g.nr(), Nr::new(2).unwrap());
    assert_eq!(g.base(), b"B +p");
    g.moved_to(Nr::new(5).unwrap());
    assert_eq!(g.nr(), Nr::new(5).unwrap());
}

#[test]
fn a_grant_needs_the_lines_of_the_same_reading() {
    let dir = folder("A\nB\n", None);
    let one = snapshot(&dir, b"A\nB\n", b"");
    let other = snapshot(&dir, b"A\nB\n", b"");
    let seen = Seen::of(&one);
    let grant = |snap: &Snapshot| {
        let item = snap.at(Source::Todo, Nr::new(1).unwrap()).unwrap();
        Grant::new(snap, &seen, item.item_id(), Id::new("t001").unwrap())
    };
    assert!(grant(&one).is_some());
    assert!(grant(&other).is_none());
}
