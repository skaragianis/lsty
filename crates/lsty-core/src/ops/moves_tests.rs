use std::cell::{Cell, RefCell};
use std::fs;
use std::io;
use std::path::Path as FsPath;
use std::time::Duration;

use super::test_util::{Dir, folder, parts, said};
use super::{
    Actor, ClaimOpts, DoOpts, Env, Outcome, archive, finish, parse_refs,
    restore,
};
use crate::errs::{Code, Result, Source};
use crate::fsio::{self, Hooks};
use crate::lock::{self, Cancel, Mode, Want};

const B: &str = "x 2026-09-28 B";
const C: &str = "x 2026-09-27 C";
const Z: &str = "x 2026-09-01 Z";

fn lines(rows: &[&str]) -> String {
    let mut out = rows.join(
        "
",
    );
    out.push('\n');
    out
}

#[derive(Clone, Copy)]
enum Op {
    Archive,
    Do(Option<bool>),
    Restore(bool),
}

fn apply(env: &Env<'_>, op: Op, refs: &str) -> Result<Outcome> {
    let targets = parse_refs(refs)?;
    match op {
        Op::Archive => archive(env),
        Op::Do(archive) => {
            let opts = DoOpts {
                archive,
                ..DoOpts::default()
            };
            finish(env, &targets, opts)
        }
        Op::Restore(takeover) => {
            restore(env, &targets, ClaimOpts { takeover })
        }
    }
}

/// Runs `op` `times` times and gives the last outcome.
fn repeat(env: &Env<'_>, op: Op, refs: &str, times: usize) -> Outcome {
    let mut last = None;
    for _ in 0..times {
        last = Some(apply(env, op, refs).expect("operation"));
    }
    last.expect("one run")
}

fn read_done(dir: &Dir) -> Option<String> {
    fs::read(&dir.found.done)
        .ok()
        .map(|b| String::from_utf8(b).expect("utf8"))
}

struct Case {
    name: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    op: Op,
    auto: bool,
    agent: &'static str,
    refs: &'static str,
    times: usize,
    want_todo: &'static str,
    want_done: Option<&'static str>,
    msgs: &'static [&'static str],
    no_op: bool,
    renumbered: bool,
}

const BASE: Case = Case {
    name: "",
    todo: "",
    done: None,
    op: Op::Archive,
    auto: false,
    agent: "",
    refs: "1",
    times: 1,
    want_todo: "",
    want_done: None,
    msgs: &[],
    no_op: false,
    renumbered: false,
};

fn check(case: &Case) {
    let mut dir = folder(case.todo, case.done);
    dir.config.auto_archive = case.auto;
    let mut env = dir.env();
    env.actor = Actor::named(case.agent).expect("actor");
    let out = repeat(&env, case.op, case.refs, case.times);
    let name = case.name;
    assert_eq!(dir.todo(), case.want_todo, "{name}: todo");
    assert_eq!(read_done(&dir).as_deref(), case.want_done, "{name}: done");
    assert_eq!(out.messages, said(case.msgs), "{name}: messages");
    assert_eq!(out.no_op, case.no_op, "{name}: no-op");
    assert_eq!(out.renumbered, case.renumbered, "{name}: renumbered");
}

