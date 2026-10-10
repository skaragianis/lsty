use super::test_util::{folder, parts, said};
use super::{
    Actor, ClaimOpts, DoOpts, Env, Outcome, PriPair, StartOpts, append, del,
    del_term, depri, due, finish, id, parse_refs, pri, reopen, replace, start,
    stop, tag,
};
use crate::errs::{Code, Error, Field};
use crate::todotxt::Priority;

const TAKEOVER: ClaimOpts = ClaimOpts { takeover: true };
const PLAIN: ClaimOpts = ClaimOpts { takeover: false };

/// One call of an operation.
#[derive(Clone, Copy)]
enum Op {
    Start(ClaimOpts),
    Do(ClaimOpts),
    Stop(ClaimOpts),
    Reopen(ClaimOpts),
    Replace(&'static str, ClaimOpts),
    Del(ClaimOpts),
    DelTerm(&'static str),
    Pri(Option<Priority>),
    Depri,
    Due(&'static str),
    Tag(&'static str),
    Append(&'static str),
    Id,
}

/// What a step gives.
#[derive(Clone, Copy)]
enum Want {
    /// The file and the messages after the call.
    Done(&'static str, &'static [&'static str]),
    /// Nothing changed, with these messages.
    NoOp(&'static str, &'static [&'static str]),
    /// `claimed`: the holder (`None` for the human) and the time.
    Claimed(Option<&'static str>, &'static str),
    /// `wrong_state` with this hint.
    WrongState(&'static str),
}

/// The agent (empty for the human), the references and the call.
struct Step {
    actor: &'static str,
    refs: &'static str,
    op: Op,
    want: Want,
}

struct Case {
    name: &'static str,
    todo: &'static str,
    steps: &'static [Step],
}

const REF: &str = "id:a";

fn call(env: &Env<'_>, refs: &str, op: Op) -> Result<Outcome, Error> {
    let targets = parse_refs(refs)?;
    let one = |priority| -> Vec<PriPair> {
        targets
            .iter()
            .map(|t| PriPair {
                target: t.clone(),
                priority,
            })
            .collect()
    };
    match op {
        Op::Start(claim) => start(
            env,
            &targets,
            StartOpts {
                claim,
                ..StartOpts::default()
            },
        ),
        Op::Do(claim) => finish(
            env,
            &targets,
            DoOpts {
                claim,
                ..DoOpts::default()
            },
        ),
        Op::Stop(claim) => stop(env, &targets, claim),
        Op::Reopen(claim) => reopen(env, &targets, claim),
        Op::Replace(text, claim) => replace(env, &targets, text, claim),
        Op::Del(claim) => del(env, &targets, claim),
        Op::DelTerm(term) => del_term(env, &targets, term),
        Op::Pri(priority) => pri(env, &one(priority)),
        Op::Depri => depri(env, &targets),
        Op::Due(input) => due(env, &targets, input),
        Op::Tag(t) => tag(env, &targets, &[t.to_owned()]),
        Op::Append(t) => append(env, &targets, t),
        Op::Id => id(env, &targets, None),
    }
}

fn run_case(case: &Case) {
    let dir = folder(case.todo, None);
    for (n, step) in case.steps.iter().enumerate() {
        let mut env = dir.env();
        env.actor = Actor::named(step.actor).expect("actor");
        let before = dir.todo();
        let got = call(&env, step.refs, step.op);
        let at = format!("{} step {n}", case.name);
        check(&at, &got, &step.want, &before, &dir.todo());
    }
}

fn check(
    at: &str,
    got: &Result<Outcome, Error>,
    want: &Want,
    before: &str,
    now: &str,
) {
    match got {
        Ok(out) => check_done(at, out, want, before, now),
        Err(err) => check_refused(at, err, want, before, now),
    }
}

fn check_done(at: &str, out: &Outcome, want: &Want, before: &str, now: &str) {
    match want {
        Want::Done(file, msgs) => {
            assert_eq!(now, *file, "{at}");
            assert_eq!(out.messages(), said(msgs), "{at}");
            assert!(!out.no_op(), "{at}");
        }
        Want::NoOp(file, msgs) => {
            assert_eq!(now, *file, "{at}");
            assert_eq!(out.messages(), said(msgs), "{at}");
            assert!(out.no_op(), "{at}");
        }
        Want::Claimed(..) | Want::WrongState(_) => {
            panic!("{at}: succeeded ({before:?}): {:?}", out.messages())
        }
    }
}

fn check_refused(at: &str, err: &Error, want: &Want, before: &str, now: &str) {
    match want {
        Want::Claimed(owner, started) => {
            assert_eq!(err.code(), Code::Claimed, "{at}: {err:?}");
            assert_eq!(err.text("owner"), *owner, "{at}");
            assert_eq!(err.text("started"), Some(*started), "{at}");
        }
        Want::WrongState(hint) => {
            let (code, _, got_hint) = parts(err);
            assert_eq!((code, got_hint), (Code::WrongState, *hint), "{at}");
        }
        Want::Done(..) | Want::NoOp(..) => panic!("{at}: failed: {err:?}"),
    }
    assert_eq!(now, before, "{at}");
}

const fn step(
    actor: &'static str,
    refs: &'static str,
    op: Op,
    want: Want,
) -> Step {
    Step {
        actor,
        refs,
        op,
        want,
    }
}

const AGENT_ROWS: &[Case] = &[
    Case {
        name: "18.1 agent starts",
        todo: "A id:a\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Start(PLAIN),
            Want::Done(
                "A id:a started:2026-09-29T0900Z owner:agent1\n",
                &["a started by agent1"],
            ),
        )],
    },
    Case {
        name: "18.2 agent starts its own",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Start(PLAIN),
            Want::NoOp(
                "A id:a started:2026-09-29T0800Z owner:agent1\n",
                &["a is already underway"],
            ),
        )],
    },
    Case {
        name: "18.3 agent starts another's",
        todo: "A id:a started:2026-09-29T0800Z owner:agent2\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Start(PLAIN),
            Want::Claimed(Some("agent2"), "2026-09-29T08:00:00Z"),
        )],
    },
    Case {
        name: "18.4 agent takes over a start",
        todo: "A id:a started:2026-09-29T0800Z owner:agent2\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Start(TAKEOVER),
            Want::Done(
                "A id:a started:2026-09-29T0900Z owner:agent1\n",
                &["a taken over from agent2"],
            ),
        )],
    },
    Case {
        name: "18.5 agent starts the human's",
        todo: "A id:a started:2026-09-29T0800Z\n",
        steps: &[
            step(
                "agent1",
                REF,
                Op::Start(PLAIN),
                Want::Claimed(None, "2026-09-29T08:00:00Z"),
            ),
            step(
                "agent1",
                REF,
                Op::Start(TAKEOVER),
                Want::Done(
                    "A id:a started:2026-09-29T0900Z owner:agent1\n",
                    &["a taken over from the human"],
                ),
            ),
        ],
    },
    Case {
        name: "18.6 agent does an unstarted",
        todo: "A id:a\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Do(PLAIN),
            Want::WrongState("start it first: lsty start id:a"),
        )],
    },
    Case {
        name: "18.7 agent does an unstarted with takeover",
        todo: "A id:a\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Do(TAKEOVER),
            Want::WrongState("start it first: lsty start id:a"),
        )],
    },
    Case {
        name: "18.8 agent does its own",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Do(PLAIN),
            Want::Done(
                "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1\n",
                &["a done: A"],
            ),
        )],
    },
    Case {
        name: "18.9 agent does another's",
        todo: "A id:a started:2026-09-29T0800Z owner:agent2\n",
        steps: &[
            step(
                "agent1",
                REF,
                Op::Do(PLAIN),
                Want::Claimed(Some("agent2"), "2026-09-29T08:00:00Z"),
            ),
            step(
                "agent1",
                REF,
                Op::Do(TAKEOVER),
                Want::Done(
                    "x 2026-09-29 A id:a started:2026-09-29T0800Z \
                     owner:agent1\n",
                    &["a done: A", "a taken over from agent2"],
                ),
            ),
        ],
    },
    Case {
        name: "18.10 agent does the human's with takeover",
        todo: "A id:a started:2026-09-29T0800Z\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Do(TAKEOVER),
            Want::Done(
                "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1\n",
                &["a done: A", "a taken over from the human"],
            ),
        )],
    },
    Case {
        name: "18.11 agent does a done",
        todo: "x 2026-09-28 A id:a started:2026-09-29T0800Z owner:agent2\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Do(PLAIN),
            Want::NoOp(
                "x 2026-09-28 A id:a started:2026-09-29T0800Z owner:agent2\n",
                &["a is already done"],
            ),
        )],
    },
];

