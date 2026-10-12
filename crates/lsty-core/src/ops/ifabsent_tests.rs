use super::{
    Actor, AddParams, IdChoice, Outcome, Place, Target, add,
    test_util::{Sequence, clock_at, folder, said, strings},
};
use crate::errs::{Code, Error, Field, Source};

/// One case: the files, the clock, the actor, the calls, and what the
/// last call and the file must show.
struct Case {
    label: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    now: &'static str,
    agent: &'static str,
    calls: &'static [&'static [&'static str]],
    id: Id,
    want: &'static str,
    no_op: bool,
    messages: &'static [&'static str],
    items: &'static [u32],
    changed: &'static [u32],
}

#[derive(Clone, Copy)]
enum Id {
    Implied,
    Random,
    Chosen(&'static str),
}

const NOW: &str = "2026-09-29T0900Z";

const BASE: Case = Case {
    label: "",
    todo: "",
    done: None,
    now: NOW,
    agent: "",
    calls: &[],
    id: Id::Implied,
    want: "",
    no_op: false,
    messages: &[],
    items: &[],
    changed: &[],
};

fn params(texts: &[&str], id: Id) -> AddParams {
    AddParams {
        texts: strings(texts),
        if_absent: true,
        id: match id {
            Id::Implied => IdChoice::Implied,
            Id::Random => IdChoice::Random,
            Id::Chosen(c) => IdChoice::Chosen(c.to_owned()),
        },
        ..AddParams::default()
    }
}

fn run_calls(
    c: &Case,
    params_of: impl Fn(&[&str]) -> AddParams,
) -> (String, Result<Outcome, Error>) {
    let mut dir = folder(c.todo, c.done);
    dir.clock = clock_at(c.now);
    let ids = Sequence::letters();
    let mut env = dir.env();
    env.ids = &ids;
    env.actor = Actor::named(c.agent).expect("actor");
    let mut last = None;
    for texts in c.calls {
        let result = add(&env, &params_of(texts));
        let failed = result.is_err();
        last = Some(result);
        if failed {
            break;
        }
    }
    let result = last.unwrap_or_else(|| Err(Error::new(Code::Usage, "none")));
    (dir.todo(), result)
}

fn nrs(out: &Outcome, ids: &[crate::list::ItemId]) -> Vec<u32> {
    ids.iter()
        .filter_map(|id| out.after().item(*id))
        .map(|item| item.nr().get())
        .collect()
}

fn check(c: &Case) {
    let (file, result) = run_calls(c, |t| params(t, c.id));
    let out = result.unwrap_or_else(|e| panic!("{}: {e:?}", c.label));
    assert_eq!(file, c.want, "{}", c.label);
    assert_eq!(out.no_op(), c.no_op, "{}", c.label);
    assert_eq!(out.messages(), said(c.messages), "{}", c.label);
    assert_eq!(nrs(&out, out.items()), c.items, "{}", c.label);
    assert_eq!(nrs(&out, out.changed()), c.changed, "{}", c.label);
}

const CASES: &[Case] = &[
    Case {
        label: "15.1",
        todo: "2026-09-20 Buy milk\n",
        calls: &[&["Buy milk"]],
        want: "2026-09-20 Buy milk\n",
        no_op: true,
        messages: &["1 already there: Buy milk"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.2 priority not compared",
        todo: "(A) 2026-09-20 Buy milk\n",
        calls: &[&["(B) Buy milk"]],
        want: "(A) 2026-09-20 Buy milk\n",
        no_op: true,
        messages: &["1 already there: Buy milk"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.3 whitespace collapsed",
        todo: "Buy  milk\n",
        calls: &[&["Buy milk"]],
        want: "Buy  milk\n",
        no_op: true,
        messages: &["1 already there: Buy milk"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.4 case counts",
        todo: "Buy Milk\n",
        calls: &[&["Buy milk"]],
        want: "Buy Milk\n2026-09-29 Buy milk\n",
        messages: &["2 added: Buy milk"],
        items: &[2],
        ..BASE
    },
    Case {
        label: "15.5 done item",
        todo: "x 2026-09-28 Buy milk\n",
        calls: &[&["Buy milk"]],
        want: "x 2026-09-28 Buy milk\n2026-09-29 Buy milk\n",
        messages: &["2 added: Buy milk"],
        items: &[2],
        ..BASE
    },
    Case {
        label: "15.6 archived item",
        done: Some("x 2026-09-28 Buy milk\n"),
        calls: &[&["Buy milk"]],
        want: "2026-09-29 Buy milk\n",
        messages: &["1 added: Buy milk"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.7 other due date",
        todo: "Pay rent due:2026-10-01\n",
        calls: &[&["Pay rent due:2026-11-01"]],
        want: "Pay rent due:2026-10-01\n2026-09-29 Pay rent due:2026-11-01\n",
        messages: &["2 added: Pay rent"],
        items: &[2],
        ..BASE
    },
    Case {
        label: "15.8 fuzzy date, same day",
        todo: "Pay rent due:2026-10-02\n",
        calls: &[&["Pay rent due:fri"]],
        want: "Pay rent due:2026-10-02\n",
        no_op: true,
        messages: &["1 already there: Pay rent"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.9 fuzzy date, next day",
        todo: "Pay rent due:2026-10-02\n",
        now: "2026-10-03T0900Z",
        calls: &[&["Pay rent due:fri"]],
        want: "Pay rent due:2026-10-02\n2026-10-03 Pay rent due:2026-10-09\n",
        messages: &["2 added: Pay rent"],
        items: &[2],
        ..BASE
    },
    Case {
        label: "15.10 stamps and links ignored",
        todo: "Buy milk id:k3f9 dep:a started:2026-09-29T0800Z owner:agent1\n",
        calls: &[&["Buy milk"]],
        want: "Buy milk id:k3f9 dep:a started:2026-09-29T0800Z owner:agent1\n",
        no_op: true,
        messages: &["1 already there as id:k3f9: Buy milk"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "ended: is ignored as started: is",
        todo: "Buy milk x ended:2026-09-29T0800Z\n",
        calls: &[&["Buy milk x"]],
        want: "Buy milk x ended:2026-09-29T0800Z\n",
        no_op: true,
        messages: &["1 already there: Buy milk x"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.11 order and projects count",
        todo: "Buy milk +home\n",
        calls: &[&["Buy milk"], &["Buy +home milk"]],
        want: "Buy milk +home\n2026-09-29 Buy milk\n\
               2026-09-29 Buy +home milk\n",
        messages: &["3 added: Buy milk"],
        items: &[3],
        ..BASE
    },
    Case {
        label: "15.12 first match wins",
        todo: "A\nBuy\nBuy\n",
        calls: &[&["Buy"]],
        want: "A\nBuy\nBuy\n",
        no_op: true,
        messages: &["2 already there: Buy"],
        items: &[2],
        ..BASE
    },
    Case {
        label: "15.13 match by id, whatever the text",
        todo: "Retitled id:parser\n",
        calls: &[&["Write parser"]],
        id: Id::Chosen("parser"),
        want: "Retitled id:parser\n",
        no_op: true,
        messages: &["1 already there as id:parser: Retitled"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.16 key id given to a text match",
        todo: "Write parser\n",
        calls: &[&["Write parser"]],
        id: Id::Chosen("parser"),
        want: "Write parser id:parser\n",
        messages: &["1 already there, now id:parser: Write parser"],
        items: &[1],
        changed: &[1],
        ..BASE
    },
    Case {
        label: "15.17 a retry matches by id",
        todo: "Write parser\n",
        calls: &[&["Write parser"], &["Write parser"]],
        id: Id::Chosen("parser"),
        want: "Write parser id:parser\n",
        no_op: true,
        messages: &["1 already there as id:parser: Write parser"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.18 random id given",
        todo: "Write parser\n",
        calls: &[&["Write parser"]],
        id: Id::Random,
        want: "Write parser id:aaaa\n",
        messages: &["1 already there, now id:aaaa: Write parser"],
        items: &[1],
        changed: &[1],
        ..BASE
    },
    Case {
        label: "the same text twice gives one id",
        todo: "Write parser\n",
        calls: &[&["Write parser", "Write parser"]],
        id: Id::Random,
        want: "Write parser id:aaaa\n",
        messages: &[
            "1 already there, now id:aaaa: Write parser",
            "1 already there as id:aaaa: Write parser",
        ],
        items: &[1, 1],
        changed: &[1],
        ..BASE
    },
    Case {
        label: "15.19 an agent gets an id",
        todo: "Write parser\n",
        agent: "agent1",
        calls: &[&["Write parser"]],
        want: "Write parser id:aaaa\n",
        messages: &["1 already there, now id:aaaa: Write parser"],
        items: &[1],
        changed: &[1],
        ..BASE
    },
    Case {
        label: "15.21 a random id keeps the old one",
        todo: "Write parser id:x1\n",
        calls: &[&["Write parser"]],
        id: Id::Random,
        want: "Write parser id:x1\n",
        no_op: true,
        messages: &["1 already there as id:x1: Write parser"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.22 id in the text",
        todo: "Other id:parser\n",
        calls: &[&["Write parser id:parser"]],
        want: "Other id:parser\n",
        no_op: true,
        messages: &["1 already there as id:parser: Other"],
        items: &[1],
        ..BASE
    },
    Case {
        label: "15.23 some texts match",
        todo: "A\n",
        calls: &[&["A", "B"]],
        want: "A\n2026-09-29 B\n",
        messages: &["1 already there: A", "2 added: B"],
        items: &[1, 2],
        ..BASE
    },
    Case {
        label: "15.24 the same text twice",
        calls: &[&["A", "A"]],
        want: "2026-09-29 A\n",
        messages: &["1 added: A", "1 already there: A"],
        items: &[1, 1],
        ..BASE
    },
    Case {
        label: "15.25 matched by id and added",
        todo: "A2 id:a\n",
        calls: &[&["A id:a", "B id:b"]],
        want: "A2 id:a\n2026-09-29 B id:b\n",
        messages: &["1 already there as id:a: A2", "2 added as id:b: B"],
        items: &[1, 2],
        ..BASE
    },
    Case {
        label: "a random id avoids an id typed in a later text",
        todo: "Write parser\n",
        calls: &[&["Write parser", "Other id:aaaa"]],
        id: Id::Random,
        want: "Write parser id:bbbb\n2026-09-29 Other id:aaaa\n",
        messages: &[
            "1 already there, now id:bbbb: Write parser",
            "2 added as id:aaaa: Other",
        ],
        items: &[1, 2],
        changed: &[1],
        ..BASE
    },
    Case {
        label: "15.26 every text matches",
        todo: "A\nB\n",
        calls: &[&["A", "B"]],
        want: "A\nB\n",
        no_op: true,
        messages: &["1 already there: A", "2 already there: B"],
        items: &[1, 2],
        ..BASE
    },
];

#[test]
fn if_absent_cases() {
    for case in CASES {
        check(case);
    }
}

#[test]
fn a_match_changes_nothing_next_to_an_anchor() {
    // 15.28
    let case = Case {
        todo: "A\nB\n",
        calls: &[&["B"]],
        ..BASE
    };
    let (file, result) = run_calls(&case, |t| AddParams {
        place: Place::After(Target::todo(1)),
        ..params(t, Id::Implied)
    });
    let out = result.expect("add");
    assert_eq!(file, "A\nB\n");
    assert!(out.no_op());
    assert_eq!(out.messages(), said(&["2 already there: B"]));
    assert_eq!(nrs(&out, out.items()), [2]);
}

#[test]
fn a_match_reports_the_item_before_too() {
    // 15.21
    let case = Case {
        todo: "Write parser id:x1\n",
        calls: &[&["Write parser"]],
        ..BASE
    };
    let (_, result) = run_calls(&case, |t| params(t, Id::Implied));
    let out = result.expect("add");
    let before: Vec<u32> = out.items_before().map(|i| i.nr().get()).collect();
    assert_eq!(before, [1]);
}

/// The file, the call, the id, and the code, the `holder.source` text
/// and the item's id the error shows.
struct Refusal {
    label: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    texts: &'static [&'static str],
    id: Id,
    code: Code,
    holder: &'static str,
    /// The id of the item the error names.
    item: &'static str,
}

const REFUSALS: &[Refusal] = &[
    Refusal {
        label: "15.14 a done item holds the id",
        todo: "x 2026-09-28 W id:parser\n",
        done: None,
        texts: &["W"],
        id: Id::Chosen("parser"),
        code: Code::IdTaken,
        holder: "todo",
        item: "",
    },
    Refusal {
        label: "a done item's id wins over an open text match",
        todo: "x 2026-09-28 W id:parser\nW id:x1\n",
        done: None,
        texts: &["W"],
        id: Id::Chosen("parser"),
        code: Code::IdTaken,
        holder: "todo",
        item: "",
    },
    Refusal {
        label: "15.15 the done file holds the id",
        todo: "",
        done: Some("x 2026-09-28 W id:parser\n"),
        texts: &["W"],
        id: Id::Chosen("parser"),
        code: Code::IdTaken,
        holder: "done",
        item: "",
    },
    Refusal {
        label: "15.20 another id on the match",
        todo: "Write parser id:x1\n",
        done: None,
        texts: &["Write parser"],
        id: Id::Chosen("parser"),
        code: Code::WrongState,
        holder: "",
        item: "x1",
    },
    Refusal {
        label: "another id on a line staged earlier in the call",
        todo: "",
        done: None,
        texts: &["W id:a", "W id:b"],
        id: Id::Implied,
        code: Code::WrongState,
        holder: "",
        item: "a",
    },
    Refusal {
        label: "15.27 the text is checked first",
        todo: "Buy\n",
        done: None,
        texts: &["x Buy"],
        id: Id::Implied,
        code: Code::InvalidValue,
        holder: "",
        item: "",
    },
];

#[test]
fn if_absent_refusals() {
    for r in REFUSALS {
        let mut dir = folder(r.todo, r.done);
        dir.clock = clock_at(NOW);
        let env = dir.env();
        let err = add(&env, &params(r.texts, r.id)).expect_err(r.label);
        assert_eq!(err.code(), r.code, "{}", r.label);
        assert_eq!(dir.todo(), r.todo, "{}", r.label);
        if !r.holder.is_empty() {
            let source = match err.field("holder") {
                Some(Field::Ref(h)) => Some(h.source),
                _ => None,
            };
            let want = if r.holder == "done" {
                Source::Done
            } else {
                Source::Todo
            };
            assert_eq!(source, Some(want), "{}", r.label);
        }
        let item = err.item().and_then(|i| i.id.as_deref());
        assert_eq!(item.unwrap_or_default(), r.item, "{}", r.label);
        assert_eq!(err.field("state").is_some(), r.code == Code::WrongState);
    }
}
