use super::test_util::{Dir, folder, parts, said};
use super::{
    Env, Outcome, PasteLine, Place, Relink, Target, del, parse_one,
    parse_refs, paste,
};
use crate::errs::{Code, Nr, Result};

fn own(raw: &str) -> PasteLine {
    own_with(raw, Vec::new())
}

fn own_with(raw: &str, relink: Vec<Relink>) -> PasteLine {
    PasteLine::Own {
        raw: raw.as_bytes().to_vec(),
        relink,
    }
}

fn foreign(raw: &str) -> PasteLine {
    PasteLine::Foreign(raw.as_bytes().to_vec())
}

fn relink(nr: u32, before: &str, after: &str) -> Relink {
    Relink {
        nr: Nr::new(nr),
        before: before.as_bytes().to_vec(),
        after: after.as_bytes().to_vec(),
    }
}

fn anchor(nr: &str) -> Target {
    parse_one("paste", nr).expect("target")
}

fn paste_at(dir: &Dir, lines: &[PasteLine], place: &Place) -> Result<Outcome> {
    paste(&dir.env(), lines, place)
}

fn messages(out: &Outcome) -> Vec<String> {
    out.messages()
        .iter()
        .map(|m| String::from_utf8_lossy(m).into_owned())
        .collect()
}

/// One paste: the file before, the lines, where, and what comes out.
struct Case {
    name: &'static str,
    todo: &'static str,
    lines: &'static [(&'static str, bool)],
    below: Option<&'static str>,
    file: &'static str,
    said: &'static [&'static str],
}

const CASES: [Case; 15] = [
    Case {
        name: "43.6 a copy goes below the item",
        todo: "A\nB\n",
        lines: &[("A", true)],
        below: Some("2"),
        file: "A\nB\n2026-09-29 A\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.8 a copy has no id and no claim",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\nB\n",
        lines: &[("A id:a started:2026-09-29T0800Z owner:agent1", true)],
        below: Some("2"),
        file: "A id:a started:2026-09-29T0800Z owner:agent1\nB\n\
               2026-09-29 A\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.9 a copy of a done item is open again",
        todo: "x 2026-09-28 2026-09-20 Fix pri:B\n",
        lines: &[("x 2026-09-28 2026-09-20 Fix pri:B", true)],
        below: Some("1"),
        file: "x 2026-09-28 2026-09-20 Fix pri:B\n(B) 2026-09-29 Fix\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.12 a move keeps its line exactly",
        todo: "\nB\n",
        lines: &[("A", true)],
        below: Some("2"),
        file: "\nB\nA\n",
        said: &["pasted 1 item (1 moved)"],
    },
    Case {
        name: "43.13 an identical line in the file makes a copy",
        todo: "A\n\nB\n",
        lines: &[("A", true)],
        below: Some("3"),
        file: "A\n\nB\n2026-09-29 A\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.13 the first of two twins moves, the second copies",
        todo: "\nB\n",
        lines: &[("A", true), ("A", true)],
        below: Some("2"),
        file: "\nB\nA\n2026-09-29 A\n",
        said: &["pasted 2 items (1 moved)"],
    },
    Case {
        name: "43.8 the first of two own lines with one id moves",
        todo: "\nB\n",
        lines: &[("A id:a", true), ("A id:a", true)],
        below: Some("2"),
        file: "\nB\nA id:a\n2026-09-29 A\n",
        said: &["pasted 2 items (1 moved)"],
    },
    Case {
        name: "43.16 foreign text is a copy",
        todo: "A\n",
        lines: &[("Buy milk", false)],
        below: None,
        file: "A\n2026-09-29 Buy milk\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.17 several lines of foreign text are one item",
        todo: "A\n",
        lines: &[("one\ntwo\r\nthree\t four\n", false)],
        below: None,
        file: "A\n2026-09-29 one two three  four\n",
        said: &["pasted 1 item (joined from 3 lines)"],
    },
    Case {
        name: "43.18 foreign stamps are dropped",
        todo: "A\n",
        lines: &[(
            "x 2026-01-01 Old id:zz started:2026-09-29T0800Z owner:a",
            false,
        )],
        below: None,
        file: "A\n2026-09-29 Old\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.19 foreign fuzzy dates are expanded",
        todo: "A\n",
        lines: &[("(b) Call due:fri", false)],
        below: None,
        file: "A\n(B) 2026-09-29 Call due:2026-10-02\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "43.20 a bad date is kept with a warning",
        todo: "A\n",
        lines: &[("Call due:soon", false)],
        below: None,
        file: "A\n2026-09-29 Call due:soon\n",
        said: &["pasted 1 item", "due:soon is not a date"],
    },
    Case {
        name: "43.33 an empty file is filled",
        todo: "",
        lines: &[("First", false)],
        below: None,
        file: "2026-09-29 First\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "a foreign text that forges a stamp is a copy first",
        todo: "A\n",
        lines: &[("2026-01-01 Old", false)],
        below: None,
        file: "A\n2026-09-29 Old\n",
        said: &["pasted 1 item"],
    },
    Case {
        name: "a copy of a copy keeps the priority",
        todo: "(A) Call +work\n",
        lines: &[("(A) Call +work", true)],
        below: Some("1"),
        file: "(A) Call +work\n(A) 2026-09-29 Call +work\n",
        said: &["pasted 1 item"],
    },
];

