use super::test_util::{field, folder, parts, said};
use super::{
    Actor, ClaimOpts, DoOpts, Env, Outcome, finish, parse_refs, reopen, spawn,
};
use crate::errs::{Code, Result};
use crate::todotxt::{Line, Priority};

const BEGAN: &str = "started:2026-09-29T0800Z";

#[derive(Clone, Copy)]
enum Op {
    Do,
    Reopen,
}

struct Case {
    name: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    refs: &'static str,
    agent: &'static str,
    creation: bool,
    priority: Option<char>,
    ops: &'static [Op],
    want: &'static str,
    msgs: &'static [&'static str],
    renumbered: bool,
    spawned: &'static [u32],
}

const BASE: Case = Case {
    name: "",
    todo: "",
    done: None,
    refs: "1",
    agent: "",
    creation: true,
    priority: None,
    ops: &[Op::Do],
    want: "",
    msgs: &[],
    renumbered: false,
    spawned: &[2],
};

const DATES: &[Case] = &[
    Case {
        name: "20.1 monthly",
        todo: "(B) 2026-09-01 Pay rent +home due:2026-09-30 rec:1m\n",
        want: "x 2026-09-29 2026-09-01 Pay rent +home due:2026-09-30 \
               rec:1m pri:B\n\
               (B) 2026-09-29 Pay rent +home due:2026-10-29 rec:1m\n",
        msgs: &["1 done: Pay rent", "2 next: Pay rent (due 2026-10-29)"],
        ..BASE
    },
    Case {
        name: "20.2 strict monthly",
        todo: "(B) 2026-09-01 Pay rent +home due:2026-09-30 rec:+1m\n",
        want: "x 2026-09-29 2026-09-01 Pay rent +home due:2026-09-30 \
               rec:+1m pri:B\n\
               (B) 2026-09-29 Pay rent +home due:2026-10-30 rec:+1m\n",
        msgs: &["1 done: Pay rent", "2 next: Pay rent (due 2026-10-30)"],
        ..BASE
    },
    Case {
        name: "20.3 strict with threshold",
        todo: "Water due:2026-10-10 t:2026-10-08 rec:+1w\n",
        want: "x 2026-09-29 Water due:2026-10-10 t:2026-10-08 rec:+1w\n\
               2026-09-29 Water due:2026-10-17 t:2026-10-15 rec:+1w\n",
        msgs: &["1 done: Water", "2 next: Water (due 2026-10-17)"],
        ..BASE
    },
    Case {
        name: "20.4 loose with threshold",
        todo: "Water due:2026-10-10 t:2026-10-08 rec:1w\n",
        want: "x 2026-09-29 Water due:2026-10-10 t:2026-10-08 rec:1w\n\
               2026-09-29 Water due:2026-10-06 t:2026-10-04 rec:1w\n",
        msgs: &["1 done: Water", "2 next: Water (due 2026-10-06)"],
        ..BASE
    },
    Case {
        name: "20.5 threshold only",
        todo: "Water t:2026-09-25 rec:2w\n",
        want: "x 2026-09-29 Water t:2026-09-25 rec:2w\n\
               2026-09-29 Water t:2026-10-13 rec:2w\n",
        msgs: &["1 done: Water", "2 next: Water (from 2026-10-13)"],
        ..BASE
    },
    Case {
        name: "20.5 threshold only strict",
        todo: "Water t:2026-09-25 rec:+2w\n",
        want: "x 2026-09-29 Water t:2026-09-25 rec:+2w\n\
               2026-09-29 Water t:2026-10-09 rec:+2w\n",
        msgs: &["1 done: Water", "2 next: Water (from 2026-10-09)"],
        ..BASE
    },
    Case {
        name: "20.6 neither date",
        todo: "Stretch rec:1d\n",
        want: "x 2026-09-29 Stretch rec:1d\n\
               2026-09-29 Stretch rec:1d due:2026-09-30\n",
        msgs: &["1 done: Stretch", "2 next: Stretch (due 2026-09-30)"],
        ..BASE
    },
    Case {
        name: "20.7 month clamps",
        todo: "Report due:2026-01-31 rec:+1m\n",
        want: "x 2026-09-29 Report due:2026-01-31 rec:+1m\n\
               2026-09-29 Report due:2026-02-28 rec:+1m\n",
        msgs: &["1 done: Report", "2 next: Report (due 2026-02-28)"],
        ..BASE
    },
    Case {
        name: "20.8 business days",
        todo: "Standup due:2026-10-02 rec:+1b\n",
        want: "x 2026-09-29 Standup due:2026-10-02 rec:+1b\n\
               2026-09-29 Standup due:2026-10-05 rec:+1b\n",
        msgs: &["1 done: Standup", "2 next: Standup (due 2026-10-05)"],
        ..BASE
    },
    Case {
        name: "20.9 strict in the past",
        todo: "Old due:2026-08-01 rec:+1w\n",
        want: "x 2026-09-29 Old due:2026-08-01 rec:+1w\n\
               2026-09-29 Old due:2026-08-08 rec:+1w\n",
        msgs: &["1 done: Old", "2 next: Old (due 2026-08-08)"],
        ..BASE
    },
    Case {
        name: "20.10 bad due is overwritten",
        todo: "A due:soon rec:1w\n",
        want: "x 2026-09-29 A due:soon rec:1w\n\
               2026-09-29 A due:2026-10-06 rec:1w\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        ..BASE
    },
    Case {
        name: "20.26 lead time is kept",
        todo: "Report due:2026-01-31 t:2026-01-29 rec:+1m\n",
        want: "x 2026-09-29 Report due:2026-01-31 t:2026-01-29 rec:+1m\n\
               2026-09-29 Report due:2026-02-28 t:2026-02-26 rec:+1m\n",
        msgs: &["1 done: Report", "2 next: Report (due 2026-02-28)"],
        ..BASE
    },
];

