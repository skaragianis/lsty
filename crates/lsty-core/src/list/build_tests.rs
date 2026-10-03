use std::time::Duration;

use jiff::Timestamp;

use super::test_util::{clock, snapshot};
use super::{Claim, Item, ItemId, Snapshot, State};
use crate::dates::format_elapsed;
use crate::errs::{Nr, Source};

fn nr(n: u32) -> Nr {
    Nr::new(n).expect("nr")
}

fn nrs(snap: &Snapshot, ids: &[ItemId]) -> Vec<u32> {
    ids.iter()
        .map(|&id| snap.item(id).expect("item").nr.get())
        .collect()
}

struct ItemsRow {
    name: &'static str,
    todo: &'static [&'static str],
    nrs: &'static [u32],
    states: &'static [State],
}

const ITEMS: &[ItemsRow] = &[
    ItemsRow {
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
    ItemsRow {
        name: "08.2 bad started is still doing",
        todo: &["Add started:yes"],
        nrs: &[1],
        states: &[State::Doing],
    },
];

#[test]
fn numbers_and_states() {
    for row in ITEMS {
        let snap = snapshot(row.todo, &[]);
        let got: Vec<u32> =
            snap.items().iter().map(|i| i.nr().get()).collect();
        assert_eq!(got, row.nrs, "{}", row.name);
        let states: Vec<State> =
            snap.items().iter().map(Item::state).collect();
        assert_eq!(states, row.states, "{}", row.name);
    }
}

#[test]
fn order_and_at() {
    let snap = snapshot(&["A", "B"], &["x 2026-09-28 C", "x D"]);
    let got: Vec<(Source, u32)> = snap
        .items()
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
    assert_eq!(snap.at(Source::Done, nr(2)), snap.items().get(3));
    assert_eq!(snap.at(Source::Todo, nr(3)), None);
    let item = snap.items().get(2).expect("item");
    assert_eq!(snap.line(item).raw(), b"x 2026-09-28 C");
}

#[test]
fn at_skips_blank_lines() {
    let snap = snapshot(&["A", "", "B"], &[]);
    assert!(snap.at(Source::Todo, nr(2)).is_none());
    assert_eq!(snap.at(Source::Todo, nr(3)), snap.items().get(1));
}

struct BlockRow {
    name: &'static str,
    todo: &'static [&'static str],
    done: &'static [&'static str],
    blocked: &'static [bool],
    blockers: &'static [&'static [u32]],
    dangling: &'static [&'static [&'static str]],
}

const NONE: &[&[u32]] = &[];
const NO_DANGLING: &[&[&str]] = &[];

const BLOCKING: &[BlockRow] = &[
    BlockRow {
        name: "08.3 open prerequisite",
        todo: &["A id:a", "B dep:a"],
        done: &[],
        blocked: &[false, true],
        blockers: &[&[], &[1]],
        dangling: NO_DANGLING,
    },
    BlockRow {
        name: "08.4 done prerequisite",
        todo: &["x 2026-09-28 A id:a", "B dep:a"],
        done: &[],
        blocked: &[false, false],
        blockers: NONE,
        dangling: NO_DANGLING,
    },
    BlockRow {
        name: "08.5 prerequisite done in the done file",
        todo: &["B dep:a"],
        done: &["x 2026-09-28 A id:a"],
        blocked: &[false],
        blockers: NONE,
        dangling: &[&[]],
    },
    BlockRow {
        name: "08.6 dangling",
        todo: &["B dep:zz"],
        done: &[],
        blocked: &[false],
        blockers: NONE,
        dangling: &[&["zz"]],
    },
    BlockRow {
        name: "08.7 ids are case sensitive",
        todo: &["B dep:PARSER", "A id:parser"],
        done: &[],
        blocked: &[false, false],
        blockers: NONE,
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
        name: "08.9 done items are never blocked",
        todo: &["x 2026-09-28 B dep:a", "A id:a"],
        done: &[],
        blocked: &[false, false],
        blockers: NONE,
        dangling: NO_DANGLING,
    },
    BlockRow {
        name: "08.17 first line with the id decides",
        todo: &["x 2026-09-28 A id:k", "B id:k", "C dep:k"],
        done: &[],
        blocked: &[false, false, false],
        blockers: NONE,
        dangling: NO_DANGLING,
    },
    BlockRow {
        name: "blockers keep dep order and are listed once",
        todo: &["A id:a", "B id:b", "C dep:b dep:a dep:b"],
        done: &[],
        blocked: &[false, false, true],
        blockers: &[&[], &[], &[2, 1]],
        dangling: NO_DANGLING,
    },
];

