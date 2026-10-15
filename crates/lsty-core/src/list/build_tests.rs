use std::time::Duration;

use jiff::Timestamp;

use super::{
    Claim, Snapshot, State, Task, TaskId,
    test_util::{clock, snapshot},
};
use crate::errs::{Nr, Ref, Source};

fn nr(n: u32) -> Nr {
    Nr::new(n).expect("nr")
}

fn nrs(snap: &Snapshot, ids: &[TaskId]) -> Vec<u32> {
    ids.iter()
        .map(|&id| snap.task(id).expect("item").nr.get())
        .collect()
}

struct TasksRow {
    name: &'static str,
    todo: &'static [&'static str],
    nrs: &'static [u32],
    states: &'static [State],
}

const TASKS: &[TasksRow] = &[
    TasksRow {
        name: "08.1 numbers and states",
        todo: &[
            "Buy",
            "",
            "x 2026-09-28 Fix",
            "Add started:2026-09-29T0800Z",
        ],
        nrs: &[1, 3, 4],
        states: &[State::Todo, State::Done, State::Doing],
    },
    TasksRow {
        name: "08.2 bad started is still doing",
        todo: &["Add started:yes"],
        nrs: &[1],
        states: &[State::Doing],
    },
];

#[test]
fn numbers_and_states() {
    for row in TASKS {
        let snap = snapshot(row.todo, &[]);
        let got: Vec<u32> =
            snap.tasks().iter().map(|i| i.nr().get()).collect();
        assert_eq!(got, row.nrs, "{}", row.name);
        let states: Vec<State> =
            snap.tasks().iter().map(Task::state).collect();
        assert_eq!(states, row.states, "{}", row.name);
    }
}

#[test]
fn order_and_at() {
    let snap = snapshot(&["A", "B"], &["x 2026-09-28 C", "x D"]);
    let got: Vec<(Source, u32)> = snap
        .tasks()
        .iter()
        .map(|i| (i.source, i.nr().get()))
        .collect();
    let want = [
        (Source::Todo, 1),
        (Source::Todo, 2),
        (Source::Done, 1),
        (Source::Done, 2),
    ];
    assert_eq!(got, want, "08.30");
    assert_eq!(snap.at(Source::Done, nr(2)), snap.tasks().get(3));
    assert_eq!(snap.at(Source::Todo, nr(3)), None);
    let task = snap.tasks().get(2).expect("item");
    assert_eq!(snap.line(task).raw(), b"x 2026-09-28 C");
}

#[test]
fn at_skips_blank_lines() {
    let snap = snapshot(&["A", "", "B"], &[]);
    assert!(snap.at(Source::Todo, nr(2)).is_none());
    assert_eq!(snap.at(Source::Todo, nr(3)), snap.tasks().get(1));
}

struct BlockRow {
    name: &'static str,
    todo: &'static [&'static str],
    done: &'static [&'static str],
    blocked: &'static [bool],
    blockers: &'static [&'static [u32]],
    dangling: &'static [&'static [&'static str]],
}

const BLOCKING: &[BlockRow] = &[
    BlockRow {
        name: "08.3 open prerequisite",
        todo: &["A id:a", "B dep:a"],
        done: &[],
        blocked: &[false, true],
        blockers: &[&[], &[1]],
        dangling: &[],
    },
    BlockRow {
        name: "08.4 done prerequisite",
        todo: &["x 2026-09-28 A id:a", "B dep:a"],
        done: &[],
        blocked: &[false, false],
        blockers: &[],
        dangling: &[],
    },
    BlockRow {
        name: "08.5 prerequisite done in the done file",
        todo: &["B dep:a"],
        done: &["x 2026-09-28 A id:a"],
        blocked: &[false],
        blockers: &[],
        dangling: &[&[]],
    },
    BlockRow {
        name: "08.6 dangling",
        todo: &["B dep:zz"],
        done: &[],
        blocked: &[false],
        blockers: &[],
        dangling: &[&["zz"]],
    },
    BlockRow {
        name: "08.7 ids are case sensitive",
        todo: &["B dep:PARSER", "A id:parser"],
        done: &[],
        blocked: &[false, false],
        blockers: &[],
        dangling: &[&["PARSER"], &[]],
    },
    BlockRow {
        name: "08.8 mixed deps",
        todo: &["A id:a", "x 2026-09-28 C id:c", "B dep:c dep:a dep:zz"],
        done: &[],
        blocked: &[false, false, true],
        blockers: &[&[], &[], &[1]],
        dangling: &[&[], &[], &["zz"]],
    },
    BlockRow {
        name: "08.9 done tasks are never blocked",
        todo: &["x 2026-09-28 B dep:a", "A id:a"],
        done: &[],
        blocked: &[false, false],
        blockers: &[],
        dangling: &[],
    },
    BlockRow {
        name: "08.17 first line with the id decides",
        todo: &["x 2026-09-28 A id:k", "B id:k", "C dep:k"],
        done: &[],
        blocked: &[false, false, false],
        blockers: &[],
        dangling: &[],
    },
    BlockRow {
        name: "blockers keep dep order and are listed once",
        todo: &["A id:a", "B id:b", "C dep:b dep:a dep:b"],
        done: &[],
        blocked: &[false, false, true],
        blockers: &[&[], &[], &[2, 1]],
        dangling: &[],
    },
];

