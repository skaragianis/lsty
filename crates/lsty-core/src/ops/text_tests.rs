use super::{
    Actor, ClaimOpts, Outcome, append, del_term, due, parse_refs, prepend,
    replace, tag,
    test_util::{folder, parts, said, strings},
    untag,
};
use crate::{
    errs::{Code, Error},
    list::State,
};

/// One call of an operation.
#[derive(Clone, Copy)]
enum Op {
    Append(&'static str),
    Prepend(&'static str),
    Replace(&'static str),
    Del(&'static str),
    Tag(&'static [&'static str]),
    Untag(&'static [&'static str]),
    Due(&'static str),
}

/// What a call gives: the file and the messages after it, a no-op with
/// its messages, or a refusal (code, message, hint) that leaves the file.
#[derive(Clone, Copy)]
enum Want {
    Done(&'static str, &'static [&'static str]),
    NoOp(&'static [&'static str]),
    Refused(Code, &'static str, &'static str),
}

/// References, the call and the expectation.
type Step = (&'static str, Op, Want);

/// Name, file at the start, the agent (empty for the human), steps.
type Case = (&'static str, &'static str, &'static str, &'static [Step]);

fn call(env: &super::Env<'_>, refs: &str, op: Op) -> Result<Outcome, Error> {
    let targets = parse_refs(refs)?;
    match op {
        Op::Append(t) => append(env, &targets, t),
        Op::Prepend(t) => prepend(env, &targets, t),
        Op::Replace(t) => replace(env, &targets, t, ClaimOpts::default()),
        Op::Del(t) => del_term(env, &targets, t),
        Op::Tag(t) => tag(env, &targets, &strings(t)),
        Op::Untag(t) => untag(env, &targets, &strings(t)),
        Op::Due(t) => due(env, &targets, t),
    }
}

fn run_case(case: &Case) {
    let (name, todo, agent, steps) = case;
    let dir = folder(todo, Some("x 2026-09-01 Old\n"));
    let mut env = dir.env();
    env.actor = Actor::named(agent).expect("actor");
    for (n, (refs, op, want)) in steps.iter().enumerate() {
        let before = dir.todo();
        let got = call(&env, refs, *op);
        let at = format!("{name} step {n}");
        check(&at, &got, want, &before, &dir.todo());
    }
}

fn check(
    at: &str,
    got: &Result<Outcome, Error>,
    want: &Want,
    before: &str,
    now: &str,
) {
    match (want, got) {
        (Want::Done(file, msgs), Ok(out)) => {
            assert_eq!(now, *file, "{at}");
            assert_eq!(out.messages(), said(msgs), "{at}");
            assert!(!out.no_op(), "{at}");
        }
        (Want::NoOp(msgs), Ok(out)) => {
            assert_eq!(now, before, "{at}");
            assert_eq!(out.messages(), said(msgs), "{at}");
            assert!(out.no_op(), "{at}");
        }
        (Want::Refused(code, msg, hint), Err(err)) => {
            assert_eq!(parts(err), (*code, *msg, *hint), "{at}");
            assert_eq!(now, before, "{at}");
        }
        (_, Ok(out)) => panic!("{at}: succeeded: {:?}", out.messages()),
        (_, Err(err)) => panic!("{at}: failed: {err:?}"),
    }
}

const WS: Code = Code::WrongState;
const IV: Code = Code::InvalidValue;

const APPEND: &[Case] = &[
    (
        "16.1",
        "Call client\n",
        "",
        &[(
            "1",
            Op::Append("about Q3"),
            Want::Done("Call client about Q3\n", &["1 Call client about Q3"]),
        )],
    ),
    (
        "16.2",
        "Call client\n",
        "",
        &[
            (
                "1",
                Op::Append(", then invoice"),
                Want::Done(
                    "Call client, then invoice\n",
                    &["1 Call client, then invoice"],
                ),
            ),
            (
                "1",
                Op::Append("; later"),
                Want::Done(
                    "Call client, then invoice; later\n",
                    &["1 Call client, then invoice; later"],
                ),
            ),
        ],
    ),
    (
        "16.3",
        "Call\n",
        "",
        &[
            (
                "1",
                Op::Append("due:fri"),
                Want::Done(
                    "Call due:2026-10-02\n",
                    &["1 Call due:2026-10-02"],
                ),
            ),
            (
                "1",
                Op::Append("due:fri"),
                Want::Done(
                    "Call due:2026-10-02 due:2026-10-02\n",
                    &[
                        "1 Call due:2026-10-02 due:2026-10-02",
                        "1 already has due:2026-10-02; the first one counts",
                    ],
                ),
            ),
        ],
    ),
    (
        "16.4",
        "Call\n",
        "",
        &[
            (
                "1",
                Op::Append("due:soon"),
                Want::Refused(IV, "not a date: soon", ""),
            ),
            (
                "1",
                Op::Append("rec:1x"),
                Want::Refused(IV, "not a recurrence: 1x", ""),
            ),
        ],
    ),
    (
        "16.5",
        "(A) 2026-09-20 Call\n",
        "",
        &[(
            "1",
            Op::Prepend("URGENT"),
            Want::Done(
                "(A) 2026-09-20 URGENT Call\n",
                &["1 (A) 2026-09-20 URGENT Call"],
            ),
        )],
    ),
    (
        "16.6",
        "Call\n",
        "",
        &[(
            "1",
            Op::Prepend("(A) urgent"),
            Want::Done("(A) urgent Call\n", &["1 (A) urgent Call"]),
        )],
    ),
    (
        "16.7",
        "Call\n",
        "",
        &[
            (
                "1",
                Op::Prepend("x"),
                Want::Refused(
                    WS,
                    "1: a text edit can't change a stamp",
                    "use lsty do 1",
                ),
            ),
            (
                "1",
                Op::Prepend("2026-01-01 note"),
                Want::Refused(
                    WS,
                    "1: a text edit can't change a stamp",
                    "the creation date is stamped",
                ),
            ),
        ],
    ),
    (
        "16.8",
        "x 2026-09-28 Fix\n",
        "",
        &[(
            "1",
            Op::Append("(late)"),
            Want::Done(
                "x 2026-09-28 Fix (late)\n",
                &["1 x 2026-09-28 Fix (late)"],
            ),
        )],
    ),
    (
        "16.9",
        "Call\n",
        "",
        &[
            (
                "1",
                Op::Append("  "),
                Want::Refused(Code::Usage, "append needs text", ""),
            ),
            (
                "1",
                Op::Append("a\nb"),
                Want::Refused(IV, "a task is one line", ""),
            ),
        ],
    ),
    (
        "16.10",
        "A\nB\n",
        "",
        &[(
            "1,2",
            Op::Append("more"),
            Want::Done("A more\nB more\n", &["1 A more", "2 B more"]),
        )],
    ),
    (
        "16.11",
        "A\nB\n",
        "",
        &[(
            "1,2",
            Op::Append("x id:k"),
            Want::Refused(Code::IdTaken, "id:k is taken by 2", ""),
        )],
    ),
    (
        "16.44 done item",
        "A\n",
        "",
        &[
            (
                "done:1",
                Op::Append("x"),
                Want::Refused(
                    WS,
                    "done:1 is read-only history",
                    "done:1 is archived; use restore",
                ),
            ),
            (
                "done:1",
                Op::Due("fri"),
                Want::Refused(
                    WS,
                    "done:1 is read-only history",
                    "done:1 is archived; use restore",
                ),
            ),
        ],
    ),
];

const CLAIM: &str =
    "Call +w id:k3f9 dep:a rec:1w started:2026-09-29T0800Z owner:agent1\n";

const REPLACE: &[Case] = &[
    (
        "16.12",
        "(A) 2026-09-20 Call client +work @phone color:red\n",
        "",
        &[(
            "1",
            Op::Replace("Ring client +sales"),
            Want::Done(
                "(A) 2026-09-20 Ring client +sales\n",
                &["1 (A) 2026-09-20 Ring client +sales"],
            ),
        )],
    ),
    (
        "16.13 later date",
        "Call\n",
        "",
        &[(
            "1",
            Op::Replace("(A) (B) 2026-01-01 Ring"),
            Want::Done(
                "(A) (B) 2026-01-01 Ring\n",
                &["1 (A) (B) 2026-01-01 Ring"],
            ),
        )],
    ),
    (
        "16.13",
        "(A) 2026-09-20 Call client +work\n",
        "",
        &[(
            "1",
            Op::Replace("(B) Ring"),
            Want::Done("(B) 2026-09-20 Ring\n", &["1 (B) 2026-09-20 Ring"]),
        )],
    ),
    (
        "16.14",
        CLAIM,
        "",
        &[(
            "1",
            Op::Replace("Ring"),
            Want::Done(
                "Ring dep:a rec:1w started:2026-09-29T0800Z owner:agent1 id:k3f9\n",
                &[
                    "1 Ring dep:a rec:1w started:2026-09-29T0800Z owner:agent1 id:k3f9",
                    "1 is claimed by agent1",
                ],
            ),
        )],
    ),
    (
        "16.15",
        CLAIM,
        "",
        &[(
            "1",
            Op::Replace("Ring rec:2w dep:b"),
            Want::Done(
                "Ring rec:2w dep:b started:2026-09-29T0800Z owner:agent1 id:k3f9\n",
                &[
                    "1 Ring rec:2w dep:b started:2026-09-29T0800Z owner:agent1 id:k3f9",
                    "1 is claimed by agent1",
                ],
            ),
        )],
    ),
    (
        "16.16",
        CLAIM,
        "",
        &[(
            "1",
            Op::Replace("Ring id:k3f9 now"),
            Want::Done(
                "Ring id:k3f9 now dep:a rec:1w started:2026-09-29T0800Z owner:agent1\n",
                &[
                    "1 Ring id:k3f9 now dep:a rec:1w started:2026-09-29T0800Z owner:agent1",
                    "1 is claimed by agent1",
                ],
            ),
        )],
    ),
    (
        "16.17",
        CLAIM,
        "",
        &[(
            "1",
            Op::Replace("Ring id:other"),
            Want::Refused(
                WS,
                "1: a text edit can't change a stamp",
                "an id is never changed; lsty untag 1 id:k3f9 removes it",
            ),
        )],
    ),
    (
        "16.18",
        "Call\n",
        "",
        &[(
            "1",
            Op::Replace("2026-01-01 Ring"),
            Want::Refused(
                WS,
                "1: a text edit can't change a stamp",
                "the creation date is stamped",
            ),
        )],
    ),
    (
        "16.18 spaces",
        "(A) Call\n",
        "",
        &[(
            "1",
            Op::Replace("(A)  2026-01-01 Ring"),
            Want::Refused(
                WS,
                "1: a text edit can't change a stamp",
                "the creation date is stamped",
            ),
        )],
    ),
    (
        "16.19",
        "Call\n",
        "",
        &[(
            "1",
            Op::Replace("x Ring"),
            Want::Refused(
                WS,
                "1: a text edit can't change a stamp",
                "use lsty do 1",
            ),
        )],
    ),
    (
        "16.20",
        "x 2026-09-28 2026-09-20 Fix +w pri:B\n",
        "",
        &[
            (
                "1",
                Op::Replace("Mend"),
                Want::Done(
                    "x 2026-09-28 2026-09-20 Mend pri:B\n",
                    &["1 x 2026-09-28 2026-09-20 Mend pri:B"],
                ),
            ),
            ("1", Op::Replace("(B) Mend"), Want::NoOp(&["1 unchanged"])),
            (
                "1",
                Op::Replace("(A) Mend"),
                Want::Refused(
                    WS,
                    "1 is done; reopen it first",
                    "use lsty reopen 1",
                ),
            ),
        ],
    ),
    (
        "16.21",
        "(A) Call +w\n",
        "",
        &[("1", Op::Replace("Call +w"), Want::NoOp(&["1 unchanged"]))],
    ),
    (
        "16.22",
        "Call started:2026-09-29T0800Z owner:agent1\n",
        "agent1",
        &[(
            "1",
            Op::Replace("Ring owner:agent2"),
            Want::Refused(
                WS,
                "1: a text edit can't change a claim",
                "use lsty stop",
            ),
        )],
    ),
];

const DEL: &[Case] = &[
    (
        "only whole words are deleted",
        "repay the pay\n",
        "",
        &[(
            "1",
            Op::Del("pay"),
            Want::Done("repay the\n", &["1 repay the"]),
        )],
    ),
    (
        "a nul byte in the description is text",
        "a \0 b pay c\n",
        "",
        &[(
            "1",
            Op::Del("pay"),
            Want::Done("a \0 b c\n", &["1 a \0 b c"]),
        )],
    ),
    (
        "a unicode space after a word is not a boundary",
        "a pay\u{3000}\n",
        "",
        &[("1", Op::Del("pay"), Want::NoOp(&["1 has no \"pay\""]))],
    ),
    (
        "a unicode space before a word is not a boundary",
        "\u{3000}pay pay b\n",
        "",
        &[(
            "1",
            Op::Del("pay"),
            Want::Done("\u{3000}pay b\n", &["1 \u{3000}pay b"]),
        )],
    ),
    (
        "a word inside a tag stays",
        "pay +pay ab\n",
        "",
        &[("1", Op::Del("pay"), Want::Done("+pay ab\n", &["1 +pay ab"]))],
    ),
    (
        "a term with a space is a run of whole words",
        "pay rent and prepay rent now pay rent\n",
        "",
        &[(
            "1",
            Op::Del("pay rent"),
            Want::Done("and prepay rent now\n", &["1 and prepay rent now"]),
        )],
    ),
    (
        "white space at the start stays",
        " (B) pay ab plan\n",
        "",
        &[(
            "1",
            Op::Del("pay"),
            Want::Done(" (B) ab plan\n", &["1  (B) ab plan"]),
        )],
    ),
    (
        "white space at the start stays in front of the next word",
        " pay (B) ab\n",
        "",
        &[("1", Op::Del("pay"), Want::Done(" (B) ab\n", &["1  (B) ab"]))],
    ),
    (
        "white space after the priority stays",
        "(Z)  x 2025-12-31 call 2025-12-31\n",
        "",
        &[(
            "1",
            Op::Del("2025-12-31"),
            Want::Done("(Z)  x call\n", &["1 (Z)  x call"]),
        )],
    ),
    (
        "words left over do not become a priority",
        "pay (B) ab\n(A) pay (B) cd\n",
        "",
        &[
            (
                "1",
                Op::Del("pay"),
                Want::Refused(
                    WS,
                    "1: a text edit can't change a stamp",
                    "the text would start with a stamp-like word; reword it first",
                ),
            ),
            (
                "2",
                Op::Del("pay"),
                Want::Done("pay (B) ab\n(A) (B) cd\n", &["2 (A) (B) cd"]),
            ),
        ],
    ),
    (
        "white space alone is not left behind",
        "x 2026-09-29  ab\n",
        "",
        &[(
            "1",
            Op::Del("ab"),
            Want::Refused(WS, "1 would be empty", "use lsty del 1"),
        )],
    ),
    (
        "words left over do not become a done mark",
        "pay x ab\n",
        "",
        &[(
            "1",
            Op::Del("pay"),
            Want::Refused(
                WS,
                "1: a text edit can't change a stamp",
                "the text would start with a stamp-like word; reword it first",
            ),
        )],
    ),
    (
        "words left over do not become a date",
        "pay 2025-01-01 ab\n",
        "",
        &[(
            "1",
            Op::Del("pay"),
            Want::Refused(
                WS,
                "1: a text edit can't change a stamp",
                "the text would start with a stamp-like word; reword it first",
            ),
        )],
    ),
    (
        "a priority with only white space left is empty",
        "(A)  +x\n",
        "",
        &[(
            "1",
            Op::Untag(&["+x"]),
            Want::Refused(WS, "1 would be empty", "use lsty del 1"),
        )],
    ),
    (
        "16.23",
        "Call the client now\n",
        "",
        &[(
            "1",
            Op::Del("the"),
            Want::Done("Call client now\n", &["1 Call client now"]),
        )],
    ),
    (
        "16.24",
        "a b a b\n",
        "",
        &[("1", Op::Del("a"), Want::Done("b b\n", &["1 b b"]))],
    ),
    (
        "16.25",
        "Call\n",
        "",
        &[
            ("1", Op::Del("zzz"), Want::NoOp(&["1 has no \"zzz\""])),
            (
                "1",
                Op::Del("Call"),
                Want::Refused(WS, "1 would be empty", "use lsty del 1"),
            ),
        ],
    ),
    (
        "16.26",
        "Call id:k3f9\n",
        "",
        &[
            (
                "1",
                Op::Del("id:k3f9"),
                Want::Refused(
                    WS,
                    "1: a text edit can't change a stamp",
                    "an id is never changed; lsty untag 1 id:k3f9 removes it",
                ),
            ),
            ("1", Op::Del("3f"), Want::NoOp(&["1 has no \"3f\""])),
        ],
    ),
    (
        "del is case sensitive",
        "Call call\n",
        "",
        &[("1", Op::Del("call"), Want::Done("Call\n", &["1 Call"]))],
    ),
];

const TAG: &[Case] = &[
    (
        "16.27 16.28",
        "Call\n",
        "",
        &[
            (
                "1",
                Op::Tag(&["+work", "@phone", "color:red"]),
                Want::Done(
                    "Call +work @phone color:red\n",
                    &[
                        "1 tagged +work",
                        "1 tagged @phone",
                        "1 tagged color:red",
                    ],
                ),
            ),
            (
                "1",
                Op::Tag(&["+work", "@phone", "color:red"]),
                Want::NoOp(&[
                    "1 already has +work",
                    "1 already has @phone",
                    "1 already has color:red",
                ]),
            ),
        ],
    ),
    (
        "16.29",
        "Call due:2026-10-01 +w\n",
        "",
        &[(
            "1",
            Op::Tag(&["due:fri"]),
            Want::Done(
                "Call due:2026-10-02 +w\n",
                &["1 due:2026-10-01 is now due:2026-10-02"],
            ),
        )],
    ),
    (
        "16.30",
        "Call t:2026-10-01 t:2026-11-01\n",
        "",
        &[(
            "1",
            Op::Tag(&["t:tom"]),
            Want::Done(
                "Call t:2026-09-30 t:2026-11-01\n",
                &["1 t:2026-10-01 is now t:2026-09-30"],
            ),
        )],
    ),
    (
        "16.31",
        "Call rec:1w\n",
        "",
        &[
            (
                "1",
                Op::Tag(&["rec:+2w"]),
                Want::Done("Call rec:+2w\n", &["1 rec:1w is now rec:+2w"]),
            ),
            (
                "1",
                Op::Tag(&["rec:often"]),
                Want::Refused(IV, "not a recurrence: often", ""),
            ),
        ],
    ),
    (
        "16.32",
        "Call file:a.pdf\n",
        "",
        &[(
            "1",
            Op::Tag(&["file:b.pdf"]),
            Want::Done(
                "Call file:a.pdf file:b.pdf\n",
                &["1 tagged file:b.pdf"],
            ),
        )],
    ),
    (
        "16.33 work",
        "Call\n",
        "",
        &[(
            "1",
            Op::Tag(&["work"]),
            Want::Refused(
                Code::Usage,
                "\"work\" is not a tag",
                "use +name, @name or key:value",
            ),
        )],
    ),
    (
        "16.33 empty",
        "Call\n",
        "",
        &[(
            "1",
            Op::Tag(&["due:"]),
            Want::Refused(
                Code::Usage,
                "\"due:\" is not a tag",
                "use +name, @name or key:value",
            ),
        )],
    ),
    (
        "16.33 colons",
        "Call\n",
        "",
        &[(
            "1",
            Op::Tag(&["a:b:c"]),
            Want::Refused(
                Code::Usage,
                "\"a:b:c\" is not a tag",
                "use +name, @name or key:value",
            ),
        )],
    ),
    (
        "16.34",
        "Call +work +work @phone due:2026-10-01\n",
        "",
        &[
            (
                "1",
                Op::Untag(&["+work"]),
                Want::Done(
                    "Call @phone due:2026-10-01\n",
                    &["1 untagged +work"],
                ),
            ),
            (
                "1",
                Op::Untag(&["due:"]),
                Want::Done("Call @phone\n", &["1 untagged due:"]),
            ),
        ],
    ),
    (
        "16.35",
        "Call +work\n",
        "",
        &[("1", Op::Untag(&["+Work"]), Want::NoOp(&["1 has no +Work"]))],
    ),
    (
        "16.45",
        "x 2026-09-28 Fix pri:B\n",
        "",
        &[
            (
                "1",
                Op::Tag(&["pri:A"]),
                Want::Refused(
                    WS,
                    "1 is done; reopen it first",
                    "use lsty reopen 1",
                ),
            ),
            (
                "1",
                Op::Untag(&["pri:"]),
                Want::Refused(
                    WS,
                    "1 is done; reopen it first",
                    "use lsty reopen 1",
                ),
            ),
            (
                "1",
                Op::Tag(&["pri:B"]),
                Want::NoOp(&["1 already has pri:B"]),
            ),
        ],
    ),
    (
        "16.46",
        "Fix\n",
        "",
        &[(
            "1",
            Op::Tag(&["pri:B"]),
            Want::Done("Fix pri:B\n", &["1 tagged pri:B"]),
        )],
    ),
    (
        "untag id",
        "A id:k3f9\n",
        "",
        &[(
            "1",
            Op::Untag(&["id:k3f9"]),
            Want::Done("A\n", &["1 untagged id:k3f9"]),
        )],
    ),
];

const START: &str = "started:2026-09-29T0900Z";

const CLAIMS: &[Case] = &[
    (
        "16.36 add",
        "Call\n",
        "agent1",
        &[(
            "1",
            Op::Tag(&[START]),
            Want::Refused(
                WS,
                "1: a text edit can't change a claim",
                "use lsty start",
            ),
        )],
    ),
    (
        "16.36 remove",
        "Call started:2026-09-29T0800Z owner:agent2\n",
        "agent1",
        &[(
            "1",
            Op::Untag(&["owner:agent2"]),
            Want::Refused(
                WS,
                "1: a text edit can't change a claim",
                "use lsty stop",
            ),
        )],
    ),
    (
        "16.37",
        "Call\n",
        "",
        &[(
            "1",
            Op::Tag(&[START, "owner:agent1"]),
            Want::Done(
                "Call started:2026-09-29T0900Z owner:agent1\n",
                &[
                    "1 tagged started:2026-09-29T0900Z",
                    "1 tagged owner:agent1",
                ],
            ),
        )],
    ),
    (
        "an agent can't add ended:",
        "Call started:2026-09-29T0800Z owner:agent1\n",
        "agent1",
        &[(
            "1",
            Op::Tag(&["ended:2026-09-29T0900Z"]),
            Want::Refused(
                WS,
                "1: a text edit can't change a claim",
                "ended: is written by lsty do",
            ),
        )],
    ),
    (
        "an agent can't remove ended:",
        "x 2026-09-29 Call started:2026-09-29T0800Z ended:2026-09-29T0900Z\n",
        "agent1",
        &[(
            "1",
            Op::Untag(&["ended:"]),
            Want::Refused(
                WS,
                "1: a text edit can't change a claim",
                "ended: is written by lsty do",
            ),
        )],
    ),
    (
        "a human can set and remove ended:",
        "Call started:2026-09-29T0800Z\n",
        "",
        &[
            (
                "1",
                Op::Tag(&["ended:2026-09-29T0900Z"]),
                Want::Done(
                    "Call started:2026-09-29T0800Z ended:2026-09-29T0900Z\n",
                    &["1 tagged ended:2026-09-29T0900Z"],
                ),
            ),
            (
                "1",
                Op::Untag(&["ended:"]),
                Want::Done(
                    "Call started:2026-09-29T0800Z\n",
                    &["1 untagged ended:"],
                ),
            ),
        ],
    ),
    (
        "16.38",
        "A id:a\nB dep:a\n",
        "",
        &[(
            "2",
            Op::Tag(&[START]),
            Want::Refused(
                Code::Blocked,
                "2 is blocked",
                "finish what it waits for first",
            ),
        )],
    ),
    (
        "16.39",
        "A started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "1",
            Op::Untag(&["started:"]),
            Want::Done(
                "A\n",
                &["1 untagged started:", "1 is claimed by agent1"],
            ),
        )],
    ),
    (
        "16.40",
        "A started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "1",
            Op::Untag(&["owner:"]),
            Want::Done(
                "A started:2026-09-29T0800Z\n",
                &["1 untagged owner:", "1 is claimed by agent1"],
            ),
        )],
    ),
    (
        "dropping the owner of a started blocked item is no start",
        "A id:a\nB dep:a started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "2",
            Op::Untag(&["owner:"]),
            Want::Done(
                "A id:a\nB dep:a started:2026-09-29T0800Z\n",
                &["2 untagged owner:", "2 is claimed by agent1"],
            ),
        )],
    ),
];