const ARCHIVE_ROWS: &[Case] = &[
    Case {
        name: "21.1 archive",
        todo: "A\nx 2026-09-28 B\n\nx 2026-09-27 C\nD\n",
        done: Some("x 2026-09-01 Z\n"),
        want_todo: "A\nD\n",
        want_done: Some("x 2026-09-01 Z\nx 2026-09-28 B\nx 2026-09-27 C\n"),
        msgs: &["archived 2 to done.txt", "removed 1 blank lines"],
        renumbered: true,
        ..BASE
    },
    Case {
        name: "21.3 archive again",
        todo: "A\nx 2026-09-28 B\n\nx 2026-09-27 C\nD\n",
        done: Some("x 2026-09-01 Z\n"),
        times: 2,
        want_todo: "A\nD\n",
        want_done: Some("x 2026-09-01 Z\nx 2026-09-28 B\nx 2026-09-27 C\n"),
        msgs: &["nothing to archive"],
        no_op: true,
        ..BASE
    },
    Case {
        name: "21.4 blank lines without a done file",
        todo: "A\n\nB\n",
        want_todo: "A\nB\n",
        msgs: &["removed 1 blank lines"],
        renumbered: true,
        ..BASE
    },
    Case {
        name: "21.5 done file compacted",
        todo: "A\nB\n",
        done: Some("x 2026-09-01 Z\n\n\nx 2026-09-28 B\n"),
        want_todo: "A\nB\n",
        want_done: Some("x 2026-09-01 Z\nx 2026-09-28 B\n"),
        msgs: &["removed 2 blank lines"],
        renumbered: true,
        ..BASE
    },
    Case {
        name: "21.6 done file created",
        todo: "x 2026-09-28 B\n",
        want_todo: "",
        want_done: Some("x 2026-09-28 B\n"),
        msgs: &["archived 1 to done.txt"],
        ..BASE
    },
    Case {
        name: "21.7 over an interrupted line",
        todo: "x 2026-09-28 B id:k\n",
        done: Some("x 2026-09-28 B-old id:k\nx 2026-09-01 Z\n"),
        want_todo: "",
        want_done: Some("x 2026-09-28 B id:k\nx 2026-09-01 Z\n"),
        msgs: &[
            "archived 1 to done.txt",
            "finished an interrupted move of id:k",
        ],
        ..BASE
    },
    Case {
        name: "21.8 dependent kept",
        todo: "A id:a\nx 2026-09-28 B dep:a\n",
        want_todo: "A id:a\n",
        want_done: Some("x 2026-09-28 B dep:a\n"),
        msgs: &["archived 1 to done.txt"],
        ..BASE
    },
    Case {
        name: "21.9 blocker archived",
        todo: "x 2026-09-28 A id:a\nB dep:a\n",
        want_todo: "B dep:a\n",
        want_done: Some("x 2026-09-28 A id:a\n"),
        msgs: &["archived 1 to done.txt"],
        renumbered: true,
        ..BASE
    },
];

#[test]
fn archive_rows() {
    for case in ARCHIVE_ROWS {
        check(case);
    }
}

#[test]
fn archive_items_and_before() {
    let dir = folder(&lines(&["A", B, "", C, "D"]), Some(&lines(&[Z])));
    let out = archive(&dir.env()).unwrap();
    let after: Vec<_> = out
        .items_after()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(after, [(Source::Done, 2), (Source::Done, 3)], "21.2");
    let before: Vec<_> = out
        .items_before()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(before, [(Source::Todo, 2), (Source::Todo, 4)], "21.2");
}

/// Counts the renames.
#[derive(Default)]
struct Renames(Cell<usize>);