const HELD2: &str = "A id:a started:2026-09-29T0800Z owner:agent2\n";
const CLAIM2: &str = "a is claimed by agent2";
const HELD2_AT: &str = "2026-09-29T08:00:00Z";

const AGENT_GUARDS: &[Case] = &[
    Case {
        name: "18.12 agent stop of another's",
        todo: HELD2,
        steps: &[step(
            "agent1",
            REF,
            Op::Stop(PLAIN),
            Want::Claimed(Some("agent2"), HELD2_AT),
        )],
    },
    Case {
        name: "18.12 agent replace of another's",
        todo: HELD2,
        steps: &[step(
            "agent1",
            REF,
            Op::Replace("B", PLAIN),
            Want::Claimed(Some("agent2"), HELD2_AT),
        )],
    },
    Case {
        name: "18.12 agent del of another's",
        todo: HELD2,
        steps: &[step(
            "agent1",
            REF,
            Op::Del(PLAIN),
            Want::Claimed(Some("agent2"), HELD2_AT),
        )],
    },
    Case {
        name: "18.13 agent del with takeover",
        todo: HELD2,
        steps: &[step(
            "agent1",
            REF,
            Op::Del(TAKEOVER),
            Want::Done("\n", &["a deleted", "a taken over from agent2"]),
        )],
    },
    Case {
        name: "18.13 agent stop with takeover",
        todo: HELD2,
        steps: &[step(
            "agent1",
            REF,
            Op::Stop(TAKEOVER),
            Want::Done("A id:a\n", &["a stopped", "a taken over from agent2"]),
        )],
    },
    Case {
        name: "18.13 agent replace with takeover",
        todo: HELD2,
        steps: &[step(
            "agent1",
            REF,
            Op::Replace("B", TAKEOVER),
            Want::Done(
                "B started:2026-09-29T0800Z owner:agent2 id:a\n",
                &[
                    "a B started:2026-09-29T0800Z owner:agent2 id:a",
                    "a taken over from agent2",
                ],
            ),
        )],
    },
    Case {
        name: "18.14 agent stops an unstarted",
        todo: "A id:a\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Stop(PLAIN),
            Want::NoOp("A id:a\n", &["a is not underway"]),
        )],
    },
    Case {
        name: "18.15 agent edits another's metadata",
        todo: HELD2,
        steps: &[
            step(
                "agent1",
                REF,
                Op::Pri(Some(Priority::A)),
                Want::Done(
                    "(A) A id:a started:2026-09-29T0800Z owner:agent2\n",
                    &["a prioritized (A)", CLAIM2],
                ),
            ),
            step(
                "agent1",
                REF,
                Op::Due("fri"),
                Want::Done(
                    "(A) A id:a started:2026-09-29T0800Z owner:agent2 \
                     due:2026-10-02\n",
                    &["a due 2026-10-02", CLAIM2],
                ),
            ),
            step(
                "agent1",
                REF,
                Op::Tag("+x"),
                Want::Done(
                    "(A) A id:a started:2026-09-29T0800Z owner:agent2 \
                     due:2026-10-02 +x\n",
                    &["a tagged +x", CLAIM2],
                ),
            ),
            step(
                "agent1",
                REF,
                Op::Append("y"),
                Want::Done(
                    "(A) A id:a started:2026-09-29T0800Z owner:agent2 \
                     due:2026-10-02 +x y\n",
                    &[
                        "a (A) A id:a started:2026-09-29T0800Z \
                         owner:agent2 due:2026-10-02 +x y",
                        CLAIM2,
                    ],
                ),
            ),
        ],
    },
    Case {
        name: "18.15 agent depri, del term and id on another's",
        todo: "(A) A id:a started:2026-09-29T0800Z owner:agent2\n",
        steps: &[
            step(
                "agent1",
                REF,
                Op::Id,
                Want::NoOp(
                    "(A) A id:a started:2026-09-29T0800Z owner:agent2\n",
                    &["a is id:a (unchanged)", CLAIM2],
                ),
            ),
            step(
                "agent1",
                REF,
                Op::Depri,
                Want::Done(
                    "A id:a started:2026-09-29T0800Z owner:agent2\n",
                    &["a deprioritized", CLAIM2],
                ),
            ),
            step(
                "agent1",
                REF,
                Op::DelTerm("A "),
                Want::Done(
                    "id:a started:2026-09-29T0800Z owner:agent2\n",
                    &["a id:a started:2026-09-29T0800Z owner:agent2", CLAIM2],
                ),
            ),
        ],
    },
    Case {
        name: "18.16 agent edits the human's",
        todo: "A id:a started:2026-09-29T0800Z\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Tag("+x"),
            Want::Done(
                "A id:a started:2026-09-29T0800Z +x\n",
                &["a tagged +x", "a is claimed by the human"],
            ),
        )],
    },
    Case {
        name: "18.17 agent edits its own",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Tag("+x"),
            Want::Done(
                "A id:a started:2026-09-29T0800Z owner:agent1 +x\n",
                &["a tagged +x"],
            ),
        )],
    },
    Case {
        name: "18.18 agent does several with a claimed one",
        todo: "A id:a started:2026-09-29T0800Z owner:agent2\nB id:b\n",
        steps: &[
            step(
                "agent1",
                "id:b",
                Op::Start(PLAIN),
                Want::Done(
                    "A id:a started:2026-09-29T0800Z owner:agent2\n\
                     B id:b started:2026-09-29T0900Z owner:agent1\n",
                    &["b started by agent1"],
                ),
            ),
            step(
                "agent1",
                "id:b,id:a",
                Op::Do(PLAIN),
                Want::Claimed(Some("agent2"), HELD2_AT),
            ),
        ],
    },
];