const DUE: &[Case] = &[
    (
        "16.41",
        "Call\n",
        "",
        &[
            (
                "1",
                Op::Due("fri"),
                Want::Done("Call due:2026-10-02\n", &["1 due 2026-10-02"]),
            ),
            (
                "1",
                Op::Due("fri"),
                Want::NoOp(&["1 is already due 2026-10-02"]),
            ),
            (
                "1",
                Op::Due(""),
                Want::Done("Call\n", &["1 due date cleared"]),
            ),
            ("1", Op::Due(""), Want::NoOp(&["1 has no due date"])),
        ],
    ),
    (
        "16.42",
        "A\nB\n",
        "",
        &[(
            "1,2",
            Op::Due("+3d"),
            Want::Done(
                "A due:2026-10-02\nB due:2026-10-02\n",
                &["1 due 2026-10-02", "2 due 2026-10-02"],
            ),
        )],
    ),
    (
        "16.43",
        "A\n",
        "",
        &[(
            "1",
            Op::Due("someday"),
            Want::Refused(IV, "not a date: someday", ""),
        )],
    ),
];

const TAGHINT: &str = "use +name, @name or key:value";
const EMPTY_HINT: &str = "use lsty del 1";
const ARCHIVED: &str = "done:1 is archived; use restore";
const ARCHIVED_MSG: &str = "done:1 is read-only history";