impl Hooks for Renames {
    fn before_rename(&self, _: &FsPath) -> io::Result<()> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

#[test]
fn archive_with_nothing_to_do_writes_nothing() {
    let dir = folder("A\n", Some(&lines(&[Z])));
    let hooks = Renames::default();
    let mut env = dir.env();
    env.hooks = &hooks;
    let out = archive(&env).unwrap();
    assert!(out.no_op && out.undo.is_none(), "21.3");
    assert_eq!(hooks.0.get(), 0, "21.3");
    assert_eq!(out.messages, said(&["nothing to archive"]), "21.3");
}

const DO_ROWS: &[Case] = &[
    Case {
        name: "21.10 auto archive",
        todo: "A\nB\nC\n",
        auto: true,
        refs: "2",
        want_todo: "A\n\nC\n",
        want_done: Some("x 2026-09-29 B\n"),
        msgs: &["2 done: B", "archived to done.txt as done:1"],
        op: Op::Do(None),
        ..BASE
    },
    Case {
        name: "21.11 do again",
        todo: "A\nB\nC\n",
        auto: true,
        refs: "2",
        times: 2,
        want_todo: "A\n\nC\n",
        want_done: Some("x 2026-09-29 B\n"),
        msgs: &["2 already deleted"],
        no_op: true,
        op: Op::Do(None),
        ..BASE
    },
    Case {
        name: "21.12 by id again",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        auto: true,
        agent: "agent1",
        refs: "id:a",
        times: 2,
        want_todo: "\n",
        want_done: Some(
            "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1\n",
        ),
        msgs: &["a is already done"],
        no_op: true,
        op: Op::Do(None),
        ..BASE
    },
    Case {
        name: "21.13 no-archive wins",
        todo: "A\n",
        auto: true,
        want_todo: "x 2026-09-29 A\n",
        msgs: &["1 done: A"],
        op: Op::Do(Some(false)),
        ..BASE
    },
    Case {
        name: "21.14 archive flag wins",
        todo: "A\n",
        want_todo: "\n",
        want_done: Some("x 2026-09-29 A\n"),
        msgs: &["1 done: A", "archived to done.txt as done:1"],
        op: Op::Do(Some(true)),
        ..BASE
    },
    Case {
        name: "21.15 spawn below the blank",
        todo: "A rec:1w\nB\n",
        auto: true,
        want_todo: "\n2026-09-29 A rec:1w due:2026-10-06\nB\n",
        want_done: Some("x 2026-09-29 A rec:1w\n"),
        msgs: &[
            "1 done: A",
            "archived to done.txt as done:1",
            "2 next: A (due 2026-10-06)",
        ],
        renumbered: true,
        op: Op::Do(None),
        ..BASE
    },
    Case {
        name: "a rec: line of the done file turns archiving off",
        todo: "A\n",
        done: Some("x 2026-09-01 Z\nOther rec:1w\n"),
        auto: true,
        refs: "1,done:2",
        want_todo: "x 2026-09-29 A\n2026-09-29 Other rec:1w due:2026-10-06\n",
        want_done: Some("x 2026-09-01 Z\nx 2026-09-29 Other rec:1w\n"),
        msgs: &[
            "1 done: A",
            "done:2 done: Other",
            "2 next: Other (due 2026-10-06)",
        ],
        op: Op::Do(None),
        ..BASE
    },
    Case {
        name: "21.16 old done stays",
        todo: "x 2026-09-28 Old\nA\n",
        auto: true,
        refs: "2",
        want_todo: "x 2026-09-28 Old\n\n",
        want_done: Some("x 2026-09-29 A\n"),
        msgs: &["2 done: A", "archived to done.txt as done:1"],
        op: Op::Do(None),
        ..BASE
    },
];

#[test]
fn do_with_archive_rows() {
    for case in DO_ROWS {
        check(case);
    }
}

#[test]
fn do_with_archive_items_and_before() {
    let mut dir = folder("A\nB\n", Some(&lines(&[Z])));
    dir.config.auto_archive = true;
    let out = finish(&dir.env(), &parse_refs("2").unwrap(), DoOpts::default())
        .unwrap();
    let after: Vec<_> = out
        .items_after()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(after, [(Source::Done, 2)], "21.10");
    let before: Vec<_> = out
        .items_before()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(before, [(Source::Todo, 2)], "21.10");
}

const RESTORE_ROWS: &[Case] = &[
    Case {
        name: "21.17 restore",
        todo: "A\n",
        done: Some("x 2026-09-01 Z\nx 2026-09-28 2026-09-20 Fix pri:B\n"),
        refs: "done:2",
        want_todo: "A\n(B) 2026-09-20 Fix\n",
        want_done: Some("x 2026-09-01 Z\n\n"),
        msgs: &["done:2 restored as 2"],
        op: Op::Restore(false),
        ..BASE
    },
    Case {
        name: "21.18 restore again",
        todo: "A\n",
        done: Some("x 2026-09-01 Z\nx 2026-09-28 2026-09-20 Fix pri:B\n"),
        refs: "done:2",
        times: 2,
        want_todo: "A\n(B) 2026-09-20 Fix\n",
        want_done: Some("x 2026-09-01 Z\n\n"),
        msgs: &["done:2 already restored"],
        no_op: true,
        op: Op::Restore(false),
        ..BASE
    },
    Case {
        name: "21.19 back to the finisher",
        done: Some(
            "x 2026-09-28 Fix id:k started:2026-09-29T0800Z owner:agent1\n",
        ),
        agent: "agent2",
        refs: "id:k",
        want_todo: "Fix id:k started:2026-09-29T0800Z owner:agent1\n",
        want_done: Some("\n"),
        msgs: &["k restored as 1; back to agent1"],
        op: Op::Restore(false),
        ..BASE
    },
    Case {
        name: "21.19 takeover",
        done: Some(
            "x 2026-09-28 Fix id:k started:2026-09-29T0800Z owner:agent1\n",
        ),
        agent: "agent2",
        refs: "id:k",
        want_todo: "Fix id:k started:2026-09-29T0900Z owner:agent2\n",
        want_done: Some("\n"),
        msgs: &["k restored as 1; now yours"],
        op: Op::Restore(true),
        ..BASE
    },
    Case {
        name: "21.20 already in todo",
        todo: "Fix id:k\n",
        refs: "id:k",
        want_todo: "Fix id:k\n",
        msgs: &["k is already in todo.txt"],
        no_op: true,
        op: Op::Restore(false),
        ..BASE
    },
    Case {
        name: "21.21 in both files",
        todo: "Fix id:k\n",
        done: Some("x 2026-09-28 Fix id:k\n"),
        refs: "id:k",
        want_todo: "Fix id:k\n",
        want_done: Some("\n"),
        msgs: &["finished an interrupted move of id:k"],
        op: Op::Restore(false),
        ..BASE
    },
    Case {
        name: "21.23 two in order",
        todo: "A\n",
        done: Some("x 2026-09-28 B\nx 2026-09-27 C\n"),
        refs: "done:1,done:2",
        want_todo: "A\nB\nC\n",
        want_done: Some("\n\n"),
        msgs: &["done:1 restored as 2", "done:2 restored as 3"],
        op: Op::Restore(false),
        ..BASE
    },
    Case {
        name: "a line that is not done",
        todo: "A\n",
        done: Some("Other id:o\n"),
        refs: "done:1",
        want_todo: "A\nOther id:o\n",
        want_done: Some("\n"),
        msgs: &["done:1 restored as 2"],
        op: Op::Restore(false),
        ..BASE
    },
];

#[test]
fn restore_rows() {
    for case in RESTORE_ROWS {
        check(case);
    }
}

#[test]
fn restore_items() {
    let dir = folder("A\n", Some(&lines(&[B])));
    let out = restore(
        &dir.env(),
        &parse_refs("done:1").unwrap(),
        ClaimOpts::default(),
    )
    .unwrap();
    let items: Vec<_> = out
        .items_after()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(items, [(Source::Todo, 2)], "21.17");
    assert!(!out.renumbered, "21.17");
    let before: Vec<_> = out
        .items_before()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(before, [(Source::Done, 1)], "21.17");
}

#[test]
fn restore_of_a_todo_number_is_a_usage_error() {
    let dir = folder("A\nB\nC\n", Some(&lines(&[B])));
    let err =
        restore(&dir.env(), &parse_refs("3").unwrap(), ClaimOpts::default())
            .unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::Usage,
            "3 is not an archived item",
            "use done:NR or id:"
        ),
        "21.22"
    );
}