const SHAPE: &[Case] = &[
    Case {
        name: "20.11 bad rec",
        todo: "A rec:often\n",
        want: "x 2026-09-29 A rec:often\n",
        msgs: &["1 done: A", "1 has a bad rec: value; no next occurrence"],
        spawned: &[],
        ..BASE
    },
    Case {
        name: "20.12 first rec counts",
        todo: "A rec:1w rec:1d\n",
        want: "x 2026-09-29 A rec:1w rec:1d\n\
               2026-09-29 A rec:1w rec:1d due:2026-10-06\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        ..BASE
    },
    Case {
        name: "20.14 no creation date",
        todo: "2026-09-01 A rec:1w\n",
        creation: false,
        want: "x 2026-09-29 2026-09-01 A rec:1w\nA rec:1w due:2026-10-06\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        ..BASE
    },
    Case {
        name: "20.15 default priority is not applied",
        todo: "A rec:1w\n",
        priority: Some('C'),
        want: "x 2026-09-29 A rec:1w\n\
               2026-09-29 A rec:1w due:2026-10-06\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        ..BASE
    },
    Case {
        name: "20.16 spawn goes below",
        todo: "A rec:1w\nB\nC\n",
        want: "x 2026-09-29 A rec:1w\n\
               2026-09-29 A rec:1w due:2026-10-06\nB\nC\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        renumbered: true,
        ..BASE
    },
    Case {
        name: "20.17 spawn at the end",
        todo: "B\nA rec:1w\n",
        refs: "2",
        want: "B\nx 2026-09-29 A rec:1w\n\
               2026-09-29 A rec:1w due:2026-10-06\n",
        msgs: &["2 done: A", "3 next: A (due 2026-10-06)"],
        spawned: &[3],
        ..BASE
    },
    Case {
        name: "20.18 retry is a no-op",
        todo: "A rec:1w\nB\nC\n",
        ops: &[Op::Do, Op::Do],
        want: "x 2026-09-29 A rec:1w\n\
               2026-09-29 A rec:1w due:2026-10-06\nB\nC\n",
        msgs: &["1 is already done"],
        spawned: &[],
        ..BASE
    },
    Case {
        name: "20.19 several spawn",
        todo: "A rec:1w\nB rec:1d\n",
        refs: "1,2",
        want: "x 2026-09-29 A rec:1w\n\
               2026-09-29 A rec:1w due:2026-10-06\n\
               x 2026-09-29 B rec:1d\n\
               2026-09-29 B rec:1d due:2026-09-30\n",
        msgs: &[
            "1 done: A",
            "2 next: A (due 2026-10-06)",
            "2 done: B",
            "4 next: B (due 2026-09-30)",
        ],
        spawned: &[2, 4],
        ..BASE
    },
    Case {
        name: "20.20 spawn has no id",
        todo: "A id:a rec:1w\nB dep:a\n",
        want: "x 2026-09-29 A id:a rec:1w\n\
               2026-09-29 A rec:1w due:2026-10-06\nB dep:a\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        renumbered: true,
        ..BASE
    },
    Case {
        name: "20.22 done item makes no spawn",
        todo: "x 2026-09-28 A rec:1w\n",
        want: "x 2026-09-28 A rec:1w\n",
        msgs: &["1 is already done"],
        spawned: &[],
        ..BASE
    },
    Case {
        name: "20.25 crlf",
        todo: "A rec:1w\r\nB\r\n",
        want: "x 2026-09-29 A rec:1w\r\n\
               2026-09-29 A rec:1w due:2026-10-06\r\nB\r\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        renumbered: true,
        ..BASE
    },
    Case {
        name: "20.21 reopen leaves the spawn",
        todo: "(B) 2026-09-01 Pay rent due:2026-09-30 rec:1m\n",
        ops: &[Op::Do, Op::Reopen],
        want: "(B) 2026-09-01 Pay rent due:2026-09-30 rec:1m\n\
               (B) 2026-09-29 Pay rent due:2026-10-29 rec:1m\n",
        msgs: &[
            "1 reopened",
            "1 has rec:; its next occurrence stays in the list",
        ],
        spawned: &[],
        ..BASE
    },
];