const ARCHIVED_OPS: [Op; 7] = [
    Op::Append("x"),
    Op::Prepend("x"),
    Op::Replace("x"),
    Op::Del("x"),
    Op::Tag(&["+x"]),
    Op::Untag(&["+x"]),
    Op::Due("fri"),
];

const MORE: &[Case] = &[
    (
        "replace done with priority prefix",
        "x (A) 2026-09-28 Fix\n",
        "",
        &[(
            "1",
            Op::Replace("(A) Mend"),
            Want::Done(
                "x (A) 2026-09-28 Mend\n",
                &["1 x (A) 2026-09-28 Mend"],
            ),
        )],
    ),
    (
        "a term inside a word is not deleted",
        "(A) abc\n",
        "",
        &[("1", Op::Del("b"), Want::NoOp(&["1 has no \"b\""]))],
    ),
    (
        "del term with a double quote",
        "Call\n",
        "",
        &[("1", Op::Del("a\"b"), Want::NoOp(&["1 has no \"a\\\"b\""]))],
    ),
    (
        "16.9 empty term",
        "A\n",
        "",
        &[(
            "1",
            Op::Del(" "),
            Want::Refused(Code::Usage, "del needs a term", ""),
        )],
    ),
    (
        "untag value",
        "Call due:2026-10-01 due:2026-11-01\n",
        "",
        &[(
            "1",
            Op::Untag(&["due:2026-11-01"]),
            Want::Done(
                "Call due:2026-10-01\n",
                &["1 untagged due:2026-11-01"],
            ),
        )],
    ),
    (
        "untag key removes the id",
        "Call id:k3f9\n",
        "",
        &[(
            "1",
            Op::Untag(&["id:"]),
            Want::Done("Call\n", &["1 untagged id:"]),
        )],
    ),
    (
        "untag plain word",
        "A\n",
        "",
        &[(
            "1",
            Op::Untag(&["work"]),
            Want::Refused(Code::Usage, "\"work\" is not a tag", TAGHINT),
        )],
    ),
    (
        "untag two colons",
        "A\n",
        "",
        &[(
            "1",
            Op::Untag(&["a:b:"]),
            Want::Refused(Code::Usage, "\"a:b:\" is not a tag", TAGHINT),
        )],
    ),
    (
        "untag the only tag",
        "+work\n",
        "",
        &[(
            "1",
            Op::Untag(&["+work"]),
            Want::Refused(WS, "1 would be empty", EMPTY_HINT),
        )],
    ),
    (
        "untag started on a line made only of the claim",
        "started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "1",
            Op::Untag(&["started:"]),
            Want::Refused(WS, "1 would be empty", EMPTY_HINT),
        )],
    ),
    (
        "del the description of a claimed item",
        "Call started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "1",
            Op::Del("Call started:2026-09-29T0800Z"),
            Want::Refused(WS, "1 would be empty", EMPTY_HINT),
        )],
    ),
    (
        "clear the only due token",
        "due:2026-10-02\n",
        "",
        &[(
            "1",
            Op::Due(""),
            Want::Refused(WS, "1 would be empty", EMPTY_HINT),
        )],
    ),
    (
        "id is never changed by tag",
        "A id:k\n",
        "",
        &[(
            "1",
            Op::Tag(&["id:z"]),
            Want::Refused(WS, "1 already has id:k", "an item has one id"),
        )],
    ),
    (
        "replace naming started keeps the typed one",
        "Call started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "1",
            Op::Replace("Ring started:2026-09-30T0800Z"),
            Want::Done(
                "Ring started:2026-09-30T0800Z owner:agent1\n",
                &[
                    "1 Ring started:2026-09-30T0800Z owner:agent1",
                    "1 is claimed by agent1",
                ],
            ),
        )],
    ),
    (
        "replace keeps ended: unless the text names it",
        "Call started:2026-09-29T0800Z ended:2026-09-29T0900Z\n",
        "",
        &[
            (
                "1",
                Op::Replace("Ring"),
                Want::Done(
                    "Ring started:2026-09-29T0800Z ended:2026-09-29T0900Z\n",
                    &["1 Ring started:2026-09-29T0800Z \
                         ended:2026-09-29T0900Z"],
                ),
            ),
            (
                "1",
                Op::Replace("Ring ended:2026-09-30T0900Z"),
                Want::Done(
                    "Ring ended:2026-09-30T0900Z started:2026-09-29T0800Z\n",
                    &[
                        "1 Ring ended:2026-09-30T0900Z started:2026-09-29T0800Z",
                    ],
                ),
            ),
        ],
    ),
    (
        "del leaves the owner behind",
        "Call started:2026-09-29T0800Z owner:agent1\n",
        "",
        &[(
            "1",
            Op::Del("started:2026-09-29T0800Z"),
            Want::Done("Call\n", &["1 Call", "1 is claimed by agent1"]),
        )],
    ),
    (
        "due replaces the first of two due tags",
        "Call due:2026-10-01 due:2026-10-02\n",
        "",
        &[(
            "1",
            Op::Due("fri"),
            Want::Done(
                "Call due:2026-10-02 due:2026-10-02\n",
                &["1 due 2026-10-02"],
            ),
        )],
    ),
    (
        "tag replaces the first of two due tags",
        "Call due:2026-10-01 due:2026-10-02\n",
        "",
        &[(
            "1",
            Op::Tag(&["due:2026-10-02"]),
            Want::Done(
                "Call due:2026-10-02 due:2026-10-02\n",
                &["1 due:2026-10-01 is now due:2026-10-02"],
            ),
        )],
    ),
];

