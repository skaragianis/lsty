use super::{
    AddParams, Grant, Target, add, edit_typed,
    test_util::{Dir, folder, snapshot},
};
use crate::{
    errs::{Code, Nr, Source},
    todotxt::Id,
};

/// The grant of `id` to line `nr` of the file `read`.
fn grant(dir: &Dir, read: &str, nr: u32, id: &str) -> Grant {
    let snap = snapshot(dir, read.as_bytes(), b"");
    let task = snap.at(Source::Todo, Nr::new(nr).unwrap()).unwrap();
    Grant::new(&snap, task.task_id(), Id::new(id).unwrap()).unwrap()
}

const READ: &str = "Call client\nWrite docs +spec01\n";

/// Name, the file by the time of the write, and the file after it; none
/// for a refusal, which leaves the file as it was.
type Row = (&'static str, &'static str, Option<&'static str>);

const ROWS: &[Row] = &[
    (
        "39.24 id on commit",
        READ,
        Some("Call client dep:t001\nWrite docs +spec01 id:t001\n"),
    ),
    (
        "a line added above is a shift",
        "Intro\nCall client\nWrite docs +spec01\n",
        Some("Intro\nCall client dep:t001\nWrite docs +spec01 id:t001\n"),
    ),
    (
        "the task changed",
        "Call client\nWrite docs +spec01 more\n",
        None,
    ),
    (
        "the task got an id",
        "Call client\nWrite docs +spec01 id:zz\n",
        None,
    ),
    ("the task is gone", "Call client\n", None),
    ("the task moved", "Write docs +spec01\nCall client\n", None),
];

#[test]
fn a_typed_edit_gives_the_id_in_the_same_write_or_neither() {
    for (name, disk, want) in ROWS {
        let dir = folder(disk, None);
        let shown = snapshot(&dir, READ.as_bytes(), b"");
        let mut env = dir.env();
        env.aim = Some(&shown);
        let g = grant(&dir, READ, 2, "t001");
        let got =
            edit_typed(&env, &Target::todo(1), b"Call client dep:t001", &[g]);
        if let Some(want) = want {
            assert!(got.unwrap().undo().is_some(), "{name}");
            assert_eq!(dir.todo(), *want, "{name}");
        } else {
            let e = got.unwrap_err();
            assert_eq!(e.code(), Code::ExpectMismatch, "{name}");
            assert_eq!(dir.todo(), *disk, "{name}");
        }
    }
}

#[test]
fn a_new_task_gives_the_id_too() {
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
        "Write docs id:t001\n2026-09-29 Call client dep:t001 id:aaaa\n"
    );
}
