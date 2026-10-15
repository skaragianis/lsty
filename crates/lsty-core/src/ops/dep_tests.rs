use super::{
    Actor, AddParams, ClaimOpts, Env, Outcome, add, append, del, dep,
    parse_refs, prepend, replace, tag,
    test_util::{Sequence, folder, parts, said, strings},
    undep, untag,
};
use crate::errs::{Code, Error, Field};

const PLAIN: ClaimOpts = ClaimOpts { takeover: false };
const TAKEOVER: ClaimOpts = ClaimOpts { takeover: true };

/// One call.
#[derive(Clone, Copy)]
enum Call {
    /// Waiting, prerequisites, chosen id.
    Dep(&'static str, &'static str, Option<&'static str>),
    Undep(&'static str, &'static str),
    Del(&'static str, ClaimOpts),
    Tag(&'static str, &'static str),
    Append(&'static str, &'static str),
    Replace(&'static str, &'static str),
    Untag(&'static str, &'static [&'static str]),
    Add(&'static str),
    Prepend(&'static str, &'static str),
}

/// What a call that succeeds gives; a field left at its default is not
/// checked.
#[derive(Clone, Copy)]
struct Done {
    file: &'static str,
    msgs: Option<&'static [&'static str]>,
    no_op: bool,
    changed: Option<&'static [u32]>,
    tasks: Option<&'static [u32]>,
    before: Option<&'static [u32]>,
    blocked: Option<bool>,
}

const DONE: Done = Done {
    file: "",
    msgs: None,
    no_op: false,
    changed: None,
    tasks: None,
    before: None,
    blocked: None,
};

/// What a call that fails gives.
#[derive(Clone, Copy)]
struct Refused {
    code: Code,
    hint: Option<&'static str>,
    message: Option<&'static str>,
    path: Option<&'static [u32]>,
}

const REFUSED: Refused = Refused {
    code: Code::Usage,
    hint: None,
    message: None,
    path: None,
};

#[derive(Clone, Copy)]
enum Want {
    Done(Done),
    Refused(Refused),
}

struct Step {
    actor: &'static str,
    call: Call,
    want: Want,
}

struct Case {
    name: &'static str,
    todo: &'static str,
    done: Option<&'static str>,
    steps: &'static [Step],
}

const fn step(call: Call, want: Want) -> Step {
    Step {
        actor: "",
        call,
        want,
    }
}

const fn as_agent(actor: &'static str, call: Call, want: Want) -> Step {
    Step { actor, call, want }
}

const fn ok(file: &'static str) -> Want {
    Want::Done(Done { file, ..DONE })
}

const fn said_ok(file: &'static str, msgs: &'static [&'static str]) -> Want {
    Want::Done(Done {
        file,
        msgs: Some(msgs),
        ..DONE
    })
}

const fn no_op(file: &'static str, msgs: &'static [&'static str]) -> Want {
    Want::Done(Done {
        file,
        msgs: Some(msgs),
        no_op: true,
        ..DONE
    })
}

const fn refused(code: Code) -> Want {
    Want::Refused(Refused { code, ..REFUSED })
}

const fn cycle(message: &'static str, path: &'static [u32]) -> Want {
    Want::Refused(Refused {
        code: Code::Cycle,
        message: Some(message),
        path: Some(path),
        ..REFUSED
    })
}

const GONE1_NL: &str = "x 2026-09-28 A id:a\n";
const HELD2: &str = "A id:a started:2026-09-29T0800Z owner:agent2\n";
const HELD1_DEP: &str =
    "A id:a started:2026-09-29T0800Z owner:agent1\nB dep:a\n";

const DEP_ROWS: &[Case] = &[
    Case {
        name: "19.1 dep gives the prerequisite an id",
        todo: "A\nB\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", None),
            Want::Done(Done {
                file: "A id:aaaa\nB dep:aaaa\n",
                msgs: Some(&["1 is id:aaaa", "2 now waits for 1"]),
                changed: Some(&[1]),
                tasks: Some(&[2]),
                blocked: Some(true),
                ..DONE
            }),
        )],
    },
    Case {
        name: "19.2 dep keeps an existing id",
        todo: "A id:parser\nB\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", None),
            Want::Done(Done {
                file: "A id:parser\nB dep:parser\n",
                msgs: Some(&["2 now waits for 1"]),
                changed: Some(&[]),
                ..DONE
            }),
        )],
    },
    Case {
        name: "a prerequisite named twice counts once",
        todo: "A\nB\nC\n",
        done: None,
        steps: &[step(
            Call::Dep("3", "1,1", None),
            said_ok(
                "A id:aaaa\nB\nC dep:aaaa\n",
                &["1 is id:aaaa", "3 now waits for 1"],
            ),
        )],
    },
    Case {
        name: "19.3 dep again is a no-op",
        todo: "A id:parser\nB dep:parser\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", None),
            no_op("A id:parser\nB dep:parser\n", &["2 already waits for 1"]),
        )],
    },
    Case {
        name: "19.4 dep with a chosen id",
        todo: "A\nB\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", Some("parser")),
            said_ok(
                "A id:parser\nB dep:parser\n",
                &["1 is id:parser", "2 now waits for 1"],
            ),
        )],
    },
    Case {
        name: "19.5 dep with a chosen id that differs",
        todo: "A id:x1\nB\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", Some("parser")),
            refused(Code::WrongState),
        )],
    },
    Case {
        name: "19.6 dep for two waiting tasks",
        todo: "A\nB\nC\n",
        done: None,
        steps: &[step(
            Call::Dep("2,3", "1", None),
            said_ok(
                "A id:aaaa\nB dep:aaaa\nC dep:aaaa\n",
                &["1 is id:aaaa", "2 now waits for 1", "3 now waits for 1"],
            ),
        )],
    },
    Case {
        name: "19.7 dep on two prerequisites",
        todo: "A\nB\nC\n",
        done: None,
        steps: &[step(
            Call::Dep("3", "1,2", None),
            ok("A id:aaaa\nB id:bbbb\nC dep:aaaa dep:bbbb\n"),
        )],
    },
    Case {
        name: "19.8 dep on two with a chosen id",
        todo: "A\nB\nC\n",
        done: None,
        steps: &[step(
            Call::Dep("3", "1,2", Some("parser")),
            refused(Code::Usage),
        )],
    },
    Case {
        name: "19.9 dep that closes a loop",
        todo: "A id:a dep:b\nB id:b\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", None),
            cycle("2 → 1 → 2 would be a dependency cycle", &[2, 1, 2]),
        )],
    },
    Case {
        name: "19.10 dep on itself",
        todo: "A\n",
        done: None,
        steps: &[step(
            Call::Dep("1", "1", None),
            cycle("1 → 1 would be a dependency cycle", &[1, 1]),
        )],
    },
    Case {
        name: "19.11 dep that closes a longer loop",
        todo: "A id:a dep:b\nB id:b dep:c\nC id:c\n",
        done: None,
        steps: &[step(
            Call::Dep("3", "1", None),
            cycle("3 → 1 → 2 → 3 would be a dependency cycle", &[3, 1, 2, 3]),
        )],
    },
    Case {
        name: "19.12 an old cycle does not refuse",
        todo: "A id:a dep:b\nB id:b dep:a\nC\nD\n",
        done: None,
        steps: &[step(
            Call::Dep("4", "3", None),
            ok("A id:a dep:b\nB id:b dep:a\nC id:aaaa\nD dep:aaaa\n"),
        )],
    },
    Case {
        name: "a dep on a task that is already on a cycle",
        todo: "A id:a dep:b\nB id:b dep:a\nC id:c\n",
        done: None,
        steps: &[step(
            Call::Dep("1", "3", None),
            said_ok(
                "A id:a dep:b dep:c\nB id:b dep:a\nC id:c\n",
                &["1 now waits for 3"],
            ),
        )],
    },
    Case {
        name: "19.13 tag that closes a loop",
        todo: "A id:a dep:b\nB id:b\n",
        done: None,
        steps: &[step(Call::Tag("2", "dep:a"), refused(Code::Cycle))],
    },
    Case {
        name: "19.13 append that closes a loop",
        todo: "A id:a dep:b\nB id:b\n",
        done: None,
        steps: &[step(Call::Append("2", "dep:a"), refused(Code::Cycle))],
    },
    Case {
        name: "19.13 replace that closes a loop",
        todo: "A id:a dep:b\nB id:b\n",
        done: None,
        steps: &[step(Call::Replace("2", "B dep:a"), refused(Code::Cycle))],
    },
    Case {
        name: "19.13 add that closes a loop",
        todo: "A id:a dep:b\n",
        done: None,
        steps: &[step(
            Call::Add("B id:b dep:a"),
            cycle("2 → 1 → 2 would be a dependency cycle", &[2, 1, 2]),
        )],
    },
    Case {
        name: "19.13 prepend that closes a loop",
        todo: "A id:a dep:b\nB id:b\n",
        done: None,
        steps: &[step(Call::Prepend("2", "dep:a"), refused(Code::Cycle))],
    },
    Case {
        name: "19.15 a done prerequisite does not block",
        todo: "x 2026-09-28 A id:a\nB\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", None),
            Want::Done(Done {
                file: "x 2026-09-28 A id:a\nB dep:a\n",
                blocked: Some(false),
                ..DONE
            }),
        )],
    },
    Case {
        name: "19.16 a done-file prerequisite by id",
        todo: "B\n",
        done: Some(GONE1_NL),
        steps: &[step(Call::Dep("1", "id:a", None), ok("B dep:a\n"))],
    },
    Case {
        name: "19.17 a done-file prerequisite without an id",
        todo: "B\n",
        done: Some("x 2026-09-28 A\n"),
        steps: &[step(
            Call::Dep("1", "done:1", None),
            refused(Code::WrongState),
        )],
    },
    Case {
        name: "a done task without an id is given one as a prerequisite",
        todo: "x 2026-09-28 A\nB\n",
        done: None,
        steps: &[step(
            Call::Dep("2", "1", None),
            ok("x 2026-09-28 A id:aaaa\nB dep:aaaa\n"),
        )],
    },
    Case {
        name: "a done task waits like any other",
        todo: "x 2026-09-28 A\nB id:b\n",
        done: None,
        steps: &[step(
            Call::Dep("1", "2", None),
            ok("x 2026-09-28 A dep:b\nB id:b\n"),
        )],
    },
    Case {
        name: "an archived task cannot wait",
        todo: "B id:b\n",
        done: Some("x 2026-09-28 A\n"),
        steps: &[step(
            Call::Dep("done:1", "1", None),
            Want::Refused(Refused {
                code: Code::WrongState,
                hint: Some("done:1 is archived; use restore"),
                ..REFUSED
            }),
        )],
    },
    Case {
        name: "a waiting task that is its own prerequisite is a cycle",
        todo: "A\nB\n",
        done: None,
        steps: &[step(Call::Dep("1,2", "1", None), refused(Code::Cycle))],
    },
];