#[test]
fn blocking() {
    for row in BLOCKING {
        let snap = snapshot(row.todo, row.done);
        assert!(snap.tasks().len() >= row.blocked.len(), "{}", row.name);
        for (i, task) in
            snap.tasks().iter().take(row.blocked.len()).enumerate()
        {
            let at = format!("{} task {}", row.name, i + 1);
            assert_eq!(Some(&task.blocked()), row.blocked.get(i), "{at}");
            let want = row.blockers.get(i).copied().unwrap_or_default();
            assert_eq!(nrs(&snap, task.blockers()), want, "{at}");
            let want = row.dangling.get(i).copied().unwrap_or_default();
            let want: Vec<&[u8]> = want.iter().map(|d| d.as_bytes()).collect();
            assert_eq!(snap.dangling(task).collect::<Vec<_>>(), want, "{at}");
        }
    }
}

struct CycleRow {
    name: &'static str,
    todo: &'static [&'static str],
    blocked: &'static [bool],
    cycle: &'static [bool],
    first_path: Option<&'static [u32]>,
}

const CYCLES: &[CycleRow] = &[
    CycleRow {
        name: "08.10 two tasks",
        todo: &["A id:a dep:b", "B id:b dep:a"],
        blocked: &[true, true],
        cycle: &[true, true],
        first_path: Some(&[1, 2]),
    },
    CycleRow {
        name: "08.11 self dependency",
        todo: &["A id:a dep:a"],
        blocked: &[true],
        cycle: &[true],
        first_path: Some(&[1]),
    },
    CycleRow {
        name: "08.12 waiting on a cycle",
        todo: &["A id:a dep:b", "B id:b dep:a", "C dep:a"],
        blocked: &[true, true, true],
        cycle: &[true, true, false],
        first_path: Some(&[1, 2]),
    },
    CycleRow {
        name: "08.13 done task breaks the cycle",
        todo: &["A id:a dep:b", "x 2026-09-28 B id:b dep:a"],
        blocked: &[false, false],
        cycle: &[false, false],
        first_path: None,
    },
    CycleRow {
        name: "08.14 three tasks",
        todo: &["A id:a dep:b", "B id:b dep:c", "C id:c dep:a"],
        blocked: &[true, true, true],
        cycle: &[true, true, true],
        first_path: Some(&[1, 2, 3]),
    },
    CycleRow {
        name: "two cycles joined by a bridge",
        todo: &[
            "A id:a dep:b",
            "B id:b dep:a dep:c",
            "C id:c dep:d",
            "D id:d dep:c",
        ],
        blocked: &[true, true, true, true],
        cycle: &[true, true, true, true],
        first_path: Some(&[1, 2]),
    },
    CycleRow {
        name: "a chain is no cycle",
        todo: &["A id:a dep:b", "B id:b dep:c", "C id:c"],
        blocked: &[true, true, false],
        cycle: &[false, false, false],
        first_path: None,
    },
];