#[test]
fn more_rows() {
    run_all(MORE);
}

#[test]
fn every_operation_refuses_an_archived_task() {
    for op in ARCHIVED_OPS {
        let dir = folder("A\n", Some("x 2026-09-28 B\n"));
        let env = dir.env();
        let err = call(&env, "done:1", op).expect_err("refused");
        assert_eq!(
            parts(&err),
            (WS, ARCHIVED_MSG, ARCHIVED),
            "{}",
            op_name(op)
        );
    }
}

fn op_name(op: Op) -> &'static str {
    match op {
        Op::Append(_) => "append",
        Op::Prepend(_) => "prepend",
        Op::Replace(_) => "replace",
        Op::Del(_) => "del",
        Op::Tag(_) => "tag",
        Op::Untag(_) => "untag",
        Op::Due(_) => "due",
    }
}

#[test]
fn a_human_claim_by_text_is_doing_and_owned() {
    let dir = folder("Call\n", None);
    let env = dir.env();
    let out =
        call(&env, "1", Op::Tag(&[START, "owner:agent1"])).expect("allowed");
    let after = out.after();
    let task = out.tasks_after().next().expect("item");
    assert_eq!(task.state(), State::Doing);
    assert_eq!(after.tags(task).owner, Some(b"agent1".as_slice()));
}