const UNDEP_ROWS: &[Case] = &[
    Case {
        name: "19.18 undep",
        todo: "A id:a\nB dep:a dep:c\n",
        done: None,
        steps: &[step(
            Call::Undep("2", "1"),
            said_ok("A id:a\nB dep:c\n", &["2 no longer waits for 1"]),
        )],
    },
    Case {
        name: "19.19 undep without an id",
        todo: "A\nB\n",
        done: None,
        steps: &[step(
            Call::Undep("2", "1"),
            no_op("A\nB\n", &["2 does not wait for 1"]),
        )],
    },
    Case {
        name: "19.19 undep without that dep",
        todo: "A id:a\nB\n",
        done: None,
        steps: &[step(
            Call::Undep("2", "1"),
            no_op("A id:a\nB\n", &["2 does not wait for 1"]),
        )],
    },
];

const DEL_ROWS: &[Case] = &[
    Case {
        name: "19.20 del blanks the line",
        todo: "A\nB\nC\n",
        done: None,
        steps: &[step(
            Call::Del("2", PLAIN),
            Want::Done(Done {
                file: "A\n\nC\n",
                msgs: Some(&["2 deleted"]),
                tasks: Some(&[]),
                before: Some(&[2]),
                ..DONE
            }),
        )],
    },
    Case {
        name: "19.21 del again",
        todo: "A\n\nC\n",
        done: None,
        steps: &[step(
            Call::Del("2", PLAIN),
            no_op("A\n\nC\n", &["2 already deleted"]),
        )],
    },
    Case {
        name: "19.22 del cascades, and a retry by id finds nothing",
        todo: "A id:a\nB dep:a\nC dep:a dep:x\n",
        done: None,
        steps: &[
            step(
                Call::Del("1", PLAIN),
                Want::Done(Done {
                    file: "\nB\nC dep:x\n",
                    msgs: Some(&["1 deleted; unblocked 2, 3"]),
                    changed: Some(&[2, 3]),
                    ..DONE
                }),
            ),
            step(Call::Del("id:a", PLAIN), refused(Code::NoTask)),
        ],
    },
    Case {
        name: "19.24 del keeps dependents when the id survives",
        todo: "A id:a\nA2 id:a\nB dep:a\n",
        done: None,
        steps: &[step(
            Call::Del("1", PLAIN),
            Want::Done(Done {
                file: "\nA2 id:a\nB dep:a\n",
                msgs: Some(&["1 deleted"]),
                changed: Some(&[]),
                ..DONE
            }),
        )],
    },
    Case {
        name: "19.25 del keeps dependents when the done file has the id",
        todo: "A id:a\nB dep:a\n",
        done: Some(GONE1_NL),
        steps: &[step(Call::Del("1", PLAIN), ok("\nB dep:a\n"))],
    },
    Case {
        name: "19.26 del leaves done-file dependents",
        todo: "A id:a\n",
        done: Some("x 2026-09-28 B dep:a\n"),
        steps: &[step(Call::Del("1", PLAIN), ok("\n"))],
    },
    Case {
        name: "19.28 del of both ends",
        todo: "A id:a\nB dep:a\n",
        done: None,
        steps: &[step(
            Call::Del("1,2", PLAIN),
            said_ok("\n\n", &["1 deleted", "2 deleted"]),
        )],
    },
    Case {
        name: "19.29 del of a done task",
        todo: "x 2026-09-28 A\n",
        done: None,
        steps: &[step(Call::Del("1", PLAIN), ok("\n"))],
    },
    Case {
        name: "19.30 del of an archived task",
        todo: "A\n",
        done: Some("x 2026-09-28 B\n"),
        steps: &[step(
            Call::Del("done:1", PLAIN),
            Want::Refused(Refused {
                code: Code::WrongState,
                hint: Some("done:1 is archived; use restore"),
                ..REFUSED
            }),
        )],
    },
    Case {
        name: "19.31 del with a missing reference",
        todo: "A\nB\n",
        done: None,
        steps: &[step(Call::Del("1,9", PLAIN), refused(Code::NoTask))],
    },
];