#[test]
fn cycles() {
    for row in CYCLES {
        let snap = snapshot(row.todo, &[]);
        let blocked: Vec<bool> =
            snap.tasks().iter().map(Task::blocked).collect();
        let cycle: Vec<bool> = snap.tasks().iter().map(Task::cycle).collect();
        assert_eq!(blocked, row.blocked, "{}", row.name);
        assert_eq!(cycle, row.cycle, "{}", row.name);
        let path = snap.cycle_path(snap.id_for(0));
        let got = path.map(|p| nrs(&snap, &p));
        assert_eq!(got.as_deref(), row.first_path, "{}", row.name);
        for task in snap.tasks() {
            let has = snap.cycle_path(task.task_id()).is_some();
            assert_eq!(has, task.cycle, "{} {}", row.name, task.nr);
        }
    }
}

#[test]
fn waiting_on_agrees_with_the_blocker_paths() {
    for row in CYCLES {
        let snap = snapshot(row.todo, &[]);
        for to in snap.tasks() {
            let waiting = snap.waiting_on(to.task_id());
            for from in snap.tasks() {
                let (a, b) = (from.task_id(), to.task_id());
                let path = snap.path_to(a, b).is_some();
                let same = a == b;
                let got = waiting.get(a.index()) == Some(&true);
                assert!(same || got == path, "{}", row.name);
            }
        }
    }
}

#[test]
fn cycle_path_from_the_middle() {
    let snap =
        snapshot(&["A id:a dep:b", "B id:b dep:c", "C id:c dep:a"], &[]);
    let path = snap.cycle_path(snap.id_for(1)).expect("cycle");
    assert_eq!(nrs(&snap, &path), [2, 3, 1], "08.14");
}

#[test]
fn long_chain_and_ring_are_linear() {
    let n = 5000;
    let mut lines: Vec<String> = (0..n)
        .map(|i| format!("T{i} id:i{i} dep:i{}", (i + 1) % n))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let snap = snapshot(&refs, &[]);
    assert!(snap.tasks().iter().all(|i| i.cycle() && i.blocked()));
    let path = snap.cycle_path(snap.id_for(0)).expect("ring");
    assert_eq!(path.len(), n);
    lines.pop();
    lines.push(String::from("end id:i4999"));
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let snap = snapshot(&refs, &[]);
    assert!(snap.tasks().iter().all(|i| !i.cycle()));
}

struct FlagRow {
    name: &'static str,
    todo: &'static [&'static str],
    later: &'static [bool],
    overdue: &'static [bool],
    stale: &'static [bool],
    claim: &'static [Claim],
}

const FLAGS: &[FlagRow] = &[
    FlagRow {
        name: "08.19 threshold",
        todo: &["Pay t:2026-09-30", "Pay t:2026-09-29", "Pay t:soon"],
        later: &[true, false, false],
        overdue: &[],
        stale: &[],
        claim: &[],
    },
    FlagRow {
        name: "08.20 done and later",
        todo: &["x 2026-09-28 Pay t:2026-10-30"],
        later: &[true],
        overdue: &[],
        stale: &[],
        claim: &[],
    },
    FlagRow {
        name: "08.21 overdue",
        todo: &[
            "Pay due:2026-09-28",
            "Pay due:2026-09-29",
            "x 2026-09-29 Pay due:2026-09-01",
        ],
        later: &[],
        overdue: &[true, false, false],
        stale: &[],
        claim: &[],
    },
    FlagRow {
        name: "08.22 stale",
        todo: &["A started:2026-09-29T0459Z owner:agent1"],
        later: &[],
        overdue: &[],
        stale: &[true],
        claim: &[],
    },
    FlagRow {
        name: "08.23 exactly the limit",
        todo: &["A started:2026-09-29T0500Z owner:agent1"],
        later: &[],
        overdue: &[],
        stale: &[false],
        claim: &[],
    },
    FlagRow {
        name: "08.24 human claim",
        todo: &["A started:2026-09-28T0500Z"],
        later: &[],
        overdue: &[],
        stale: &[false],
        claim: &[Claim::Human],
    },
    FlagRow {
        name: "08.25 done keeps its claim",
        todo: &["x 2026-09-29 A started:2026-09-28T0500Z owner:agent1"],
        later: &[],
        overdue: &[],
        stale: &[false],
        claim: &[Claim::Agent],
    },
    FlagRow {
        name: "bad started is never stale",
        todo: &["A started:yes owner:agent1", "B"],
        later: &[],
        overdue: &[],
        stale: &[false, false],
        claim: &[Claim::Agent, Claim::None],
    },
];