fn apply(env: &Env<'_>, op: Op, refs: &str) -> Result<Outcome> {
    let targets = parse_refs(refs)?;
    match op {
        Op::Do => finish(env, &targets, DoOpts::default()),
        Op::Reopen => reopen(env, &targets, ClaimOpts::default()),
    }
}

fn check(case: &Case) {
    let dir = folder(case.todo, case.done);
    let mut config = dir.config.clone();
    config.include_creation = case.creation;
    config.default_priority =
        case.priority.and_then(|c| Priority::new(c as u8));
    let mut env = Env::new(&dir.found, &config, &dir.clock);
    env.actor = Actor::named(case.agent).expect("actor");
    let mut last = None;
    for &op in case.ops {
        last = Some(apply(&env, op, case.refs).expect(case.name));
    }
    let out = last.expect("an operation");
    assert_eq!(dir.todo(), case.want, "{}", case.name);
    assert_eq!(out.messages, said(case.msgs), "{}", case.name);
    assert_eq!(out.renumbered, case.renumbered, "{}", case.name);
    let spawned: Vec<u32> = out
        .spawned
        .iter()
        .filter_map(|id| out.after().item(*id))
        .map(|item| item.nr().get())
        .collect();
    assert_eq!(spawned, case.spawned, "{}", case.name);
    if case.name.starts_with("20.19") {
        assert_eq!(out.undo.map(|u| u.files.len()), Some(1));
    }
}

#[test]
fn dates_of_the_spawn() {
    DATES.iter().for_each(check);
}

#[test]
fn shape_of_the_spawn() {
    SHAPE.iter().for_each(check);
}

#[test]
fn claim_and_id_are_dropped() {
    let todo =
        format!("A id:k3f9 {BEGAN} owner:agent1 dep:zz file:a.md rec:1w\n");
    let dir = folder(&todo, None);
    let mut env = dir.env();
    env.actor = Actor::named("agent1").expect("actor");
    let out = finish(&env, &parse_refs("id:k3f9").expect("refs"), {
        DoOpts::default()
    })
    .expect("do");
    let want = format!(
        "x 2026-09-29 A id:k3f9 {BEGAN} owner:agent1 dep:zz file:a.md \
         rec:1w\n2026-09-29 A dep:zz file:a.md rec:1w due:2026-10-06\n"
    );
    assert_eq!(dir.todo(), want);
    assert_eq!(
        out.messages,
        said(&["k3f9 done: A", "2 next: A (due 2026-10-06)"])
    );
}