const CLAIM_ROWS: &[Case] = &[
    Case {
        name: "19.32 agent dep on a claimed task",
        todo: "A id:a started:2026-09-29T0800Z owner:agent2\nB id:b\n",
        done: None,
        steps: &[as_agent(
            "agent1",
            Call::Dep("id:a", "id:b", None),
            said_ok(
                "A id:a started:2026-09-29T0800Z owner:agent2 dep:b\n\
                 B id:b\n",
                &["a now waits for b", "a is claimed by agent2"],
            ),
        )],
    },
    Case {
        name: "19.33 agent del of a claimed task",
        todo: HELD2,
        done: None,
        steps: &[
            as_agent(
                "agent1",
                Call::Del("id:a", PLAIN),
                refused(Code::Claimed),
            ),
            as_agent(
                "agent1",
                Call::Del("id:a", TAKEOVER),
                said_ok("\n", &["a deleted", "a taken over from agent2"]),
            ),
        ],
    },
    Case {
        name: "19.34 human del of an agent's task",
        todo: HELD1_DEP,
        done: None,
        steps: &[step(
            Call::Del("1", PLAIN),
            said_ok(
                "\nB\n",
                &["1 deleted; unblocked 2", "1 was claimed by agent1"],
            ),
        )],
    },
    Case {
        name: "an agent's del frees a dependent another agent holds",
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n\
               B dep:a started:2026-09-29T0800Z owner:agent2\n",
        done: None,
        steps: &[as_agent(
            "agent1",
            Call::Del("id:a", PLAIN),
            Want::Done(Done {
                file: "\nB started:2026-09-29T0800Z owner:agent2\n",
                msgs: Some(&["a deleted; unblocked 2"]),
                changed: Some(&[2]),
                ..DONE
            }),
        )],
    },
    Case {
        name: "a dependent whose stamp would change keeps its dep",
        todo: "A id:a\ndep:a x marks spot\n",
        done: None,
        steps: &[step(
            Call::Del("1", PLAIN),
            Want::Done(Done {
                file: "\ndep:a x marks spot\n",
                msgs: Some(&["1 deleted"]),
                changed: Some(&[]),
                ..DONE
            }),
        )],
    },
    Case {
        name: "untag of an id by its key cascades",
        todo: "A id:a\nB dep:a\nC dep:a\n",
        done: None,
        steps: &[step(
            Call::Untag("1", &["id:"]),
            Want::Done(Done {
                file: "A\nB\nC\n",
                msgs: Some(&["1 untagged id:; unblocked 2, 3"]),
                changed: Some(&[2, 3]),
                ..DONE
            }),
        )],
    },
    Case {
        name: "untag of an id that is also on the done file keeps the dep",
        todo: "A id:a\nB dep:a\n",
        done: Some(GONE1_NL),
        steps: &[step(
            Call::Untag("1", &["id:a"]),
            said_ok("A\nB dep:a\n", &["1 untagged id:a"]),
        )],
    },
    Case {
        name: "untag of an id cascades to a dependent that is not first",
        todo: "X\nA id:a\nB dep:a dep:z\n",
        done: None,
        steps: &[step(
            Call::Untag("2", &["id:a"]),
            said_ok("X\nA\nB dep:z\n", &["2 untagged id:a; unblocked 3"]),
        )],
    },
    Case {
        name: "only the removed id says what it unblocked",
        todo: "X\nA id:a +x\nB dep:a\n",
        done: None,
        steps: &[step(
            Call::Untag("2", &["id:a", "+x"]),
            said_ok(
                "X\nA\nB\n",
                &["2 untagged id:a; unblocked 3", "2 untagged +x"],
            ),
        )],
    },
    Case {
        name: "each removed id says only what it unblocked",
        todo: "A id:b id:a\nB dep:b\n",
        done: None,
        steps: &[step(
            Call::Untag("1", &["id:a", "id:b"]),
            said_ok(
                "A\nB\n",
                &["1 untagged id:a", "1 untagged id:b; unblocked 2"],
            ),
        )],
    },
    Case {
        name: "19.27 untag of an id cascades",
        todo: "A id:a\nB dep:a\n",
        done: None,
        steps: &[step(
            Call::Untag("1", &["id:a"]),
            said_ok("A\nB\n", &["1 untagged id:a; unblocked 2"]),
        )],
    },
];