fn check<T: PartialEq + std::fmt::Debug>(
    name: &str,
    want: &[T],
    get: impl Fn(usize) -> Option<T>,
) {
    for (i, want) in want.iter().enumerate() {
        assert_eq!(get(i).as_ref(), Some(want), "{name} task {}", i + 1);
    }
}

#[test]
fn later_overdue_stale_claim() {
    for row in FLAGS {
        let snap = snapshot(row.todo, &[]);
        let task = |i: usize| snap.tasks().get(i);
        check(row.name, row.later, |i| task(i).map(Task::later));
        check(row.name, row.overdue, |i| task(i).map(|i| i.overdue));
        check(row.name, row.stale, |i| task(i).map(Task::stale));
        check(row.name, row.claim, |i| task(i).map(Task::claim));
    }
}

struct ElapsedRow {
    name: &'static str,
    line: &'static str,
    want: Option<u64>,
}

const ELAPSED: &[ElapsedRow] = &[
    ElapsedRow {
        name: "08.26 half an hour",
        line: "A started:2026-09-29T0830Z owner:agent1",
        want: Some(30 * 60),
    },
    ElapsedRow {
        name: "08.27 future start",
        line: "A started:2026-09-29T1000Z",
        want: Some(0),
    },
    ElapsedRow {
        name: "not underway",
        line: "A",
        want: None,
    },
    ElapsedRow {
        name: "bad started",
        line: "A started:yes",
        want: None,
    },
    ElapsedRow {
        name: "done without an end",
        line: "x 2026-09-29 A started:2026-09-29T0830Z",
        want: None,
    },
    ElapsedRow {
        name: "done, started and ended",
        line: "x 2026-09-29 A started:2026-09-29T0730Z ended:2026-09-29T0815Z",
        want: Some(45 * 60),
    },
    ElapsedRow {
        name: "done, ended before it started",
        line: "x 2026-09-29 A started:2026-09-29T0830Z ended:2026-09-29T0815Z",
        want: Some(0),
    },
    ElapsedRow {
        name: "done, ended but never started",
        line: "x 2026-09-29 A ended:2026-09-29T0815Z",
        want: None,
    },
    ElapsedRow {
        name: "done, bad end",
        line: "x 2026-09-29 A started:2026-09-29T0830Z ended:soon",
        want: None,
    },
    ElapsedRow {
        name: "open with an end counts to now",
        line: "A started:2026-09-29T0830Z ended:2026-09-29T0835Z",
        want: Some(30 * 60),
    },
];

#[test]
fn elapsed() {
    let now: Timestamp = clock().now();
    for row in ELAPSED {
        let snap = snapshot(&[row.line], &[]);
        let got = snap.tasks().first().and_then(|i| i.elapsed(now));
        let want = row.want.map(Duration::from_secs);
        assert_eq!(got, want, "{}", row.name);
    }
}

#[test]
fn tags_and_line_of_an_task() {
    let snap = snapshot(&["A id:a dep:b due:2026-10-01"], &[]);
    let task = snap.tasks().first().expect("item");
    let tags = snap.tags(task);
    assert_eq!(tags.due, Some("2026-10-01"));
    assert_eq!(snap.id_of(task), Some(b"a".as_slice()));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "task of another snapshot")]
fn a_task_of_another_snapshot_is_caught() {
    let one = snapshot(&["A"], &[]);
    let other = snapshot(&["B"], &[]);
    let task = other.tasks().first().expect("item");
    assert_eq!(one.line(task).raw(), b"A");
}