const HUMAN_ROWS: &[Case] = &[
    Case {
        name: "18.19 human starts",
        todo: "A\n",
        steps: &[step(
            "",
            "1",
            Op::Start(PLAIN),
            Want::Done("A started:2026-09-29T0900Z\n", &["1 started"]),
        )],
    },
    Case {
        name: "18.20 human takes over a start",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Start(PLAIN),
            Want::Done(
                "A started:2026-09-29T0900Z\n",
                &["1 was claimed by agent1; now yours"],
            ),
        )],
    },
    Case {
        name: "18.21 agent does after a human takeover",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        steps: &[
            step(
                "",
                REF,
                Op::Start(PLAIN),
                Want::Done(
                    "A id:a started:2026-09-29T0900Z\n",
                    &["a was claimed by agent1; now yours"],
                ),
            ),
            step(
                "agent1",
                REF,
                Op::Do(PLAIN),
                Want::Claimed(None, "2026-09-29T09:00:00Z"),
            ),
        ],
    },
    Case {
        name: "18.22 human does",
        todo: "A\n",
        steps: &[step(
            "",
            "1",
            Op::Do(PLAIN),
            Want::Done("x 2026-09-29 A\n", &["1 done: A"]),
        )],
    },
    Case {
        name: "18.23 human does an agent's",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Do(PLAIN),
            Want::Done(
                "x 2026-09-29 A started:2026-09-29T0800Z\n",
                &["1 done: A", "1 was claimed by agent1"],
            ),
        )],
    },
    Case {
        name: "18.24 human stops an agent's",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Stop(PLAIN),
            Want::Done("A\n", &["1 stopped", "1 was claimed by agent1"]),
        )],
    },
    Case {
        name: "18.24 human dels an agent's",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Del(PLAIN),
            Want::Done("\n", &["1 deleted", "1 was claimed by agent1"]),
        )],
    },
    Case {
        name: "18.24 human replaces an agent's",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Replace("B", PLAIN),
            Want::Done(
                "B started:2026-09-29T0800Z owner:agent1\n",
                &[
                    "1 B started:2026-09-29T0800Z owner:agent1",
                    "1 is claimed by agent1",
                ],
            ),
        )],
    },
    Case {
        name: "18.25 human edits an agent's",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Tag("+x"),
            Want::Done(
                "A started:2026-09-29T0800Z owner:agent1 +x\n",
                &["1 tagged +x", "1 is claimed by agent1"],
            ),
        )],
    },
];

