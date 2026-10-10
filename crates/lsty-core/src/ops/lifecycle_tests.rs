use super::{
    ClaimOpts, DoOpts, Env, Outcome, StartOpts, finish, parse_refs, reopen,
    start, stop,
    test_util::{folder, parts, said},
};
use crate::{
    errs::{Code, Result},
    list::State,
};

#[derive(Clone, Copy)]
enum Op {
    Start,
    StartIgnoring,
    Stop,
    Do,
    DoIgnoring,
    Reopen,
}

fn apply(env: &Env<'_>, op: Op, refs: &str) -> Result<Outcome> {
    let targets = parse_refs(refs)?;
    let claim = ClaimOpts::default();
    let ignoring = StartOpts {
        claim,
        ignore_blocked: true,
    };
    match op {
        Op::Start => start(env, &targets, StartOpts::default()),
        Op::StartIgnoring => start(env, &targets, ignoring),
        Op::Stop => stop(env, &targets, claim),
        Op::Do => finish(env, &targets, DoOpts::default()),
        Op::DoIgnoring => finish(
            env,
            &targets,
            DoOpts {
                ignore_blocked: true,
                ..DoOpts::default()
            },
        ),
        Op::Reopen => reopen(env, &targets, claim),
    }
}

struct Case {
    name: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    refs: &'static str,
    keep: bool,
    ops: &'static [Op],
    want: &'static str,
    msgs: &'static [&'static str],
    state: Option<State>,
    no_op: bool,
    blocked: bool,
}

const BASE: Case = Case {
    name: "",
    todo: "",
    done: None,
    refs: "1",
    keep: true,
    ops: &[],
    want: "",
    msgs: &[],
    state: None,
    no_op: false,
    blocked: false,
};