#[test]
fn blocking() {
    for row in BLOCKING {
        let snap = snapshot(row.todo, row.done);
        assert!(snap.items().len() >= row.blocked.len(), "{}", row.name);
        for (i, item) in
            snap.items().iter().take(row.blocked.len()).enumerate()
        {
            let at = format!("{} item {}", row.name, i + 1);
            assert_eq!(Some(&item.blocked()), row.blocked.get(i), "{at}");
            let want = row.blockers.get(i).copied().unwrap_or_default();
            assert_eq!(nrs(&snap, item.blockers()), want, "{at}");
            let want = row.dangling.get(i).copied().unwrap_or_default();
            let want: Vec<&[u8]> = want.iter().map(|d| d.as_bytes()).collect();
            assert_eq!(snap.dangling(item).collect::<Vec<_>>(), want, "{at}");
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
        name: "08.10 two items",
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
        name: "08.13 done item breaks the cycle",
        todo: &["A id:a dep:b", "x 2026-09-28 B id:b dep:a"],
        blocked: &[false, false],
        cycle: &[false, false],
        first_path: None,
    },
    CycleRow {
        name: "08.14 three items",
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
            snap.items().iter().map(Item::blocked).collect();
        let cycle: Vec<bool> = snap.items().iter().map(Item::cycle).collect();
        assert_eq!(blocked, row.blocked, "{}", row.name);
        assert_eq!(cycle, row.cycle, "{}", row.name);
        let path = snap.cycle_path(snap.id_for(0));
        let got = path.map(|p| nrs(&snap, &p));
        assert_eq!(got.as_deref(), row.first_path, "{}", row.name);
        for item in snap.items() {
            let has = snap.cycle_path(item.item_id()).is_some();
            assert_eq!(has, item.cycle, "{} {}", row.name, item.nr);
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
    assert!(snap.items().iter().all(|i| i.cycle() && i.blocked()));
    let path = snap.cycle_path(snap.id_for(0)).expect("ring");
    assert_eq!(path.len(), n);
    lines.pop();
    lines.push(String::from("end id:i4999"));
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let snap = snapshot(&refs, &[]);
    assert!(snap.items().iter().all(|i| !i.cycle()));
}

struct FlagRow {
    name: &'static str,
    todo: &'static [&'static str],
    later: &'static [bool],
    overdue: &'static [bool],
    stale: &'static [bool],
    claim: &'static [Claim],
}

const NO_FLAGS: &[bool] = &[];
const NO_CLAIMS: &[Claim] = &[];

const FLAGS: &[FlagRow] = &[
    FlagRow {
        name: "08.19 threshold",
        todo: &["Pay t:2026-09-30", "Pay t:2026-09-29", "Pay t:soon"],
        later: &[true, false, false],
        overdue: NO_FLAGS,
        stale: NO_FLAGS,
        claim: NO_CLAIMS,
    },
    FlagRow {
        name: "08.20 done and later",
        todo: &["x 2026-09-28 Pay t:2026-10-30"],
        later: &[true],
        overdue: NO_FLAGS,
        stale: NO_FLAGS,
        claim: NO_CLAIMS,
    },
    FlagRow {
        name: "08.21 overdue",
        todo: &[
            "Pay due:2026-09-28",
            "Pay due:2026-09-29",
            "x 2026-09-29 Pay due:2026-09-01",
        ],
        later: NO_FLAGS,
        overdue: &[true, false, false],
        stale: NO_FLAGS,
        claim: NO_CLAIMS,
    },
    FlagRow {
        name: "08.22 stale",
        todo: &["A started:2026-09-29T0459Z owner:agent1"],
        later: NO_FLAGS,
        overdue: NO_FLAGS,
        stale: &[true],
        claim: NO_CLAIMS,
    },
    FlagRow {
        name: "08.23 exactly the limit",
        todo: &["A started:2026-09-29T0500Z owner:agent1"],
        later: NO_FLAGS,
        overdue: NO_FLAGS,
        stale: &[false],
        claim: NO_CLAIMS,
    },
    FlagRow {
        name: "08.24 human claim",
        todo: &["A started:2026-09-28T0500Z"],
        later: NO_FLAGS,
        overdue: NO_FLAGS,
        stale: &[false],
        claim: &[Claim::Human],
    },
    FlagRow {
        name: "08.25 done keeps its claim",
        todo: &["x 2026-09-29 A started:2026-09-28T0500Z owner:agent1"],
        later: NO_FLAGS,
        overdue: NO_FLAGS,
        stale: &[false],
        claim: &[Claim::Agent],
    },
    FlagRow {
        name: "bad started is never stale",
        todo: &["A started:yes owner:agent1", "B"],
        later: NO_FLAGS,
        overdue: NO_FLAGS,
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
        assert_eq!(get(i).as_ref(), Some(want), "{name} item {}", i + 1);
    }
}

#[test]
fn later_overdue_stale_claim() {
    for row in FLAGS {
        let snap = snapshot(row.todo, &[]);
        let item = |i: usize| snap.items().get(i);
        check(row.name, row.later, |i| item(i).map(Item::later));
        check(row.name, row.overdue, |i| item(i).map(Item::overdue));
        check(row.name, row.stale, |i| item(i).map(Item::stale));
        check(row.name, row.claim, |i| item(i).map(Item::claim));
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
        name: "done",
        line: "x 2026-09-29 A started:2026-09-29T0830Z",
        want: None,
    },
];

#[test]
fn elapsed() {
    let now: Timestamp = clock().now();
    for row in ELAPSED {
        let snap = snapshot(&[row.line], &[]);
        let got = snap.items().first().and_then(|i| i.elapsed(now));
        let want = row.want.map(Duration::from_secs);
        assert_eq!(got, want, "{}", row.name);
    }
}

#[test]
fn elapsed_formats_as_it_is() {
    let now: Timestamp = clock().now();
    let snap = snapshot(&["A started:2026-10-01T0000Z"], &[]);
    let item = snap.items().first().expect("item");
    let text = item.elapsed(now).map(format_elapsed);
    assert!(text.is_some_and(|t| t.ends_with(['m', 'h', 'd'])));
}

#[test]
fn tags_and_line_of_an_item() {
    let snap = snapshot(&["A id:a dep:b due:2026-10-01"], &[]);
    let item = snap.items().first().expect("item");
    let tags = snap.tags(item);
    assert_eq!(tags.due, Some("2026-10-01"));
    assert_eq!(snap.id_of(item), Some(b"a".as_slice()));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "item of another snapshot")]
fn an_item_of_another_snapshot_is_caught() {
    let one = snapshot(&["A"], &[]);
    let other = snapshot(&["B"], &[]);
    let item = other.items().first().expect("item");
    assert_eq!(one.line(item).raw(), b"A");
}

#[test]
fn files_and_refs_by_source() {
    let snap = snapshot(&["A id:a", "B"], &["x C id:c"]);
    let files = snap.found();
    assert_eq!(files.path(Source::Todo), files.todo);
    assert_eq!(files.path(Source::Done), files.done);
    assert_eq!(snap.file(Source::Todo).lines.len(), 2);
    assert_eq!(snap.file(Source::Done).lines.len(), 1);
    let items = snap.items();
    let refs: Vec<_> = items.iter().map(|i| snap.ref_of(i)).collect();
    let want = [
        (1, Source::Todo, Some("a")),
        (2, Source::Todo, None),
        (1, Source::Done, Some("c")),
    ];
    for (got, (n, source, id)) in refs.iter().zip(want) {
        assert_eq!(got.nr, nr(n));
        assert_eq!(got.source, source);
        assert_eq!(got.id.as_deref(), id);
    }
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "item of another snapshot")]
fn an_item_without_an_id_of_another_snapshot_is_caught() {
    let one = snapshot(&["A"], &[]);
    let other = snapshot(&["B"], &[]);
    let item = other.items().first().expect("item");
    assert_eq!(one.ref_of(item).id, None);
}