const GONE1: &str =
    "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1\n";

const REOPEN_ROWS: &[Case] = &[
    Case {
        name: "18.26 agent reopens back to the finisher",
        todo: GONE1,
        steps: &[step(
            "agent2",
            REF,
            Op::Reopen(PLAIN),
            Want::Done(
                "A id:a started:2026-09-29T0800Z owner:agent1\n",
                &["a reopened; back to agent1"],
            ),
        )],
    },
    Case {
        name: "18.27 agent reopens with takeover",
        todo: GONE1,
        steps: &[step(
            "agent2",
            REF,
            Op::Reopen(TAKEOVER),
            Want::Done(
                "A id:a started:2026-09-29T0900Z owner:agent2\n",
                &["a reopened; now yours"],
            ),
        )],
    },
    Case {
        name: "18.28 human reopens with takeover",
        todo: "x 2026-09-29 A started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "",
            "1",
            Op::Reopen(TAKEOVER),
            Want::Done(
                "A started:2026-09-29T0900Z\n",
                &["1 reopened; now yours"],
            ),
        )],
    },
    Case {
        name: "18.29 agent reopens the human's",
        todo: "x 2026-09-29 A id:a started:2026-09-29T0800Z\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Reopen(PLAIN),
            Want::Done(
                "A id:a started:2026-09-29T0800Z\n",
                &["a reopened; back to the human"],
            ),
        )],
    },
    Case {
        name: "18.30 human reopens its own",
        todo: "x 2026-09-29 A started:2026-09-29T0800Z\n",
        steps: &[step(
            "",
            "1",
            Op::Reopen(PLAIN),
            Want::Done(
                "A started:2026-09-29T0800Z\n",
                &["1 reopened; yours again"],
            ),
        )],
    },
    Case {
        name: "18.31 agent reopens an unstarted",
        todo: "x 2026-09-29 A id:a\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Reopen(PLAIN),
            Want::Done("A id:a\n", &["a reopened"]),
        )],
    },
    Case {
        name: "18.31 agent reopens an unstarted with takeover",
        todo: "x 2026-09-29 A id:a\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Reopen(TAKEOVER),
            Want::Done(
                "A id:a started:2026-09-29T0900Z owner:agent1\n",
                &["a reopened; now yours"],
            ),
        )],
    },
    Case {
        name: "18.32 agent starts a stale claim",
        todo: "A id:a started:2026-09-28T0000Z owner:agent2\n",
        steps: &[step(
            "agent1",
            REF,
            Op::Start(PLAIN),
            Want::Claimed(Some("agent2"), "2026-09-28T00:00:00Z"),
        )],
    },
    Case {
        name: "18.34 names are exact",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        steps: &[step(
            "Agent1",
            REF,
            Op::Do(PLAIN),
            Want::Claimed(Some("agent1"), HELD2_AT),
        )],
    },
];