#[test]
fn lines_are_pasted_as_moves_or_copies() {
    for c in CASES {
        let dir = folder(c.todo, None);
        let lines: Vec<PasteLine> = c
            .lines
            .iter()
            .map(
                |&(raw, own_line)| {
                    if own_line { own(raw) } else { foreign(raw) }
                },
            )
            .collect();
        let place = c.below.map_or(Place::End, |nr| Place::After(anchor(nr)));
        let out = paste_at(&dir, &lines, &place).expect(c.name);
        assert_eq!(dir.todo(), c.file, "{}", c.name);
        assert_eq!(out.messages(), said(c.said), "{}", c.name);
    }
}

#[test]
fn the_lines_go_above_the_item() {
    let dir = folder("A\nB\n", None);
    let place = Place::Before(anchor("2"));
    paste_at(&dir, &[own("A")], &place).expect("paste");
    assert_eq!(dir.todo(), "A\n2026-09-29 A\nB\n");
}

#[test]
fn a_block_goes_in_order_in_one_write() {
    let dir = folder("\n\nC\n", None);
    let lines = [own("A"), own("B")];
    let out = paste_at(&dir, &lines, &Place::After(anchor("3"))).expect("p");
    assert_eq!(dir.todo(), "\n\nC\nA\nB\n");
    assert_eq!(out.items().len(), 2);
    let record = out.undo().expect("a record");
    assert_eq!(record.files.len(), 1);
}

#[test]
fn the_creation_date_follows_the_option() {
    let mut dir = folder("A\n", None);
    dir.config.include_creation = false;
    paste_at(&dir, &[own("A")], &Place::End).expect("paste");
    assert_eq!(dir.todo(), "A\nA\n");
}

#[test]
fn no_lines_is_refused() {
    let dir = folder("A\n", None);
    let err = paste_at(&dir, &[], &Place::End).expect_err("refused");
    assert_eq!(parts(&err), (Code::Usage, "nothing to paste", ""));
}

#[test]
fn text_with_no_description_is_refused() {
    let dir = folder("A\n", None);
    let err =
        paste_at(&dir, &[foreign(" \n ")], &Place::End).expect_err("refused");
    assert_eq!(err.code(), Code::InvalidValue);
    assert_eq!(err.message(), "empty task");
    assert_eq!(dir.todo(), "A\n");
}

#[test]
fn a_copy_that_would_start_with_a_stamp_refuses_the_paste() {
    let held = "owner:z x 2026-09-01 foo\n";
    let mut dir = folder(held, None);
    dir.config.include_creation = false;
    let lines = [foreign("B"), own("owner:z x 2026-09-01 foo")];
    let err = paste_at(&dir, &lines, &Place::End).expect_err("refused");
    assert_eq!(err.code(), Code::InvalidValue);
    assert_eq!(err.message(), "can't copy owner:z x 2026-09-01 foo");
    assert_eq!(dir.todo(), held);
}

#[test]
fn a_missing_anchor_is_refused_and_nothing_is_written() {
    let dir = folder("A\n", None);
    let err = paste_at(&dir, &[own("B")], &Place::After(anchor("9")))
        .expect_err("refused");
    assert_eq!(err.code(), Code::NoItem);
    assert_eq!(dir.todo(), "A\n");
}

fn delete(dir: &Dir, nr: &str) -> Outcome {
    del(
        &dir.env(),
        &parse_refs(nr).expect("refs"),
        super::ClaimOpts::default(),
    )
    .expect("del")
}

#[test]
fn a_move_gives_its_dependents_their_link_back() {
    let dir = folder("A id:a\nB dep:a\nC\n", None);
    delete(&dir, "1");
    assert_eq!(dir.todo(), "\nB\nC\n");
    let line = own_with("A id:a", vec![relink(2, "B dep:a", "B")]);
    let out = paste_at(&dir, &[line], &Place::After(anchor("3"))).expect("p");
    assert_eq!(dir.todo(), "\nB dep:a\nC\nA id:a\n");
    assert_eq!(messages(&out), ["pasted 1 item (1 moved)", "relinked 2"]);
    assert_eq!(out.changed().len(), 1);
}

#[test]
fn a_dependent_that_changed_is_left_alone() {
    let dir = folder("\nB +y\nC dep:a +x\n", None);
    let line = own_with(
        "A id:a",
        vec![relink(2, "B dep:a", "B"), relink(3, "C dep:a +x", "C +x")],
    );
    let out = paste_at(&dir, &[line], &Place::After(anchor("3"))).expect("p");
    assert_eq!(dir.todo(), "\nB +y\nC dep:a +x\nA id:a\n");
    assert_eq!(
        messages(&out),
        [
            "pasted 1 item (1 moved)",
            "relinked 3",
            "2 changed since; dep:a not restored"
        ]
    );
}