#[test]
fn files_and_refs_by_source() {
    let snap = snapshot(&["A id:a", "B"], &["x C id:c"]);
    let found = snap.found();
    assert_eq!(found.path(Source::Todo), found.todo);
    assert_eq!(found.path(Source::Done), found.done);
    assert_eq!(snap.file(Source::Todo).lines.len(), 2);
    assert_eq!(snap.file(Source::Done).lines.len(), 1);
    let tasks = snap.tasks();
    let got: Vec<_> = tasks.iter().map(|i| snap.ref_of(i)).collect();
    let r = |nr, source, id: Option<&str>| Ref {
        nr,
        source,
        id: id.map(Into::into),
    };
    assert_eq!(
        got,
        [
            r(nr(1), Source::Todo, Some("a")),
            r(nr(2), Source::Todo, None),
            r(nr(1), Source::Done, Some("c")),
        ]
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "task of another snapshot")]
fn a_task_without_an_id_of_another_snapshot_is_caught() {
    let one = snapshot(&["A"], &[]);
    let other = snapshot(&["B"], &[]);
    let task = other.tasks().first().expect("item");
    assert_eq!(one.ref_of(task).id, None);
}

#[test]
fn a_task_is_named_by_its_line() {
    let snap = snapshot(&["a", "b"], &["x done"]);
    let handles: Vec<String> = snap.tasks().iter().map(Task::handle).collect();
    assert_eq!(handles, ["1", "2", "done:1"]);
}

#[test]
fn state_words() {
    assert_eq!(State::Todo.as_str(), "todo");
    assert_eq!(State::Doing.as_str(), "doing");
    assert_eq!(State::Done.as_str(), "done");
}

#[test]
fn claim_and_threshold_words() {
    let snap = snapshot(
        &[
            "A started:2026-09-29T0800Z",
            "B started:2026-09-29T0800Z owner:ag",
            "x 2026-09-28 C started:2026-09-29T0800Z",
            "x 2026-09-28 D owner:ag",
            "E",
            "F t:2026-09-30",
            "G t:2026-09-29",
        ],
        &[],
    );
    let words: Vec<_> = snap
        .tasks()
        .iter()
        .map(|task| {
            let owner = snap.tags(task).owner;
            task.claim_text(owner)
                .map(|t| String::from_utf8_lossy(&t).into_owned())
        })
        .collect();
    let want = [
        Some("claimed (by you)"),
        Some("claimed by ag"),
        Some("finished (by you)"),
        Some("finished by ag"),
        None,
        None,
        None,
    ];
    assert_eq!(words, want.map(|w| w.map(str::to_owned)));
    let tasks = snap.tasks();
    assert_eq!(tasks[5].threshold_text("2026-09-30"), "2026-09-30 (later)");
    assert_eq!(tasks[6].threshold_text("2026-09-29"), "2026-09-29");
}

/// The line, then the text of its started and ended stamps.
type Stamps = (
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
);

const STAMPS: &[Stamps] = &[
    (
        "done, started and ended: the time it took beside the end",
        "x 2026-09-29 A started:2026-09-29T0730Z ended:2026-09-29T0815Z",
        Some("2026-09-29 07:30"),
        Some("2026-09-29 08:15 (45m)"),
    ),
    (
        "done without an end: no time it took",
        "x 2026-09-29 A started:2026-09-29T0730Z",
        Some("2026-09-29 07:30"),
        None,
    ),
    (
        "done, ended but never started",
        "x 2026-09-29 A ended:2026-09-29T0815Z",
        None,
        Some("2026-09-29 08:15"),
    ),
    (
        "underway: the time it has taken beside the start",
        "A started:2026-09-29T0830Z owner:agent1",
        Some("2026-09-29 08:30 (30m)"),
        None,
    ),
    (
        "stale: the mark stays beside the start",
        "A started:2026-09-29T0400Z owner:agent1",
        Some("2026-09-29 04:00 (5h, stale)"),
        None,
    ),
    (
        "underway with an end: the end stays bare",
        "A started:2026-09-29T0830Z ended:2026-09-29T0835Z",
        Some("2026-09-29 08:30 (30m)"),
        Some("2026-09-29 08:35"),
    ),
    (
        "not started, with an end",
        "A ended:2026-09-29T0835Z",
        None,
        Some("2026-09-29 08:35"),
    ),
];

#[test]
fn the_time_a_done_task_took_is_beside_its_end() {
    let clock = clock();
    for (name, line, started, ended) in STAMPS {
        let snap = snapshot(&[line], &[]);
        let task = snap.tasks().first().expect("item");
        assert_eq!(
            task.started_text(&clock, true).as_deref(),
            *started,
            "{name}"
        );
        assert_eq!(task.ended_text(&clock).as_deref(), *ended, "{name}");
    }
}