#[test]
fn agent_rows() {
    AGENT_ROWS.iter().for_each(run_case);
}

#[test]
fn agent_guard_rows() {
    AGENT_GUARDS.iter().for_each(run_case);
}

#[test]
fn human_rows() {
    HUMAN_ROWS.iter().for_each(run_case);
}

#[test]
fn reopen_and_exactness_rows() {
    REOPEN_ROWS.iter().for_each(run_case);
}

#[test]
fn invalid_agent_names_are_refused() {
    for name in ["my agent", "a:b", "a\tb"] {
        let err = Actor::named(name).expect_err(name);
        assert_eq!(err.code(), Code::InvalidValue, "{name:?}");
        assert_eq!(err.text("field"), Some("owner"), "{name:?}");
    }
}

#[test]
fn the_claimed_message_names_the_holder_and_the_time() {
    let rows = [
        (
            "A id:a started:2026-09-28T1305Z owner:agent2\n",
            "a is claimed by agent2 since 2026-09-28T13:05Z",
            Some("2026-09-28T13:05:00Z"),
        ),
        (
            "A id:a started:2026-09-28T1305Z\n",
            "a is claimed by the human since 2026-09-28T13:05Z",
            Some("2026-09-28T13:05:00Z"),
        ),
        (
            "A id:a started:soon owner:agent2\n",
            "a is claimed by agent2",
            None,
        ),
    ];
    for (todo, message, started) in rows {
        let dir = folder(todo, None);
        let mut env = dir.env();
        env.actor = Actor::named("agent1").expect("actor");
        let err = call(&env, REF, Op::Start(PLAIN)).expect_err(message);
        let (code, got, hint) = parts(&err);
        assert_eq!(code, Code::Claimed, "{message}");
        assert_eq!(got, message);
        assert_eq!(
            hint,
            "pick another item with lsty next; \
             pass --takeover only when told to"
        );
        let want = started.map_or(Field::Null, |s| Field::Str(s.to_owned()));
        assert_eq!(err.field("started"), Some(&want), "{message}");
        assert!(err.item().is_some(), "{message}");
    }
}