/// The files renamed over, in order.
#[derive(Default)]
struct Order(RefCell<Vec<String>>);

impl Hooks for Order {
    fn before_rename(&self, target: &FsPath) -> io::Result<()> {
        let name = target.file_name().unwrap_or_default();
        self.0
            .borrow_mut()
            .push(name.to_string_lossy().into_owned());
        Ok(())
    }
}

struct OrderRow {
    name: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    auto: bool,
    op: Op,
    refs: &'static str,
    want: [&'static str; 2],
}

const ORDER_ROWS: &[OrderRow] = &[
    OrderRow {
        name: "21.24 archive",
        todo: "x 2026-09-28 B\n",
        done: None,
        auto: false,
        op: Op::Archive,
        refs: "1",
        want: ["done.txt", "todo.txt"],
    },
    OrderRow {
        name: "21.24 do with auto-archive",
        todo: "A\n",
        done: None,
        auto: true,
        op: Op::Do(None),
        refs: "1",
        want: ["done.txt", "todo.txt"],
    },
    OrderRow {
        name: "21.24 restore",
        todo: "A\n",
        done: Some("x 2026-09-28 B\n"),
        auto: false,
        op: Op::Restore(false),
        refs: "done:1",
        want: ["todo.txt", "done.txt"],
    },
    OrderRow {
        name: "21.23 two restored, one write per file",
        todo: "A\n",
        done: Some("x 2026-09-28 B\nx 2026-09-27 C\n"),
        auto: false,
        op: Op::Restore(false),
        refs: "done:1,done:2",
        want: ["todo.txt", "done.txt"],
    },
];

#[test]
fn write_orders() {
    for row in ORDER_ROWS {
        let mut dir = folder(row.todo, row.done);
        dir.config.auto_archive = row.auto;
        let hooks = Order::default();
        let mut env = dir.env();
        env.hooks = &hooks;
        apply(&env, row.op, row.refs).expect(row.name);
        assert_eq!(*hooks.0.borrow(), row.want, "{}", row.name);
    }
}

#[test]
fn a_move_locks_both_folders() {
    let mut dir = folder(&lines(&[B]), None);
    let other = tempfile::tempdir().unwrap();
    let other_dir = fs::canonicalize(other.path()).unwrap();
    dir.found.done = other_dir.join("done.txt");
    let mut env = dir.env();
    env.timeout = Duration::from_millis(200);
    let wants = [Want {
        file: dir.found.done.clone(),
        mode: Mode::Exclusive,
    }];
    let mut held =
        lock::acquire(&wants, Duration::from_secs(1), &Cancel::new()).unwrap();
    let err = archive(&env).unwrap_err();
    assert_eq!(err.code(), Code::LockTimeout, "21.25");
    held.release().unwrap();
    archive(&env).unwrap();
    assert_eq!(dir.done(), lines(&[B]), "21.25");
}

#[test]
fn do_without_archive_cleans_done_orphans() {
    let dir = folder("A\n", Some(&lines(&[Z, "Other"])));
    let orphan = dir.dir().join(".done.txt.lsty-99.tmp");
    fs::write(&orphan, "junk").unwrap();
    let env = dir.env();
    finish(&env, &parse_refs("done:2").unwrap(), DoOpts::default()).unwrap();
    assert!(!orphan.exists());
}

#[test]
fn restore_refuses_an_emptied_line() {
    for takeover in [false, true] {
        let dir = folder("A\n", Some("x 2026-09-29 pri:A\n"));
        let err = restore(
            &dir.env(),
            &parse_refs("done:1").unwrap(),
            ClaimOpts { takeover },
        )
        .unwrap_err();
        assert_eq!(
            parts(&err),
            (
                Code::WrongState,
                "done:1 would be empty",
                "use lsty del done:1"
            ),
            "takeover {takeover}"
        );
        assert_eq!(dir.todo(), "A\n");
        assert_eq!(dir.done(), "x 2026-09-29 pri:A\n");
    }
}

#[test]
fn moved_lines_sharing_an_id_are_all_kept() {
    let dir = folder(
        &lines(&["x 2026-09-28 B id:k", "x 2026-09-27 C id:k"]),
        Some(&lines(&[Z, "x 2026-09-01 old id:k"])),
    );
    archive(&dir.env()).unwrap();
    let want = lines(&[Z, "x 2026-09-28 B id:k", "x 2026-09-27 C id:k"]);
    assert_eq!(dir.done(), want);
    assert_eq!(dir.todo(), "");

    let dir = folder(
        &lines(&["Fix id:k"]),
        Some(&lines(&["x 2026-09-28 B id:k", "x 2026-09-27 C id:k"])),
    );
    let targets = parse_refs("done:1,done:2").unwrap();
    restore(&dir.env(), &targets, ClaimOpts::default()).unwrap();
    assert_eq!(dir.todo(), lines(&["Fix id:k", "C id:k"]));
    assert_eq!(dir.done(), "\n\n");
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Crash {
    BeforeFirst,
    AfterFirst,
    AfterSecond,
}

/// Fails with a simulated crash at the named point of the first or
/// second write.
struct Crashing {
    at: Crash,
    writes: Cell<usize>,
}

impl Crashing {
    const fn new(at: Crash) -> Self {
        Self {
            at,
            writes: Cell::new(0),
        }
    }
}

impl Hooks for Crashing {
    fn before_rename(&self, _: &FsPath) -> io::Result<()> {
        self.writes.set(self.writes.get() + 1);
        if self.at == Crash::BeforeFirst && self.writes.get() == 1 {
            return Err(fsio::crash());
        }
        Ok(())
    }

    fn after_rename(&self, _: &FsPath) -> io::Result<()> {
        let writes = self.writes.get();
        let first = self.at == Crash::AfterFirst && writes == 1;
        let second = self.at == Crash::AfterSecond && writes == 2;
        if first || second {
            return Err(fsio::crash());
        }
        Ok(())
    }
}

struct CrashRow {
    name: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    auto: bool,
    agent: &'static str,
    op: Op,
    refs: &'static str,
    at: Crash,
    after: (&'static str, Option<&'static str>),
    retry: (&'static str, Option<&'static str>),
    msgs: &'static [&'static str],
    no_op: bool,
}

const AGENT_A: &str = "A id:a started:2026-09-29T0800Z owner:agent1\n";
const DONE_A: &str =
    "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1\n";
const ARCH_IN: &str = "x 2026-09-28 B id:k\nx 2026-09-27 C\n";
const FIX_DONE: &str = "x 2026-09-28 Fix id:k\n";

const CRASH_BASE: CrashRow = CrashRow {
    name: "",
    todo: "",
    done: None,
    auto: false,
    agent: "",
    op: Op::Archive,
    refs: "1",
    at: Crash::AfterFirst,
    after: ("", None),
    retry: ("", None),
    msgs: &[],
    no_op: false,
};

const CRASH_ROWS: &[CrashRow] = &[
    CrashRow {
        name: "21.26 archive, crash after the first",
        todo: ARCH_IN,
        after: (ARCH_IN, Some(ARCH_IN)),
        retry: (
            "",
            Some("x 2026-09-28 B id:k\nx 2026-09-27 C\nx 2026-09-27 C\n"),
        ),
        msgs: &[
            "archived 2 to done.txt",
            "finished an interrupted move of id:k",
        ],
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.27 archive, crash after the second",
        todo: ARCH_IN,
        at: Crash::AfterSecond,
        after: ("", Some(ARCH_IN)),
        retry: ("", Some(ARCH_IN)),
        msgs: &["nothing to archive"],
        no_op: true,
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.28 do, crash after the first",
        todo: AGENT_A,
        auto: true,
        agent: "agent1",
        op: Op::Do(None),
        refs: "id:a",
        after: (AGENT_A, Some(DONE_A)),
        retry: ("\n", Some(DONE_A)),
        msgs: &[
            "a done: A",
            "archived to done.txt as done:1",
            "finished an interrupted move of id:a",
        ],
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.29 do, crash after the second",
        todo: AGENT_A,
        auto: true,
        agent: "agent1",
        op: Op::Do(None),
        refs: "id:a",
        at: Crash::AfterSecond,
        after: ("\n", Some(DONE_A)),
        retry: ("\n", Some(DONE_A)),
        msgs: &["a is already done"],
        no_op: true,
        ..CRASH_BASE
    },
    CrashRow {
        name: "do of a todo item and an open done-file line with rec:",
        todo: "A\n",
        done: Some("x 2026-09-01 Z\nOther rec:1w\n"),
        auto: true,
        op: Op::Do(None),
        refs: "1,done:2",
        after: (
            "x 2026-09-29 A\n2026-09-29 Other rec:1w due:2026-10-06\n",
            Some("x 2026-09-01 Z\nOther rec:1w\n"),
        ),
        retry: (
            "x 2026-09-29 A\n2026-09-29 Other rec:1w due:2026-10-06\n\
             2026-09-29 Other rec:1w due:2026-10-06\n",
            Some("x 2026-09-01 Z\nx 2026-09-29 Other rec:1w\n"),
        ),
        msgs: &[
            "1 is already done",
            "done:2 done: Other",
            "3 next: Other (due 2026-10-06)",
        ],
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.30 restore, crash after the first",
        todo: "A\n",
        done: Some(FIX_DONE),
        op: Op::Restore(false),
        refs: "id:k",
        after: ("A\nFix id:k\n", Some(FIX_DONE)),
        retry: ("A\nFix id:k\n", Some("\n")),
        msgs: &["finished an interrupted move of id:k"],
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.31 restore, crash after the second",
        todo: "A\n",
        done: Some(FIX_DONE),
        op: Op::Restore(false),
        refs: "id:k",
        at: Crash::AfterSecond,
        after: ("A\nFix id:k\n", Some("\n")),
        retry: ("A\nFix id:k\n", Some("\n")),
        msgs: &["k is already in todo.txt"],
        no_op: true,
        ..CRASH_BASE
    },
];

fn crash_dir(row: &CrashRow) -> Dir {
    let mut dir = folder(row.todo, row.done);
    dir.config.auto_archive = row.auto;
    dir
}

fn files_of(dir: &Dir) -> (String, Option<String>) {
    (dir.todo(), read_done(dir))
}

fn expect_files(dir: &Dir, want: (&str, Option<&str>), name: &str) {
    let (todo, done) = files_of(dir);
    assert_eq!(todo, want.0, "{name}: todo");
    assert_eq!(done.as_deref(), want.1, "{name}: done");
}

#[test]
fn a_crash_after_a_rename_then_a_retry() {
    for row in CRASH_ROWS {
        let dir = crash_dir(row);
        let hooks = Crashing::new(row.at);
        let mut env = dir.env();
        env.actor = Actor::named(row.agent).unwrap();
        env.hooks = &hooks;
        let err = apply(&env, row.op, row.refs).unwrap_err();
        assert!(fsio::is_crash(&err), "{}: {err}", row.name);
        expect_files(&dir, row.after, row.name);
        env.hooks = &fsio::OsHooks;
        let out = apply(&env, row.op, row.refs).expect(row.name);
        expect_files(&dir, row.retry, row.name);
        assert_eq!(out.messages, said(row.msgs), "{}: retry", row.name);
        assert_eq!(out.no_op, row.no_op, "{}: retry no-op", row.name);
    }
}

const BEFORE_FIRST_ROWS: &[CrashRow] = &[
    CrashRow {
        name: "21.32 archive",
        todo: ARCH_IN,
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.32 archive of one line",
        todo: "x 2026-09-28 B id:k\n",
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.32 do",
        todo: AGENT_A,
        auto: true,
        agent: "agent1",
        op: Op::Do(None),
        refs: "id:a",
        ..CRASH_BASE
    },
    CrashRow {
        name: "21.32 restore",
        todo: "A\n",
        done: Some(FIX_DONE),
        op: Op::Restore(false),
        refs: "id:k",
        ..CRASH_BASE
    },
];

#[test]
fn a_crash_before_the_first_rename_then_a_retry() {
    for row in BEFORE_FIRST_ROWS {
        let clean = crash_dir(row);
        let mut clean_env = clean.env();
        clean_env.actor = Actor::named(row.agent).unwrap();
        let want = apply(&clean_env, row.op, row.refs).expect(row.name);

        let dir = crash_dir(row);
        let hooks = Crashing::new(Crash::BeforeFirst);
        let mut env = dir.env();
        env.actor = Actor::named(row.agent).unwrap();
        env.hooks = &hooks;
        let err = apply(&env, row.op, row.refs).unwrap_err();
        assert!(fsio::is_crash(&err), "{}: {err}", row.name);
        expect_files(&dir, (row.todo, row.done), row.name);
        env.hooks = &fsio::OsHooks;
        let out = apply(&env, row.op, row.refs).expect(row.name);
        assert_eq!(out.messages, want.messages, "{}", row.name);
        assert_eq!(files_of(&dir), files_of(&clean), "{}", row.name);
    }
}

struct RecRow {
    name: &'static str,
    auto: bool,
    want_todo: &'static str,
}

const REC_ROWS: &[RecRow] = &[
    RecRow {
        name: "rec line spawns into todo",
        auto: false,
        want_todo: "A\n2026-09-29 Other rec:1w due:2026-10-06\n",
    },
    RecRow {
        name: "rec line spawns into todo with auto-archive",
        auto: true,
        want_todo: "A\n2026-09-29 Other rec:1w due:2026-10-06\n",
    },
];

#[test]
fn a_rec_line_of_the_done_file_writes_todo_first() {
    for row in REC_ROWS {
        let mut dir = folder("A\n", Some("x 2026-09-01 Z\nOther rec:1w\n"));
        dir.config.auto_archive = row.auto;
        let hooks = Order::default();
        let mut env = dir.env();
        env.hooks = &hooks;
        let out = apply(&env, Op::Do(None), "done:2").expect(row.name);
        assert_eq!(
            *hooks.0.borrow(),
            ["todo.txt", "done.txt"],
            "{}",
            row.name
        );
        assert!(!out.renumbered, "{}", row.name);
        assert_eq!(dir.todo(), row.want_todo, "{}", row.name);
        let done = "x 2026-09-01 Z\nx 2026-09-29 Other rec:1w\n";
        assert_eq!(dir.done(), done, "{}", row.name);
        let msgs = ["done:2 done: Other", "2 next: Other (due 2026-10-06)"];
        assert_eq!(out.messages, said(&msgs), "{}", row.name);
    }
}