#[test]
fn a_dependent_that_is_gone_is_reported() {
    let dir = folder("\n\nC\n", None);
    let line = own_with(
        "A id:a",
        vec![
            relink(2, "B dep:a", "B"),
            relink(0, "Y dep:a", "Y"),
            relink(9, "Z dep:a", "Z"),
        ],
    );
    let out = paste_at(&dir, &[line], &Place::After(anchor("3"))).expect("p");
    assert_eq!(
        messages(&out),
        [
            "pasted 1 item (1 moved)",
            "a dependent of a is gone",
            "a dependent of a is gone",
            "a dependent of a is gone"
        ]
    );
}

#[test]
fn a_link_that_would_close_a_cycle_is_not_restored() {
    let dir = folder("\nB id:b\nC id:c dep:b\n", None);
    let line =
        own_with("A id:a dep:c", vec![relink(2, "B id:b dep:a", "B id:b")]);
    let out = paste_at(&dir, &[line], &Place::After(anchor("3"))).expect("p");
    assert_eq!(dir.todo(), "\nB id:b\nC id:c dep:b\nA id:a dep:c\n");
    assert_eq!(
        messages(&out),
        [
            "pasted 1 item (1 moved)",
            "2: dep:a would close a cycle; not restored"
        ]
    );
}

#[test]
fn a_copy_restores_nothing() {
    let dir = folder("A id:a\nB\n", None);
    let line = own_with("A id:a", vec![relink(2, "B dep:a", "B")]);
    let out = paste_at(&dir, &[line], &Place::After(anchor("2"))).expect("p");
    assert_eq!(dir.todo(), "A id:a\nB\n2026-09-29 A\n");
    assert_eq!(messages(&out), ["pasted 1 item"]);
}

#[test]
fn a_dependency_cycle_in_pasted_text_is_warned_about() {
    let dir = folder("A id:a dep:b\n", None);
    let out =
        paste_at(&dir, &[own("B id:b dep:a")], &Place::End).expect("paste");
    assert_eq!(dir.todo(), "A id:a dep:b\nB id:b dep:a\n");
    assert_eq!(
        messages(&out),
        [
            "pasted 1 item (1 moved)",
            "2 is now part of a dependency cycle"
        ]
    );
}

#[test]
fn env_is_not_changed_by_the_paste() {
    let dir = folder("A\n", None);
    let env: Env<'_> = dir.env();
    paste(&env, &[foreign("x")], &Place::End).expect("paste");
    assert!(!env.cycle_warn_only);
}

#[test]
fn a_link_closes_a_cycle_when_the_id_already_waits_for_the_dependent() {
    let edge = |from: &str, to: &[&str]| {
        (
            from.as_bytes().to_vec(),
            to.iter().map(|t| t.as_bytes().to_vec()).collect::<Vec<_>>(),
        )
    };
    let graph: super::relink::Graph =
        [edge("a", &["b"]), edge("b", &["c", "a"]), edge("c", &[])]
            .into_iter()
            .collect();
    let rows: [(&str, &str, bool); 7] = [
        ("a", "a", true),
        ("a", "b", true),
        ("a", "c", true),
        ("c", "a", false),
        ("b", "c", true),
        ("a", "z", false),
        ("z", "a", false),
    ];
    for (id, dependent, want) in rows {
        let got = super::relink::closes_cycle(
            &graph,
            id.as_bytes(),
            dependent.as_bytes(),
        );
        assert_eq!(got, want, "{id} {dependent}");
    }
}

#[test]
fn a_restored_link_counts_when_the_next_one_is_checked() {
    let dir = folder("\n\nX id:x\nY id:y\n", None);
    let a =
        own_with("A id:a dep:y", vec![relink(3, "X id:x dep:a", "X id:x")]);
    let b =
        own_with("B id:b dep:x", vec![relink(4, "Y id:y dep:b", "Y id:y")]);
    let out = paste_at(&dir, &[a, b], &Place::After(anchor("4"))).expect("p");
    assert_eq!(
        dir.todo(),
        "\n\nX id:x dep:a\nY id:y\nA id:a dep:y\nB id:b dep:x\n"
    );
    assert_eq!(
        messages(&out),
        [
            "pasted 2 items (2 moved)",
            "relinked 3",
            "4: dep:b would close a cycle; not restored"
        ]
    );
}

#[test]
fn an_id_held_in_the_done_file_makes_a_copy() {
    let dir = folder("B\n", Some("x 2026-09-01 A id:ab\n"));
    let out = paste_at(&dir, &[own("A id:ab")], &Place::End).expect("paste");
    assert_eq!(messages(&out), ["pasted 1 item"]);
}