#[test]
fn a_human_finish_drops_an_owner_before_a_date_word_with_a_refusal() {
    let dir = folder("owner:bob 2026-09-01 foo\n", None);
    let mut config = dir.config.clone();
    config.include_creation = false;
    let env = Env::new(&dir.found, &config, &dir.clock);
    let err = call(&env, "1", Op::Do(PLAIN)).expect_err("refused");
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            "1: a text edit can't change a stamp",
            "the text would start with a stamp-like word; reword it first"
        )
    );
    assert_eq!(dir.todo(), "owner:bob 2026-09-01 foo\n");
}

#[test]
fn a_takeover_that_exposes_a_done_mark_word_is_refused() {
    let todo = "owner:bob started:2026-09-29T0900Z x marks spot\n";
    let dir = folder(todo, None);
    let mut env = dir.env();
    env.actor = Actor::named("alice").expect("actor");
    let err = call(&env, "1", Op::Start(TAKEOVER)).expect_err("refused");
    assert_eq!(err.code(), Code::WrongState);
    assert_eq!(dir.todo(), todo);
}

#[test]
fn a_human_finish_drops_the_owner_of_a_todo_line() {
    let dir = folder("A owner:bob\n", None);
    let out = call(&dir.env(), "1", Op::Do(PLAIN)).expect("do");
    assert_eq!(dir.todo(), "x 2026-09-29 A\n");
    assert_eq!(out.messages(), said(&["1 done: A"]));
}

#[test]
fn holders_are_told_apart_by_state() {
    let dir = folder(
        "A id:a owner:agent2\nx 2026-09-28 B id:b owner:agent2\n",
        None,
    );
    let mut env = dir.env();
    env.actor = Actor::named("agent1").expect("actor");
    let out = call(&env, "id:a", Op::Tag("+x")).expect("tag");
    assert_eq!(out.messages(), said(&["a tagged +x"]));
    let out = call(&env, "id:b", Op::Stop(PLAIN)).expect("stop");
    assert_eq!(out.messages(), said(&["b is not underway"]));
}