const CASES_A: &[Case] = &[
    Case {
        name: "17.1 start",
        todo: "(A) Call client +work\n",
        ops: &[Op::Start],
        want: "(A) Call client +work started:2026-09-29T0900Z\n",
        msgs: &["1 started"],
        state: Some(State::Doing),
        ..BASE
    },
    Case {
        name: "17.2 start again",
        todo: "(A) Call client +work\n",
        ops: &[Op::Start, Op::Start],
        want: "(A) Call client +work started:2026-09-29T0900Z\n",
        msgs: &["1 is already underway"],
        state: Some(State::Doing),
        no_op: true,
        ..BASE
    },
    Case {
        name: "17.5 start blocked with ignore",
        todo: "A id:a\nB dep:a\n",
        refs: "2",
        ops: &[Op::StartIgnoring],
        want: "A id:a\nB dep:a started:2026-09-29T0900Z\n",
        msgs: &["2 started", "2 was blocked by 1"],
        state: Some(State::Doing),
        ..BASE
    },
    Case {
        name: "start in a cycle names every prerequisite",
        todo: "A id:a dep:b dep:c\nB id:b dep:a\nC id:c\n",
        ops: &[Op::StartIgnoring],
        want: "A id:a dep:b dep:c started:2026-09-29T0900Z\nB id:b dep:a\nC \
               id:c\n",
        msgs: &["1 started", "1 was blocked by 2, 3"],
        state: Some(State::Doing),
        ..BASE
    },
    Case {
        name: "17.7 dangling dep does not block",
        todo: "B dep:zz\n",
        ops: &[Op::Start],
        want: "B dep:zz started:2026-09-29T0900Z\n",
        msgs: &["1 started"],
        state: Some(State::Doing),
        ..BASE
    },
    Case {
        name: "17.8 stop",
        todo: "A started:2026-09-29T0800Z owner:agent1 +w\n",
        ops: &[Op::Stop],
        want: "A +w\n",
        msgs: &["1 stopped", "1 was claimed by agent1"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "17.9 stop a todo",
        todo: "A\n",
        ops: &[Op::Stop],
        want: "A\n",
        msgs: &["1 is not underway"],
        state: Some(State::Todo),
        no_op: true,
        ..BASE
    },
    Case {
        name: "17.9 stop a done",
        todo: "x 2026-09-28 A started:2026-09-29T0800Z\n",
        ops: &[Op::Stop],
        want: "x 2026-09-28 A started:2026-09-29T0800Z\n",
        msgs: &["1 is not underway"],
        state: Some(State::Done),
        no_op: true,
        ..BASE
    },
    Case {
        name: "17.10 stop a blocked",
        todo: "A id:a\nB dep:a started:2026-09-29T0800Z\n",
        refs: "2",
        ops: &[Op::Stop],
        want: "A id:a\nB dep:a\n",
        msgs: &["2 stopped"],
        state: Some(State::Todo),
        ..BASE
    },
];

const CASES_B: &[Case] = &[
    Case {
        name: "17.11 do",
        todo: "(A) 2026-09-20 Call client +work started:2026-09-29T0900Z\n",
        ops: &[Op::Do],
        want: "x 2026-09-29 2026-09-20 Call client +work \
               started:2026-09-29T0900Z pri:A\n",
        msgs: &["1 done: Call client"],
        state: Some(State::Done),
        ..BASE
    },
    Case {
        name: "17.12 do plain",
        todo: "Buy milk\n",
        ops: &[Op::Do],
        want: "x 2026-09-29 Buy milk\n",
        msgs: &["1 done: Buy milk"],
        state: Some(State::Done),
        ..BASE
    },
    Case {
        name: "17.13 do without keep_priority",
        todo: "(A) Call\n",
        keep: false,
        ops: &[Op::Do],
        want: "x 2026-09-29 Call\n",
        msgs: &["1 done: Call"],
        state: Some(State::Done),
        ..BASE
    },
    Case {
        name: "17.14 do replaces pri tags",
        todo: "(A) Call pri:C\n",
        ops: &[Op::Do],
        want: "x 2026-09-29 Call pri:A\n",
        msgs: &["1 done: Call"],
        state: Some(State::Done),
        ..BASE
    },
    Case {
        name: "17.15 do again",
        todo: "Buy\n",
        ops: &[Op::Do, Op::Do],
        want: "x 2026-09-29 Buy\n",
        msgs: &["1 is already done"],
        state: Some(State::Done),
        no_op: true,
        ..BASE
    },
    Case {
        name: "17.17 do blocked with ignore",
        todo: "A id:a\nB dep:a\n",
        refs: "2",
        ops: &[Op::DoIgnoring],
        want: "A id:a\nx 2026-09-29 B dep:a\n",
        msgs: &["2 done: B", "2 was blocked by 1"],
        state: Some(State::Done),
        ..BASE
    },
    Case {
        name: "17.19 do mixed",
        todo: "A\nx 2026-09-28 B\n",
        refs: "1,2",
        ops: &[Op::Do],
        want: "x 2026-09-29 A\nx 2026-09-28 B\n",
        msgs: &["1 done: A", "2 is already done"],
        ..BASE
    },
    Case {
        name: "17.20 do archived",
        todo: "A\n",
        done: Some("x 2026-09-28 B\n"),
        refs: "done:1",
        ops: &[Op::Do],
        want: "A\n",
        msgs: &["done:1 is already done"],
        state: Some(State::Done),
        no_op: true,
        ..BASE
    },
    Case {
        name: "17.31 crlf",
        todo: "A\r\nB\r\nC",
        ops: &[Op::Do],
        want: "x 2026-09-29 A\r\nB\r\nC",
        msgs: &["1 done: A"],
        state: Some(State::Done),
        ..BASE
    },
];

const CASES_C: &[Case] = &[
    Case {
        name: "17.21 reopen",
        todo: "x 2026-09-29 2026-09-20 Call +work started:2026-09-29T0900Z \
               pri:A\n",
        ops: &[Op::Reopen],
        want: "(A) 2026-09-20 Call +work started:2026-09-29T0900Z\n",
        msgs: &["1 reopened; yours again"],
        state: Some(State::Doing),
        ..BASE
    },
    Case {
        name: "17.22 reopen never started",
        todo: "x 2026-09-29 Buy\n",
        ops: &[Op::Reopen],
        want: "Buy\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "17.23 reopen first valid pri",
        todo: "x 2026-09-29 Buy pri:b pri:C\n",
        ops: &[Op::Reopen],
        want: "(C) Buy\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "reopen ignores other tags with a letter value",
        todo: "x 2026-09-29 Buy proj:B\n",
        ops: &[Op::Reopen],
        want: "Buy proj:B\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "17.24 reopen keeps a left priority",
        todo: "x (A) 2026-09-28 Fix pri:B\n",
        ops: &[Op::Reopen],
        want: "(A) Fix\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "17.25 reopen without date",
        todo: "x Fix\n",
        ops: &[Op::Reopen],
        want: "Fix\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "17.26 reopen open",
        todo: "Buy\n",
        ops: &[Op::Reopen],
        want: "Buy\n",
        msgs: &["1 is already open"],
        state: Some(State::Todo),
        no_op: true,
        ..BASE
    },
    Case {
        name: "17.28 reopen blocked",
        todo: "A id:a\nx 2026-09-29 B dep:a\n",
        refs: "2",
        ops: &[Op::Reopen],
        want: "A id:a\nB dep:a\n",
        msgs: &["2 reopened"],
        state: Some(State::Todo),
        blocked: true,
        ..BASE
    },
    Case {
        name: "17.29 do then reopen",
        todo: "(B) 2026-09-20 Call +w @p due:2026-10-01\n",
        ops: &[Op::Do, Op::Reopen],
        want: "(B) 2026-09-20 Call +w @p due:2026-10-01\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "reopen keeps pri tags whose removal would change a stamp",
        todo: "x 2026-09-29 pri:A 2026-09-20 Buy\n",
        ops: &[Op::Reopen],
        want: "(A) pri:A 2026-09-20 Buy\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
    Case {
        name: "reopen a line from another tool keeps its priority",
        todo: "x (A) 2026-09-29 2026-09-20 Fix\n",
        ops: &[Op::Reopen],
        want: "(A) 2026-09-20 Fix\n",
        msgs: &["1 reopened"],
        state: Some(State::Todo),
        ..BASE
    },
];

const BLANK: &[(&str, Op)] = &[
    ("17.30 start blank", Op::Start),
    ("17.30 stop blank", Op::Stop),
    ("17.30 do blank", Op::Do),
    ("17.30 reopen blank", Op::Reopen),
];

fn check(case: &Case) {
    let dir = folder(case.todo, case.done);
    let mut config = dir.config.clone();
    config.keep_priority = case.keep;
    let env = Env::new(&dir.found, &config, &dir.clock);
    let mut last = None;
    for &op in case.ops {
        last = Some(apply(&env, op, case.refs).expect(case.name));
    }
    let out = last.expect("an operation");
    assert_eq!(dir.todo(), case.want, "{}", case.name);
    assert_eq!(out.messages(), said(case.msgs), "{}", case.name);
    assert_eq!(out.no_op(), case.no_op, "{}", case.name);
    let item = out.items_after().next();
    if let (Some(want), Some(item)) = (case.state, item) {
        assert_eq!(item.state(), want, "{}", case.name);
    }
    if case.blocked {
        assert!(
            item.is_some_and(crate::list::Item::blocked),
            "{}",
            case.name
        );
    }
}

#[test]
fn lifecycle_rows() {
    for case in CASES_A.iter().chain(CASES_B).chain(CASES_C) {
        check(case);
    }
}

#[test]
fn blank_lines_are_already_deleted() {
    for &(name, op) in BLANK {
        let dir = folder("A\n\nB\n", None);
        let out = apply(&dir.env(), op, "2").expect(name);
        assert_eq!(out.messages(), said(&["2 already deleted"]), "{name}");
        assert!(out.no_op(), "{name}");
        assert_eq!(dir.todo(), "A\n\nB\n", "{name}");
    }
}

#[test]
fn start_and_do_follow_the_zone() {
    let dir = folder("A\n", None);
    let tz = jiff::tz::TimeZone::get("Australia/Sydney").expect("zone");
    let var =
        |name: &str| (name == "LSTY_NOW").then(|| "2026-09-29T2230Z".into());
    let clock =
        crate::dates::Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, tz)
            .expect("clock");
    let env = Env::new(&dir.found, &dir.config, &clock);
    apply(&env, Op::Start, "1").expect("start");
    apply(&env, Op::Do, "1").expect("do");
    assert_eq!(dir.todo(), "x 2026-09-30 A started:2026-09-29T2230Z\n");
}

/// Name, file, refs, op, code, message, hint, state field.
type ErrRow = (
    &'static str,
    &'static str,
    &'static str,
    Op,
    Code,
    &'static str,
    &'static str,
    &'static str,
);

const STAMP_WORD_HINT: &str =
    "the text would start with a stamp-like word; reword it first";

const ERRORS: &[ErrRow] = &[
    (
        "17.3 start done",
        "x 2026-09-28 Fix\n",
        "1",
        Op::Start,
        Code::WrongState,
        "1 is done",
        "use lsty reopen 1 first",
        "done",
    ),
    (
        "17.4 start blocked",
        "A id:a\nB dep:a\n",
        "2",
        Op::Start,
        Code::Blocked,
        "item 2 is blocked by 1 (A)",
        "finish what it waits for first, or use --ignore-blocked",
        "",
    ),
    (
        "17.6 start in a cycle",
        "A id:a dep:b\nB id:b dep:a\n",
        "1",
        Op::Start,
        Code::Blocked,
        "item 1 is blocked by 2 (B) (dependency cycle)",
        "finish what it waits for first, or use --ignore-blocked",
        "",
    ),
    (
        "cycle also names prerequisites outside it",
        "A id:a dep:b dep:c\nB id:b dep:a\nC id:c\n",
        "1",
        Op::Start,
        Code::Blocked,
        "item 1 is blocked by 2 (B), 3 (C) (dependency cycle)",
        "finish what it waits for first, or use --ignore-blocked",
        "",
    ),
    (
        "stop that would empty",
        "started:2026-09-29T0800Z\nB\n",
        "1",
        Op::Stop,
        Code::WrongState,
        "1 would be empty",
        "use lsty del 1",
        "doing",
    ),
    (
        "stop that would leave a bare priority",
        "(A) started:2026-09-29T0800Z owner:x\n",
        "1",
        Op::Stop,
        Code::WrongState,
        "1 would be empty",
        "use lsty del 1",
        "doing",
    ),
    (
        "reopen that would leave a bare priority",
        "x 2026-09-29 pri:A\n",
        "1",
        Op::Reopen,
        Code::WrongState,
        "1 would be empty",
        "use lsty del 1",
        "done",
    ),
    (
        "stop a claim before a done mark word",
        "started:2026-09-29T0900Z x marks spot\n",
        "1",
        Op::Stop,
        Code::WrongState,
        "1: a text edit can't change a stamp",
        STAMP_WORD_HINT,
        "doing",
    ),
    (
        "keep priority replacing a pri tag before a date word",
        "(A) pri:B 2026-09-01 foo\n",
        "1",
        Op::Do,
        Code::WrongState,
        "1: a text edit can't change a stamp",
        STAMP_WORD_HINT,
        "todo",
    ),
    (
        "17.16 do blocked",
        "A id:a\nB dep:a\n",
        "2",
        Op::Do,
        Code::Blocked,
        "item 2 is blocked by 1 (A)",
        "finish what it waits for first, or use --ignore-blocked",
        "",
    ),
    (
        "17.18 do several with a blocked one",
        "A id:a\nB dep:a\nC\n",
        "3,2",
        Op::Do,
        Code::Blocked,
        "item 2 is blocked by 1 (A)",
        "finish what it waits for first, or use --ignore-blocked",
        "",
    ),
];

#[test]
fn lifecycle_errors() {
    for &(name, todo, refs, op, code, msg, hint, state) in ERRORS {
        let dir = folder(todo, None);
        let err = apply(&dir.env(), op, refs).expect_err(name);
        let (got_code, got_msg, got_hint) = parts(&err);
        assert_eq!((got_code, got_msg), (code, msg), "{name}");
        if !hint.is_empty() {
            assert_eq!(got_hint, hint, "{name}");
        }
        if !state.is_empty() {
            assert_eq!(err.text("state"), Some(state), "{name}");
        }
        assert_eq!(dir.todo(), todo, "{name}: the file changed");
    }
}

#[test]
fn blocked_error_names_the_blockers() {
    let dir = folder("A id:a\nB dep:a\n", None);
    let err = apply(&dir.env(), Op::Start, "2").expect_err("blocked");
    let Some(crate::errs::Field::Refs(blockers)) = err.field("blockers")
    else {
        panic!("no blockers");
    };
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].nr.to_string(), "1");
    assert_eq!(blockers[0].id.as_deref(), Some("a"));
    assert_eq!(blockers[0].source, crate::errs::Source::Todo);
}

#[test]
fn reopen_of_an_archived_item() {
    let dir = folder("A\n", Some("x 2026-09-28 B\n"));
    let err = apply(&dir.env(), Op::Reopen, "done:1").expect_err("archived");
    let (code, _, hint) = parts(&err);
    assert_eq!(code, Code::WrongState);
    assert_eq!(hint, "done:1 is archived; use restore");
}

#[test]
fn done_line_with_invalid_utf8_is_echoed_raw() {
    let dir = folder(b"caf\xe9\n", None);
    let out = apply(&dir.env(), Op::Do, "1").expect("do");
    assert_eq!(out.messages(), vec![b"1 done: caf\xe9".to_vec()]);
}

#[test]
fn do_completes_an_open_done_file_line_in_place() {
    let dir = folder("A\n", Some("B\n"));
    let out = apply(&dir.env(), Op::Do, "done:1").expect("do");
    assert_eq!(out.messages(), said(&["done:1 done: B"]));
    assert_eq!(dir.done(), "x 2026-09-29 B\n");
    assert_eq!(dir.todo(), "A\n");
}
