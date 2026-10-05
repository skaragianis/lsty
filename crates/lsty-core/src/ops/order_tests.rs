use std::fs;

use super::test_util::{field, folder, parts, said};
use super::{
    Actor, Placement, deduplicate, down, parse_one, parse_refs, reorder, up,
};
use crate::errs::Code;

/// What to run.
#[derive(Clone, Copy)]
enum Cmd {
    Down(&'static str, i64),
    Up(&'static str, i64),
    After(&'static str, &'static str),
    Before(&'static str, &'static str),
    Pos(&'static str, i64),
}

/// Name, file, command, file after, messages, whether nothing changed.
type Row = (
    &'static str,
    &'static str,
    Cmd,
    &'static str,
    &'static [&'static str],
    bool,
);

const MOVES: &[Row] = &[
    (
        "22.1",
        "A\nB\nC\n",
        Cmd::Down("1", 1),
        "B\nA\nC\n",
        &["1 moved to 2"],
        false,
    ),
    (
        "22.2",
        "A\nB\nC\n",
        Cmd::Up("3", 1),
        "A\nC\nB\n",
        &["3 moved to 2"],
        false,
    ),
    (
        "22.3",
        "A\nB\nC\nD\n",
        Cmd::Down("1", 2),
        "B\nC\nA\nD\n",
        &["1 moved to 3"],
        false,
    ),
    (
        "22.4",
        "A\nB\nC\n",
        Cmd::Down("1", 9),
        "B\nC\nA\n",
        &["1 moved to 3"],
        false,
    ),
    (
        "22.5 top",
        "A\nB\n",
        Cmd::Up("1", 1),
        "A\nB\n",
        &["1 is already at the top"],
        true,
    ),
    (
        "22.5 end",
        "A\nB\n",
        Cmd::Down("2", 1),
        "A\nB\n",
        &["2 is already at the end"],
        true,
    ),
    (
        "22.6",
        "A\n\nB\nC\n",
        Cmd::Down("1", 1),
        "\nB\nA\nC\n",
        &["1 moved to 3"],
        false,
    ),
    (
        "22.7",
        "A\nB\n\nC\n",
        Cmd::Up("4", 1),
        "A\nC\nB\n\n",
        &["4 moved to 2"],
        false,
    ),
    (
        "22.8",
        "A\nB\nC\nD\n",
        Cmd::Down("1,2", 1),
        "C\nA\nB\nD\n",
        &["1 moved to 2", "2 moved to 3"],
        false,
    ),
    (
        "22.9",
        "A\nB\nC\nD\n",
        Cmd::Down("3,1", 1),
        "B\nD\nA\nC\n",
        &["1 moved to 3", "3 moved to 4"],
        false,
    ),
    (
        "22.10",
        "A\nB\nC\nD\nE\n",
        Cmd::Down("1,3", 1),
        "B\nD\nA\nC\nE\n",
        &["1 moved to 3", "3 moved to 4"],
        false,
    ),
    (
        "22.11",
        "A\nB\nC\nD\nE\n",
        Cmd::Up("3,5", 1),
        "A\nC\nE\nB\nD\n",
        &["3 moved to 2", "5 moved to 3"],
        false,
    ),
    (
        "22.14",
        "A id:a\nB id:b\nC id:c\n",
        Cmd::After("id:a", "id:c"),
        "B id:b\nC id:c\nA id:a\n",
        &["a moved to 3"],
        false,
    ),
    (
        "block above the anchor, anchor not last, after",
        "A\nB\nC\nD\n",
        Cmd::After("1", "3"),
        "B\nC\nA\nD\n",
        &["1 moved to 3"],
        false,
    ),
    (
        "block above the anchor, before",
        "A\nB\nC\nD\n",
        Cmd::Before("1", "3"),
        "B\nA\nC\nD\n",
        &["1 moved to 2"],
        false,
    ),
    (
        "22.15",
        "B id:b\nC id:c\nA id:a\n",
        Cmd::After("id:a", "id:c"),
        "B id:b\nC id:c\nA id:a\n",
        &["a is already there"],
        true,
    ),
    (
        "22.16",
        "A id:a\nB id:b\nC id:c\n",
        Cmd::Before("id:c", "id:a"),
        "C id:c\nA id:a\nB id:b\n",
        &["c moved to 1"],
        false,
    ),
    (
        "22.17",
        "A\nB\nC\nD\n",
        Cmd::After("4,2", "1"),
        "A\nB\nD\nC\n",
        &["2 moved to 2", "4 moved to 3"],
        false,
    ),
    (
        "22.21",
        "A\nB\nC\nD\n",
        Cmd::Pos("4", 1),
        "D\nA\nB\nC\n",
        &["4 moved to 1"],
        false,
    ),
    (
        "22.22",
        "A\nB\nC\nD\n",
        Cmd::Pos("1", 3),
        "B\nC\nA\nD\n",
        &["1 moved to 3"],
        false,
    ),
    (
        "22.23",
        "A\nB\nC\nD\n",
        Cmd::Pos("1", 99),
        "B\nC\nD\nA\n",
        &["1 moved to 4"],
        false,
    ),
    (
        "22.24",
        "A\nB\nC\nD\n",
        Cmd::Pos("2", 2),
        "A\nB\nC\nD\n",
        &["2 is already there"],
        true,
    ),
    (
        "before a blank line's neighbour",
        "A\n\nB\n",
        Cmd::Before("3", "1"),
        "B\nA\n\n",
        &["3 moved to 1"],
        false,
    ),
    (
        "up of a done todo item",
        "A\nx 2026-09-01 B\n",
        Cmd::Up("2", 1),
        "x 2026-09-01 B\nA\n",
        &["2 moved to 1"],
        false,
    ),
    (
        "reorder of a done todo item",
        "x 2026-09-01 A\nB\n",
        Cmd::After("1", "2"),
        "B\nx 2026-09-01 A\n",
        &["1 moved to 2"],
        false,
    ),
    (
        "gone target is skipped",
        "A\n\nB\n",
        Cmd::Down("2", 1),
        "A\n\nB\n",
        &["2 already deleted"],
        true,
    ),
];

fn run_cmd(
    env: &super::Env<'_>,
    cmd: Cmd,
) -> crate::errs::Result<super::Outcome> {
    let refs = |s: &str| parse_refs(s).expect("refs");
    let one = |s: &str| parse_one("reorder", s).expect("ref");
    match cmd {
        Cmd::Down(t, n) => down(env, &refs(t), n),
        Cmd::Up(t, n) => up(env, &refs(t), n),
        Cmd::After(t, a) => reorder(env, &refs(t), &Placement::After(one(a))),
        Cmd::Before(t, a) => {
            reorder(env, &refs(t), &Placement::Before(one(a)))
        }
        Cmd::Pos(t, p) => reorder(env, &refs(t), &Placement::Position(p)),
    }
}

#[test]
fn move_rows() {
    for (name, todo, cmd, want, msgs, noop) in MOVES {
        let dir = folder(todo, None);
        let out = run_cmd(&dir.env(), *cmd).expect(name);
        assert_eq!(dir.todo(), *want, "{name}");
        assert_eq!(out.messages(), said(msgs), "{name}");
        assert_eq!(out.no_op(), *noop, "{name}");
        assert_eq!(out.undo().is_none(), *noop, "{name}");
        assert_eq!(out.renumbered(), !*noop, "{name}: renumbered");
    }
}

#[test]
fn down_twice_moves_twice() {
    let dir = folder("A\nB\nC\n", None);
    down(&dir.env(), &parse_refs("1").unwrap(), 1).unwrap();
    let out = down(&dir.env(), &parse_refs("2").unwrap(), 1).unwrap();
    assert_eq!(out.messages(), said(&["2 moved to 3"]), "22.13");
    assert_eq!(dir.todo(), "B\nC\nA\n", "22.13");
}

#[test]
fn bad_counts() {
    for n in [0, -1] {
        let dir = folder("A\nB\n", None);
        for err in [
            down(&dir.env(), &parse_refs("1").unwrap(), n).unwrap_err(),
            up(&dir.env(), &parse_refs("2").unwrap(), n).unwrap_err(),
        ] {
            assert_eq!(err.code(), Code::InvalidValue, "22.12");
            assert_eq!(field(&err, "field").as_deref(), Some("count"));
        }
        assert_eq!(dir.todo(), "A\nB\n");
    }
}

#[test]
fn bad_position() {
    let dir = folder("A\nB\n", None);
    let place = Placement::Position(0);
    let err =
        reorder(&dir.env(), &parse_refs("1").unwrap(), &place).unwrap_err();
    assert_eq!(err.code(), Code::InvalidValue, "22.25");
    assert_eq!(field(&err, "field").as_deref(), Some("position"));
}

#[test]
fn anchor_is_a_target() {
    let dir = folder("A\nB\nC\n", None);
    let place = Placement::After(parse_one("reorder", "2").unwrap());
    let err =
        reorder(&dir.env(), &parse_refs("1,2").unwrap(), &place).unwrap_err();
    assert_eq!(
        parts(&err),
        (Code::Usage, "2 is both a target and the anchor", "")
    );
    assert_eq!(dir.todo(), "A\nB\nC\n", "22.18");
}

#[test]
fn anchor_errors() {
    let dir = folder("A\n\nC\n", Some("x 2026-09-01 Old id:o\n"));
    for (anchor, code) in [
        ("9", Code::NoItem),
        ("2", Code::NoItem),
        ("done:1", Code::WrongState),
    ] {
        let place = Placement::After(parse_one("reorder", anchor).unwrap());
        let err = reorder(&dir.env(), &parse_refs("1").unwrap(), &place)
            .unwrap_err();
        assert_eq!(err.code(), code, "22.19 {anchor}");
    }
    assert_eq!(dir.todo(), "A\n\nC\n");
}

#[test]
fn done_file_target() {
    let dir = folder("A\n", Some("x 2026-09-01 Old\n"));
    let place = Placement::After(parse_one("reorder", "1").unwrap());
    let err = reorder(&dir.env(), &parse_refs("done:1").unwrap(), &place)
        .unwrap_err();
    assert_eq!(err.code(), Code::WrongState, "22.26");
}

#[test]
fn done_items_of_the_todo_file_move() {
    let dir = folder("x 2026-09-01 A\nB\n", None);
    down(&dir.env(), &parse_refs("1").unwrap(), 1).unwrap();
    assert_eq!(dir.todo(), "B\nx 2026-09-01 A\n");
}

#[test]
fn claimed_item_moves_with_a_warning() {
    let dir = folder(
        "A id:a started:2026-09-29T0900Z owner:agent2\nB id:b\n",
        None,
    );
    let mut env = dir.env();
    env.actor = Actor::named("agent1").expect("actor");
    let place = Placement::After(parse_one("reorder", "id:b").unwrap());
    let out = reorder(&env, &parse_refs("id:a").unwrap(), &place).unwrap();
    assert_eq!(
        out.messages(),
        said(&["a moved to 2", "a is claimed by agent2"]),
        "22.27"
    );
    assert_eq!(
        dir.todo(),
        "B id:b\nA id:a started:2026-09-29T0900Z owner:agent2\n"
    );
}

/// The line numbers of the items and of their old lines.
fn nrs(out: &super::Outcome) -> (Vec<u32>, Vec<u32>) {
    let after = out.items_after().map(|i| i.nr().get()).collect();
    let before = out.items_before().map(|i| i.nr().get()).collect();
    (after, before)
}

#[test]
fn items_after_and_before() {
    let dir = folder("A\nB\nC\n", None);
    let out = down(&dir.env(), &parse_refs("1,2").unwrap(), 1).unwrap();
    assert_eq!(out.action(), "down");
    assert_eq!(nrs(&out), (vec![2, 3], vec![1, 2]));
    let raw: Vec<&[u8]> = out
        .items_after()
        .map(|i| out.after().line(i).raw())
        .collect();
    assert_eq!(raw, [b"A".as_slice(), b"B".as_slice()]);
}

#[test]
fn reorder_takes_one_anchor() {
    let err = parse_one("reorder", "2,3").unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::Usage,
            "reorder takes one reference",
            "run \"lsty help\""
        ),
        "22.20"
    );
}

