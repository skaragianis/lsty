use super::test_util::{Sequence, folder, said};
use super::{Actor, NextParams, next_read, next_start};
use crate::errs::Source;
use crate::list::SortField;
use crate::query::Query;

/// The list of the acceptance rows; now is 2026-09-29 09:00 UTC.
const L: &str = "Buy milk +home
(B) Write docs +spec02
(A) Add CLI ls +spec02 dep:parser
(A) Write parser +spec01 id:parser file:specs/01-parser.md
(A) Plan offsite +team t:2026-10-05
(C) Review PR +spec02 started:2026-09-29T0100Z owner:agent9
x 2026-09-28 Old +spec02
";

struct Case {
    name: &'static str,
    todo: &'static str,
    owner: &'static str,
    filter: &'static str,
    sort: SortField,
    /// The number of the chosen item.
    nr: Option<u32>,
    messages: &'static [&'static str],
    stale: &'static [u32],
    hidden: usize,
}

const BASE: Case = Case {
    name: "",
    todo: L,
    owner: "",
    filter: "",
    sort: SortField::Priority,
    nr: None,
    messages: &[],
    stale: &[6],
    hidden: 1,
};

const READS: &[Case] = &[
    Case {
        name: "preview takes the first priority A",
        nr: Some(4),
        messages: &["1 stale claims skipped"],
        ..BASE
    },
    Case {
        name: "a filter skips the blocked and the claimed",
        filter: "+spec02",
        nr: Some(2),
        messages: &["1 stale claims skipped"],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "file order",
        sort: SortField::File,
        nr: Some(1),
        messages: &["1 stale claims skipped"],
        ..BASE
    },
    Case {
        name: "the stale claim must match the filter",
        filter: "+spec01",
        nr: Some(4),
        messages: &[],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "nothing ready",
        filter: "+nothing",
        messages: &["nothing ready"],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "a stale claim is its owner's own",
        owner: "agent9",
        nr: Some(6),
        messages: &["6 resumed: Review PR"],
        stale: &[],
        ..BASE
    },
    Case {
        name: "a human claim is someone else's",
        todo: "(B) Write docs +spec02\n\
               (A) Write parser id:p started:2026-09-29T0800Z\n",
        nr: Some(1),
        messages: &[],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "the oldest own claim, by a bad stamp first",
        todo: "(B) A started:2026-09-29T0800Z owner:agent1\n\
               (A) B started:garbage owner:agent1\n\
               C started:2026-09-29T0700Z owner:agent1\n",
        owner: "agent1",
        nr: Some(2),
        messages: &["2 resumed: B"],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "claims started together keep file order",
        todo: "A started:2026-09-29T0800Z owner:agent1\n\
               B started:2026-09-29T0800Z owner:agent1\n",
        owner: "agent1",
        nr: Some(1),
        messages: &["1 resumed: A"],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "the oldest of two valid claims",
        todo: "A started:2026-09-29T0800Z owner:agent1\n\
               B started:2026-09-29T0700Z owner:agent1\n",
        owner: "agent1",
        nr: Some(2),
        messages: &["2 resumed: B"],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "an own claim that no longer matches",
        todo: "A +x id:a1 started:2026-09-29T0800Z owner:agent1\nB +y\n",
        owner: "agent1",
        filter: "+y",
        nr: Some(1),
        messages: &["a1 resumed: A", "a1 no longer matches the filter"],
        stale: &[],
        hidden: 0,
        ..BASE
    },
    Case {
        name: "an own claim that is blocked and later",
        todo: "A id:a1 dep:b t:2026-10-05 started:2026-09-29T0800Z owner:agent1\n\
               B id:b\n",
        owner: "agent1",
        nr: Some(1),
        messages: &[
            "a1 resumed: A",
            "a1 is blocked by 2 (B); work on the blocker or stop the claim",
            "a1 is hidden until 2026-10-05",
        ],
        stale: &[],
        hidden: 1,
        ..BASE
    },
    Case {
        name: "every open item is held back",
        todo: "(A) Blocked dep:b\nLater t:2026-10-05\n\
               Claimed started:2026-09-29T0800Z owner:agent9\n\
               Human started:2026-09-29T0800Z\n\
               Blocker id:b dep:c\nOther id:c dep:b\n",
        messages: &["nothing ready"],
        stale: &[],
        hidden: 1,
        ..BASE
    },
    Case {
        name: "sort by due",
        todo: "(A) Late +x due:2026-10-09\nMid +x due:2026-10-02\n\
               Early +x due:2026-10-01\nNo due +x\n",
        sort: SortField::Due,
        nr: Some(3),
        messages: &[],
        stale: &[],
        hidden: 0,
        ..BASE
    },
];

fn query(text: &str) -> Query {
    Query::parse(text).expect("query")
}