#[test]
fn a_bad_due_input_lists_the_accepted_forms() {
    let dir = folder("A\n", None);
    let env = dir.env();
    let err = call(&env, "1", Op::Due("someday")).expect_err("refused");
    assert_eq!(err.text("field"), Some("due"));
    assert!(err.field("accepted").is_some());
}

#[test]
fn an_invalid_agent_name_is_refused() {
    for name in ["a b", "a:b", "a\u{3000}b"] {
        let err = Actor::named(name).expect_err("refused");
        assert_eq!(
            parts(&err),
            (IV, "an agent name has no whitespace or colon", "")
        );
        assert_eq!(err.text("field"), Some("owner"));
    }
    assert_eq!(Actor::named("").expect("human"), Actor::Human);
}

/// Name, file bytes, call, file bytes after.
type Bytes = (&'static str, &'static [u8], Op, &'static [u8]);

const NON_UTF8: [Bytes; 5] = [
    (
        "prepend trims a unicode space at the end",
        b"caf\xe9 x\xe3\x80\x80\n",
        Op::Prepend("X"),
        b"X caf\xe9 x\n",
    ),
    (
        "del trims a unicode space at the end",
        b"caf\xe9 a \xe3\x80\x80\n",
        Op::Del("a"),
        b"caf\xe9\n",
    ),
    (
        "prepend keeps a space before an invalid byte",
        b"a\xe3\x80\x80\xe9\n",
        Op::Prepend("X"),
        b"X a\xe3\x80\x80\xe9\n",
    ),
    (
        "prepend keeps a unicode space inside",
        b"\xe3\x80\x80caf\xe9\n",
        Op::Prepend("X"),
        b"X \xe3\x80\x80caf\xe9\n",
    ),
    (
        "del trims a unicode space at the start",
        b"z \xe3\x80\x80\xffq\n",
        Op::Del("z"),
        b"\xffq\n",
    ),
];

#[test]
fn non_utf8_lines_are_trimmed_up_to_the_invalid_byte() {
    for (name, before, op, after) in NON_UTF8 {
        let dir = folder("", None);
        std::fs::write(&dir.found.todo, before).expect("write");
        let env = dir.env();
        call(&env, "1", op).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let got = std::fs::read(&dir.found.todo).expect("read");
        assert_eq!(got, after, "{name}");
    }
}

fn run_all(cases: &[Case]) {
    for case in cases {
        run_case(case);
    }
}

#[test]
fn append_and_prepend_rows() {
    run_all(APPEND);
}

#[test]
fn replace_rows() {
    run_all(REPLACE);
}

#[test]
fn del_term_rows() {
    run_all(DEL);
}

#[test]
fn tag_and_untag_rows() {
    run_all(TAG);
}

#[test]
fn claim_rows() {
    run_all(CLAIMS);
}

#[test]
fn due_rows() {
    run_all(DUE);
}

#[test]
fn refusals_name_the_field() {
    let dir = folder("Call\n", None);
    let env = dir.env();
    let targets = parse_refs("1").expect("refs");
    let err = append(&env, &targets, "due:soon").expect_err("refused");
    assert_eq!(err.text("field"), Some("due"));
    let err = append(&env, &targets, "rec:1x").expect_err("refused");
    assert_eq!(err.text("field"), Some("rec"));
    let err = append(&env, &targets, "a\nb").expect_err("refused");
    assert_eq!(err.text("field"), Some("text"));
    let err = due(&env, &targets, "someday").expect_err("refused");
    assert_eq!(err.text("field"), Some("due"));
}

#[test]
fn only_the_first_rec_of_a_text_is_checked() {
    let dir = folder("Call\n", None);
    let env = dir.env();
    let targets = parse_refs("1").expect("refs");
    append(&env, &targets, "rec:1w rec:often").expect("kept as typed");
    assert_eq!(dir.todo(), "Call rec:1w rec:often\n");
}

#[test]
fn a_claimed_task_edit_by_an_agent_keeps_text_edits_allowed() {
    let dir = folder("Call started:2026-09-29T0800Z owner:agent1\n", None);
    let mut env = dir.env();
    env.actor = Actor::named("agent1").expect("actor");
    let targets = parse_refs("1").expect("refs");
    append(&env, &targets, "more").expect("allowed");
    assert_eq!(
        dir.todo(),
        "Call started:2026-09-29T0800Z owner:agent1 more\n"
    );
}

/// A description and what it is after `del 1 X`.
const GAPS: &[(&str, &str)] = &[
    ("a X X b", "a b"),
    ("a X b", "a b"),
    ("X a", "a"),
    ("a X", "a"),
    ("X a X", "a"),
    ("a X  X b", "a b"),
    ("a  X  X  b", "a b"),
    ("a\tX\tb", "a b"),
    ("aX X Xb X", "aX Xb"),
];

#[test]
fn the_blanks_around_a_removed_term_become_one_space() {
    for (before, after) in GAPS {
        let dir = folder(format!("{before}\n"), None);
        let env = dir.env();
        call(&env, "1", Op::Del("X")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(dir.todo(), format!("{after}\n"), "{before:?}");
    }
}