/// Name, files, file after, messages, whether nothing changed.
type DedupRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    bool,
);

const DEDUP: &[DedupRow] = &[
    (
        "22.28",
        "A\nB\nA\nC\nA\n",
        "A\nB\n\nC\n\n",
        &["removed 2 duplicates"],
        false,
    ),
    (
        "22.29",
        "A\nB\n\nC\n\n",
        "A\nB\n\nC\n\n",
        &["no duplicates"],
        true,
    ),
    (
        "22.30",
        "A\na\nA \n",
        "A\na\nA \n",
        &["no duplicates"],
        true,
    ),
    (
        "22.31",
        "A id:k\nA id:k\nB dep:k\n",
        "A id:k\n\nB dep:k\n",
        &["removed 1 duplicates"],
        false,
    ),
    (
        "22.33",
        "A\n\n\nA\n",
        "A\n\n\n\n",
        &["removed 1 duplicates"],
        false,
    ),
];

#[test]
fn dedup_rows() {
    for (name, todo, want, msgs, noop) in DEDUP {
        let dir = folder(todo, None);
        let out = deduplicate(&dir.env()).expect(name);
        assert_eq!(dir.todo(), *want, "{name}");
        assert_eq!(out.messages(), said(msgs), "{name}");
        assert_eq!(out.no_op(), *noop, "{name}");
        assert!(!out.renumbered(), "{name}: renumbered");
        assert!(out.items().is_empty(), "{name}: items");
    }
}

#[test]
fn dedup_reports_the_blanked_lines() {
    let dir = folder("A\nB\nA\nC\nA\n", None);
    let out = deduplicate(&dir.env()).unwrap();
    let before: Vec<&[u8]> = out
        .items_before()
        .map(|i| out.read().line(i).raw())
        .collect();
    assert_eq!(before, [b"A".as_slice(), b"A".as_slice()]);
    assert_eq!(nrs(&out), (vec![], vec![3, 5]));
    assert_eq!(out.action(), "deduplicate");
}

#[test]
fn dedup_leaves_the_done_file() {
    let dir = folder("A\n", Some("A\nA\n"));
    let done = dir.found.done.clone();
    let before = fs::metadata(&done).unwrap().modified().unwrap();
    let out = deduplicate(&dir.env()).unwrap();
    assert!(out.no_op(), "22.32");
    assert_eq!(dir.done(), "A\nA\n");
    assert_eq!(fs::metadata(&done).unwrap().modified().unwrap(), before);
}