#[test]
fn dependents_are_satisfied_by_the_completed_item() {
    let dir = folder("A id:a rec:1w\nB dep:a\n", None);
    let env = dir.env();
    apply(&env, Op::Do, "1").expect("do");
    let out = apply(&env, Op::Do, "3").expect("do B");
    assert_eq!(out.messages, said(&["3 done: B"]));
}

#[test]
fn spawn_keeps_dependencies_and_may_be_blocked() {
    let dir = folder("B id:b\nA dep:b rec:1w\n", None);
    let env = dir.env();
    let targets = parse_refs("2").expect("refs");
    let opts = DoOpts {
        ignore_blocked: true,
        ..DoOpts::default()
    };
    let out = finish(&env, &targets, opts).expect("do");
    let spawned = out.spawned.first().and_then(|i| out.after().item(*i));
    assert!(spawned.is_some_and(crate::list::Item::blocked));
}

#[test]
fn done_file_line_spawns_at_the_end_of_the_todo_file() {
    let dir = folder("A rec:1d\nB\n", Some("x 2026-09-01 Z\nOther rec:1w\n"));
    let out = apply(&dir.env(), Op::Do, "done:2,1").expect("do");
    assert_eq!(
        dir.todo(),
        "x 2026-09-29 A rec:1d\n2026-09-29 A rec:1d due:2026-09-30\nB\n\
         2026-09-29 Other rec:1w due:2026-10-06\n"
    );
    assert_eq!(dir.done(), "x 2026-09-01 Z\nx 2026-09-29 Other rec:1w\n");
    assert_eq!(
        out.messages,
        said(&[
            "done:2 done: Other",
            "4 next: Other (due 2026-10-06)",
            "1 done: A",
            "2 next: A (due 2026-09-30)",
        ])
    );
}

const COPY: &str = "x 2026-09-28 2026-09-20 Fix +w id:a \
                    started:2026-09-29T0800Z owner:agent1 \
                    due:2026-10-01 pri:B";

#[test]
fn copy_of_a_done_line() {
    for (name, creation, want) in [
        ("20.23", true, "(B) 2026-09-29 Fix +w due:2026-10-01"),
        ("20.24", false, "(B) Fix +w due:2026-10-01"),
    ] {
        let dir = folder("", None);
        let mut config = dir.config.clone();
        config.include_creation = creation;
        let env = Env::new(&dir.found, &config, &dir.clock);
        let got = spawn(&env, &Line::parse(COPY)).expect(name);
        assert_eq!(got.raw(), want.as_bytes(), "{name}");
    }
}

#[test]
fn copy_that_would_make_a_stamp_is_refused() {
    let dir = folder("", None);
    let mut config = dir.config.clone();
    config.include_creation = false;
    let env = Env::new(&dir.found, &config, &dir.clock);
    let line = Line::parse("owner:z x 2026-09-01 foo");
    assert!(spawn(&env, &line).is_none());
}

#[test]
fn spawn_that_would_make_a_stamp_refuses_the_do() {
    let dir = folder("owner:z x 2026-09-01 foo rec:1w\n", None);
    let mut config = dir.config.clone();
    config.include_creation = false;
    let env = Env::new(&dir.found, &config, &dir.clock);
    let err = apply(&env, Op::Do, "1").expect_err("refused");
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            "1: a text edit can't change a stamp",
            "the text would start with a stamp-like word; reword it first",
        )
    );
    assert_eq!(dir.todo(), "owner:z x 2026-09-01 foo rec:1w\n");
}

#[test]
fn spawn_past_the_last_year_is_refused() {
    let dir = folder("A due:9999-12-01 rec:+1y\n", None);
    let err = apply(&dir.env(), Op::Do, "1").expect_err("refused");
    assert_eq!(err.code(), Code::InvalidValue);
    assert_eq!(
        err.message(),
        "the next occurrence is outside the years 0000 to 9999"
    );
    assert_eq!(field(&err, "field").as_deref(), Some("rec"));
    assert_eq!(field(&err, "value").as_deref(), Some("+1y"));
    assert_eq!(err.item().map(|r| r.nr.get()), Some(1));
    assert_eq!(dir.todo(), "A due:9999-12-01 rec:+1y\n");
}