fn call(env: &Env<'_>, what: Call) -> Result<Outcome, Error> {
    let refs = parse_refs;
    match what {
        Call::Dep(waiting, on, chosen) => {
            dep(env, &refs(waiting)?, &refs(on)?, chosen)
        }
        Call::Undep(waiting, on) => undep(env, &refs(waiting)?, &refs(on)?),
        Call::Del(targets, claim) => del(env, &refs(targets)?, claim),
        Call::Tag(targets, text) => {
            tag(env, &refs(targets)?, &[text.to_owned()])
        }
        Call::Append(targets, text) => append(env, &refs(targets)?, text),
        Call::Replace(targets, text) => {
            replace(env, &refs(targets)?, text, PLAIN)
        }
        Call::Add(text) => add(
            env,
            &AddParams {
                texts: vec![text.to_owned()],
                ..AddParams::default()
            },
        ),
        Call::Prepend(targets, text) => prepend(env, &refs(targets)?, text),
        Call::Untag(targets, texts) => {
            untag(env, &refs(targets)?, &strings(texts))
        }
    }
}

fn nrs<'a>(tasks: impl Iterator<Item = &'a crate::list::Task>) -> Vec<u32> {
    tasks.map(|task| task.nr().get()).collect()
}

fn check_done(at: &str, out: &Outcome, want: &Done, file: &str) {
    assert_eq!(file, want.file, "{at}");
    if let Some(msgs) = want.msgs {
        assert_eq!(out.messages(), said(msgs), "{at}");
    }
    assert_eq!(out.no_op(), want.no_op, "{at}");
    assert!(!out.renumbered(), "{at}");
    check_lists(at, out, want);
}