#[test]
fn next_reads_the_first_ready_item() {
    for case in READS {
        let dir = folder(case.todo, None);
        let mut env = dir.env();
        env.actor = Actor::named(case.owner).expect("actor");
        let params = NextParams {
            filter: query(case.filter),
            sort: case.sort,
        };
        let got = next_read(&env, &params).expect(case.name);
        let nrs: Vec<u32> = got
            .item
            .and_then(|id| got.snapshot.item(id))
            .map(|i| i.nr().get())
            .into_iter()
            .collect();
        assert_eq!(
            nrs,
            case.nr.into_iter().collect::<Vec<_>>(),
            "{}",
            case.name
        );
        assert_eq!(got.messages, said(case.messages), "{}", case.name);
        let stale: Vec<u32> = got.stale.iter().map(|r| r.nr.get()).collect();
        assert_eq!(stale, case.stale, "{}", case.name);
        assert_eq!(got.hidden, case.hidden, "{}", case.name);
        assert_eq!(dir.todo(), case.todo, "{}", case.name);
    }
}

#[test]
fn next_ignores_the_done_file() {
    let dir =
        folder("x 2026-09-28 Finished\n", Some("(A) Looks open\nOther\n"));
    let env = dir.env();
    let got = next_read(&env, &NextParams::default()).expect("next");
    assert_eq!(got.item, None);
    assert_eq!(got.messages, said(&["nothing ready"]));
}

#[test]
fn start_claims_and_gives_an_id() {
    let dir = folder(L, None);
    let ids = Sequence::list(["k3f9"]);
    let mut env = dir.env();
    env.ids = &ids;
    env.actor = Actor::named("agent1").expect("actor");
    let params = NextParams {
        filter: query("+spec02"),
        sort: SortField::Priority,
    };
    let outcome = next_start(&env, &params).expect("start").outcome;
    let todo = dir.todo();
    assert!(todo.contains(
        "(B) Write docs +spec02 id:k3f9 started:2026-09-29T0900Z \
         owner:agent1\n"
    ));
    assert_eq!(
        outcome.messages(),
        said(&[
            "2 is id:k3f9",
            "k3f9 started by agent1: Write docs",
            "1 stale claims skipped",
        ])
    );
    assert_eq!(outcome.action(), "next");
    assert!(!outcome.no_op());
}

#[test]
fn start_again_returns_the_same_claim() {
    let dir = folder(L, None);
    let ids = Sequence::list(["k3f9"]);
    let mut env = dir.env();
    env.ids = &ids;
    env.actor = Actor::named("agent1").expect("actor");
    let params = NextParams {
        filter: query("+spec02"),
        sort: SortField::Priority,
    };
    next_start(&env, &params).expect("first");
    let after = dir.todo();
    let again = next_start(&env, &params).expect("second").outcome;
    assert_eq!(dir.todo(), after);
    assert!(again.no_op());
    assert_eq!(
        again.messages(),
        said(&["k3f9 resumed: Write docs", "1 stale claims skipped"])
    );
}

#[test]
fn a_claim_without_an_id_gets_one_and_nothing_else_changes() {
    let todo = "(B) Write docs started:2026-09-29T0900Z owner:agent1\n";
    let dir = folder(todo, None);
    let ids = Sequence::list(["k3f9"]);
    let mut env = dir.env();
    env.ids = &ids;
    env.actor = Actor::named("agent1").expect("actor");
    let got = next_start(&env, &NextParams::default()).expect("next");
    assert_eq!(
        dir.todo(),
        "(B) Write docs started:2026-09-29T0900Z owner:agent1 id:k3f9\n"
    );
    assert_eq!(
        got.outcome.messages(),
        said(&["1 is id:k3f9", "k3f9 resumed: Write docs"])
    );
}

#[test]
fn start_with_nothing_ready_writes_nothing() {
    let dir = folder("x 2026-09-28 Done\n", None);
    let mut env = dir.env();
    env.actor = Actor::named("agent1").expect("actor");
    let got = next_start(&env, &NextParams::default()).expect("next");
    assert_eq!(got.outcome.items().len(), 0);
    assert!(got.outcome.no_op());
    assert_eq!(got.outcome.messages(), said(&["nothing ready"]));
    assert_eq!(dir.todo(), "x 2026-09-28 Done\n");
}

#[test]
fn start_skips_a_human_claim() {
    let todo =
        "(B) Write docs\n(A) Write parser id:p started:2026-09-29T0800Z\n";
    let dir = folder(todo, None);
    let ids = Sequence::list(["k3f9"]);
    let mut env = dir.env();
    env.ids = &ids;
    env.actor = Actor::named("agent1").expect("actor");
    let got = next_start(&env, &NextParams::default()).expect("next");
    let nrs: Vec<_> = got
        .outcome
        .items_after()
        .map(|i| (i.source(), i.nr().get()))
        .collect();
    assert_eq!(nrs, [(Source::Todo, 1)]);
}