fn check_lists(at: &str, out: &Outcome, want: &Done) {
    let changed = out.changed().iter().filter_map(|i| out.after().task(*i));
    let lists = [
        ("changed", want.changed, nrs(changed)),
        ("items", want.tasks, nrs(out.tasks_after())),
        ("before", want.before, nrs(out.tasks_before())),
    ];
    for (what, wanted, got) in lists {
        if let Some(wanted) = wanted {
            assert_eq!(got, wanted, "{at} {what}");
        }
    }
    if let Some(blocked) = want.blocked {
        let first = out.tasks_after().next().map(crate::list::Task::blocked);
        assert_eq!(first, Some(blocked), "{at} blocked");
    }
}

fn path_nrs(err: &Error) -> Option<Vec<u32>> {
    let Some(Field::Refs(refs)) = err.field("path") else {
        return None;
    };
    Some(refs.iter().map(|r| r.nr.get()).collect())
}

fn check_refused(at: &str, err: &Error, want: &Refused) {
    let (code, message, hint) = parts(err);
    assert_eq!(code, want.code, "{at}: {err:?}");
    if let Some(hint_want) = want.hint {
        assert_eq!(hint, hint_want, "{at}");
    }
    if let Some(message_want) = want.message {
        assert_eq!(message, message_want, "{at}");
    }
    if let Some(path) = want.path {
        assert_eq!(path_nrs(err).as_deref(), Some(path), "{at} path");
        assert!(err.task().is_some(), "{at}");
    }
}

fn run_case(case: &Case) {
    let dir = folder(case.todo, case.done);
    let ids = Sequence::letters();
    for (n, step) in case.steps.iter().enumerate() {
        let mut env = dir.env();
        env.ids = &ids;
        env.actor = Actor::named(step.actor).expect("actor");
        let before = dir.todo();
        let at = format!("{} step {n}", case.name);
        match (call(&env, step.call), &step.want) {
            (Ok(out), Want::Done(want)) => {
                check_done(&at, &out, want, &dir.todo());
            }
            (Err(err), Want::Refused(want)) => {
                check_refused(&at, &err, want);
                assert_eq!(dir.todo(), before, "{at}: the file changed");
            }
            (Ok(out), Want::Refused(_)) => {
                panic!("{at}: succeeded: {:?}", out.messages())
            }
            (Err(err), Want::Done(_)) => panic!("{at}: failed: {err:?}"),
        }
        if let Some(done) = case.done {
            assert_eq!(dir.done(), done, "{at}: the done file changed");
        }
    }
}

#[test]
fn dep_rows() {
    DEP_ROWS.iter().for_each(run_case);
}

#[test]
fn undep_rows() {
    UNDEP_ROWS.iter().for_each(run_case);
}

#[test]
fn del_rows() {
    DEL_ROWS.iter().for_each(run_case);
}

#[test]
fn claim_and_cascade_rows() {
    CLAIM_ROWS.iter().for_each(run_case);
}

#[test]
fn the_cascade_keeps_line_endings_and_bytes_it_does_not_touch() {
    let dir = folder("", None);
    let todo = b"A id:a\r\nB dep:a caf\xe9\r\nC dep:z\xff\r\n";
    std::fs::write(&dir.found.todo, todo).expect("write");
    let out =
        del(&dir.env(), &parse_refs("1").expect("refs"), PLAIN).expect("del");
    let got = std::fs::read(&dir.found.todo).expect("read");
    assert_eq!(got, b"\r\nB caf\xe9\r\nC dep:z\xff\r\n");
    assert_eq!(out.messages(), said(&["1 deleted; unblocked 2"]));
}
