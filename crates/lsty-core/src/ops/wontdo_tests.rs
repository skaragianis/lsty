use super::{
    Actor, ClaimOpts, DoOpts, Env, Outcome, Toggle, append, del, del_term,
    edit, finish, parse_refs, prepare_edit, prepend, reopen, replace, restore,
    tag,
    test_util::{Dir, folder, parts, said, snapshot},
    untag,
};
use crate::errs::{Code, Result};

const STAMP: &str =
    "the text would start with a stamp-like word; reword it first";

#[derive(Clone, Copy)]
enum Op {
    Wontdo(Toggle),
    WontdoTakeover,
    Do,
    Del,
    Reopen,
    Restore,
    Tag,
    Untag,
    Append(&'static str),
    Prepend(&'static str),
    Replace(&'static str),
    DelTerm(&'static str),
    Edit(&'static str),
}

fn call(env: &Env<'_>, dir: &Dir, refs: &str, op: Op) -> Result<Outcome> {
    let targets = parse_refs(refs)?;
    let claim = ClaimOpts::default();
    let word = ["wontdo:true".to_owned()];
    let decide = |archive| DoOpts {
        archive,
        wontdo: true,
        ..DoOpts::default()
    };
    match op {
        Op::Wontdo(archive) => finish(env, &targets, decide(archive)),
        Op::WontdoTakeover => finish(
            env,
            &targets,
            DoOpts {
                claim: ClaimOpts { takeover: true },
                ..decide(Toggle::Config)
            },
        ),
        Op::Do => finish(env, &targets, DoOpts::default()),
        Op::Del => del(env, &targets, claim),
        Op::Reopen => reopen(env, &targets, claim),
        Op::Restore => restore(env, &targets, claim),
        Op::Tag => tag(env, &targets, &word),
        Op::Untag => untag(env, &targets, &word),
        Op::Append(text) => append(env, &targets, text),
        Op::Prepend(text) => prepend(env, &targets, text),
        Op::Replace(text) => replace(env, &targets, text, claim),
        Op::DelTerm(term) => del_term(env, &targets, term),
        Op::Edit(text) => {
            let snap = snapshot(dir, dir.todo().as_bytes(), b"");
            let target = targets.first().ok_or_else(super::tx::gone)?;
            let plan = prepare_edit(&snap, &env.actor, target, claim)?;
            edit(env, &plan, text.as_bytes(), claim)
        }
    }
}

/// One call. `todo` and `done` are what the files hold before it, `want`
/// and `want_done` after it.
struct Case {
    name: &'static str,
    actor: &'static str,
    todo: &'static str,
    done: &'static str,
    refs: &'static str,
    op: Op,
    auto_archive: bool,
    want: &'static str,
    want_done: &'static str,
    msgs: &'static [&'static str],
    /// The message and hint of the `wrong_state` the call ends in.
    refused: Option<(&'static str, &'static str)>,
}

const BASE: Case = Case {
    name: "",
    actor: "",
    todo: "",
    done: "",
    refs: "1",
    op: Op::Wontdo(Toggle::Config),
    auto_archive: false,
    want: "",
    want_done: "",
    msgs: &[],
    refused: None,
};

fn check(case: &Case) {
    let mut dir = folder(case.todo, Some(case.done));
    dir.config.auto_archive = case.auto_archive;
    let mut env = dir.env();
    env.actor = Actor::named(case.actor).expect("actor");
    let got = call(&env, &dir, case.refs, case.op);
    let name = case.name;
    if let Some((message, hint)) = case.refused {
        let err = got.expect_err(name);
        assert_eq!(parts(&err), (Code::WrongState, message, hint), "{name}");
        assert_eq!(dir.todo(), case.todo, "{name}");
        assert_eq!(dir.done(), case.done, "{name}");
        return;
    }
    let got = got.expect(name);
    assert_eq!(got.messages(), said(case.msgs), "{name}");
    assert_eq!(dir.todo(), case.want, "{name}");
    assert_eq!(dir.done(), case.want_done, "{name}");
}

const DECIDING: &[Case] = &[
    Case {
        name: "the mark, the date and the tag, and no ended: if never started",
        todo: "(A) 2026-09-20 A +w\nB\n",
        want: "x 2026-09-29 2026-09-20 A +w wontdo:true pri:A\nB\n",
        msgs: &["1 won't do: A"],
        ..BASE
    },
    Case {
        name: "a started task gets ended: and keeps started:",
        todo: "A started:2026-09-29T0800Z\n",
        want: "x 2026-09-29 A started:2026-09-29T0800Z wontdo:true \
               ended:2026-09-29T0900Z\n",
        msgs: &["1 won't do: A"],
        ..BASE
    },
    Case {
        name: "another actor's claim is dropped and named",
        todo: "A started:2026-09-29T0800Z owner:agent1\n",
        want: "x 2026-09-29 A started:2026-09-29T0800Z wontdo:true \
               ended:2026-09-29T0900Z\n",
        msgs: &["1 won't do: A", "1 was claimed by agent1"],
        ..BASE
    },
    Case {
        name: "a comma list",
        todo: "A\nB\nC\n",
        refs: "1,3",
        want: "x 2026-09-29 A wontdo:true\nB\nx 2026-09-29 C wontdo:true\n",
        msgs: &["1 won't do: A", "3 won't do: C"],
        ..BASE
    },
    Case {
        name: "a blocked task is a fair thing to decide against",
        todo: "A id:a\nB dep:a\n",
        refs: "2",
        want: "A id:a\nx 2026-09-29 B dep:a wontdo:true\n",
        msgs: &["2 won't do: B"],
        ..BASE
    },
    Case {
        name: "nothing is said about the dependents it unblocks",
        todo: "A id:a\nB dep:a\n",
        want: "x 2026-09-29 A id:a wontdo:true\nB dep:a\n",
        msgs: &["1 won't do: A"],
        ..BASE
    },
    Case {
        name: "the series ends",
        todo: "A rec:1w due:2026-09-29\nB\n",
        want: "x 2026-09-29 A rec:1w due:2026-09-29 wontdo:true\nB\n",
        msgs: &["1 won't do: A", "1 has rec:; the series ended"],
        ..BASE
    },
    Case {
        name: "a bad rec: says what it always said",
        todo: "A rec:soon\n",
        want: "x 2026-09-29 A rec:soon wontdo:true\n",
        msgs: &[
            "1 won't do: A",
            "1 has a bad rec: value; no next occurrence",
        ],
        ..BASE
    },
    Case {
        name: "an open line that has the tag gets no second one",
        todo: "A wontdo:true\n",
        want: "x 2026-09-29 A wontdo:true\n",
        msgs: &["1 won't do: A"],
        ..BASE
    },
    Case {
        name: "do keeps the tag on the line",
        op: Op::Do,
        todo: "A wontdo:true\n",
        want: "x 2026-09-29 A wontdo:true\n",
        msgs: &["1 done: A"],
        ..BASE
    },
    Case {
        name: "do on a line with rec: and the tag ends the series",
        op: Op::Do,
        todo: "A rec:1w wontdo:true\nB\n",
        want: "x 2026-09-29 A rec:1w wontdo:true\nB\n",
        msgs: &["1 done: A", "1 has rec:; the series ended"],
        ..BASE
    },
    Case {
        name: "do of a line without the tag is as it was",
        op: Op::Do,
        todo: "A wontdo:false rec:1w due:2026-09-29\n",
        want: "x 2026-09-29 A wontdo:false rec:1w due:2026-09-29\n\
               2026-09-29 A wontdo:false rec:1w due:2026-10-06 id:aaaa\n",
        msgs: &["1 done: A", "2 next: A (due 2026-10-06)"],
        ..BASE
    },
];

const ALREADY: &[Case] = &[
    Case {
        name: "again is a no-op",
        todo: "x 2026-09-28 A wontdo:true\n",
        want: "x 2026-09-28 A wontdo:true\n",
        msgs: &["1 is already won't do"],
        ..BASE
    },
    Case {
        name: "a plain done task of the todo file names reopen",
        todo: "x\n\nx 2026-09-28 A\n",
        refs: "3",
        want: "x\n\nx 2026-09-28 A\n",
        msgs: &["3 is already done; use lsty reopen 3 first"],
        ..BASE
    },
    Case {
        name: "a plain done task of the done file names restore",
        done: "\n\nx 2026-09-28 A\n",
        refs: "done:3",
        want_done: "\n\nx 2026-09-28 A\n",
        msgs: &["done:3 is already done; use lsty restore done:3 first"],
        ..BASE
    },
    Case {
        name: "the route takes the reference as given",
        todo: "x 2026-09-28 A id:a\n",
        refs: "id:a",
        want: "x 2026-09-28 A id:a\n",
        msgs: &["a is already done; use lsty reopen id:a first"],
        ..BASE
    },
    Case {
        name: "the route in the done file with an id",
        done: "x 2026-09-28 A id:a\n",
        refs: "id:a",
        want_done: "x 2026-09-28 A id:a\n",
        msgs: &["a is already done; use lsty restore id:a first"],
        ..BASE
    },
    Case {
        name: "do says what it always said",
        op: Op::Do,
        todo: "x 2026-09-28 A wontdo:true\n",
        want: "x 2026-09-28 A wontdo:true\n",
        msgs: &["1 is already done"],
        ..BASE
    },
];

const DONE_FILE: &[Case] = &[
    Case {
        name: "an open line of the done file is completed",
        done: "A\n",
        refs: "done:1",
        want_done: "x 2026-09-29 A wontdo:true\n",
        msgs: &["done:1 won't do: A"],
        ..BASE
    },
    Case {
        name: "an open line of the done file with rec: makes no next",
        todo: "B\n",
        done: "A rec:1w\n",
        refs: "1,done:1",
        auto_archive: true,
        want: "\n",
        want_done: "x 2026-09-29 A rec:1w wontdo:true\n\
                    x 2026-09-29 B wontdo:true\n",
        msgs: &[
            "1 won't do: B",
            "archived to done.txt as done:2",
            "done:1 won't do: A",
            "done:1 has rec:; the series ended",
        ],
        ..BASE
    },
    Case {
        name: "do of a line of the done file with rec: and the tag makes no next",
        op: Op::Do,
        todo: "B\n",
        done: "A rec:1w wontdo:true\n",
        refs: "1,done:1",
        auto_archive: true,
        want: "\n",
        want_done: "x 2026-09-29 A rec:1w wontdo:true\nx 2026-09-29 B\n",
        msgs: &[
            "1 done: B",
            "archived to done.txt as done:2",
            "done:1 done: A",
            "done:1 has rec:; the series ended",
        ],
        ..BASE
    },
    Case {
        name: "auto_archive moves the line",
        todo: "A\nB\n",
        auto_archive: true,
        want: "\nB\n",
        want_done: "x 2026-09-29 A wontdo:true\n",
        msgs: &["1 won't do: A", "archived to done.txt as done:1"],
        ..BASE
    },
    Case {
        name: "--no-archive beats auto_archive",
        todo: "A\n",
        op: Op::Wontdo(Toggle::Off),
        auto_archive: true,
        want: "x 2026-09-29 A wontdo:true\n",
        msgs: &["1 won't do: A"],
        ..BASE
    },
    Case {
        name: "--archive beats no auto_archive",
        todo: "A\n",
        op: Op::Wontdo(Toggle::On),
        want: "\n",
        want_done: "x 2026-09-29 A wontdo:true\n",
        msgs: &["1 won't do: A", "archived to done.txt as done:1"],
        ..BASE
    },
];

const REOPENING: &[Case] = &[
    Case {
        name: "reopen removes the tag and says nothing of a next occurrence",
        op: Op::Reopen,
        todo: "x 2026-09-28 A rec:1w wontdo:true pri:A\n",
        want: "(A) A rec:1w\n",
        msgs: &["1 reopened"],
        ..BASE
    },
    Case {
        name: "reopen of a plain done task with rec: still says it stays",
        op: Op::Reopen,
        todo: "x 2026-09-28 A rec:1w\n",
        want: "A rec:1w\n",
        msgs: &[
            "1 reopened",
            "1 has rec:; its next occurrence stays in the list",
        ],
        ..BASE
    },
    Case {
        name: "restore removes the tag",
        op: Op::Restore,
        done: "x 2026-09-28 A wontdo:true\n",
        refs: "done:1",
        want: "A\n",
        want_done: "\n",
        msgs: &["done:1 restored as 1"],
        ..BASE
    },
    Case {
        name: "restore of a line without the mark removes the tag",
        op: Op::Restore,
        done: "A wontdo:true\n",
        refs: "done:1",
        want: "A\n",
        want_done: "\n",
        msgs: &["done:1 restored as 1"],
        ..BASE
    },
    Case {
        name: "a human takes back an agent's decision on a task never started",
        op: Op::Reopen,
        todo: "x 2026-09-28 A owner:agent1 wontdo:true\n",
        want: "A\n",
        msgs: &["1 reopened"],
        ..BASE
    },
    Case {
        name: "reopen keeps an owner that holds a stamp off",
        op: Op::Reopen,
        todo: "x 2026-09-28 owner:bob wontdo:true 2026-01-01 A\n",
        want: "owner:bob 2026-01-01 A\n",
        msgs: &["1 reopened"],
        ..BASE
    },
    Case {
        name: "reopen refuses to make a stamp",
        op: Op::Reopen,
        todo: "x 2026-09-28 wontdo:true 2026-01-01 A\n",
        refused: Some(("1: a text edit can't change a stamp", STAMP)),
        ..BASE
    },
    Case {
        name: "restore refuses to make a stamp",
        op: Op::Restore,
        done: "x 2026-09-28 wontdo:true 2026-01-01 A\n",
        refs: "done:1",
        refused: Some(("done:1: a text edit can't change a stamp", STAMP)),
        ..BASE
    },
    Case {
        name: "untag gives the same refusal",
        op: Op::Untag,
        todo: "wontdo:true 2026-01-01 A\n",
        refused: Some(("1: a text edit can't change a stamp", STAMP)),
        ..BASE
    },
    Case {
        name: "a line of the tag alone would be empty",
        op: Op::Reopen,
        todo: "x 2026-09-28 wontdo:true\n",
        refused: Some(("1 would be empty", "use lsty del 1")),
        ..BASE
    },
];

const AGENT: &str = "agent1";

/// An agent's call on the task `a`.
const fn agent_row(
    name: &'static str,
    op: Op,
    todo: &'static str,
    want: &'static str,
    msgs: &'static [&'static str],
) -> Case {
    Case {
        name,
        actor: AGENT,
        op,
        todo,
        refs: "id:a",
        want,
        msgs,
        ..BASE
    }
}

const TAGGED: &str = "x 2026-09-28 A b wontdo:true id:a\n";
const UNTAGGED: &str = "A id:a\n";
const DECIDED: &str = "x 2026-09-29 A id:a owner:agent1 wontdo:true\n";
const OTHERS: &str = "A id:a started:2026-09-29T0800Z owner:agent2\n";

const AGENTS: &[Case] = &[
    Case {
        name: "an agent decides a task it never started",
        actor: AGENT,
        todo: UNTAGGED,
        refs: "id:a",
        want: "x 2026-09-29 A id:a owner:agent1 wontdo:true\n",
        msgs: &["a won't do: A"],
        ..BASE
    },
    Case {
        name: "an agent decides a blocked task",
        actor: AGENT,
        todo: "A id:a dep:b\nB id:b\n",
        refs: "id:a",
        want: "x 2026-09-29 A id:a dep:b owner:agent1 wontdo:true\nB id:b\n",
        msgs: &["a won't do: A"],
        ..BASE
    },
    Case {
        name: "an agent decides a task it holds",
        actor: AGENT,
        todo: "A id:a started:2026-09-29T0800Z owner:agent1\n",
        refs: "id:a",
        want: "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1 \
               wontdo:true ended:2026-09-29T0900Z\n",
        msgs: &["a won't do: A"],
        ..BASE
    },
    Case {
        name: "an agent takes another's claim over to decide",
        actor: AGENT,
        op: Op::WontdoTakeover,
        todo: OTHERS,
        refs: "id:a",
        want: "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1 \
               wontdo:true ended:2026-09-29T0900Z\n",
        msgs: &["a won't do: A", "a taken over from agent2"],
        ..BASE
    },
    Case {
        name: "an agent's retry is a no-op",
        actor: AGENT,
        todo: "x 2026-09-28 A wontdo:true id:a\n",
        refs: "id:a",
        want: "x 2026-09-28 A wontdo:true id:a\n",
        msgs: &["a is already won't do"],
        ..BASE
    },
    agent_row(
        "tag adds it",
        Op::Tag,
        UNTAGGED,
        "A id:a wontdo:true\n",
        &["a tagged wontdo:true"],
    ),
    agent_row(
        "untag removes it",
        Op::Untag,
        TAGGED,
        "x 2026-09-28 A b id:a\n",
        &["a untagged wontdo:true"],
    ),
    agent_row(
        "replace loses it",
        Op::Replace("B"),
        TAGGED,
        "x 2026-09-28 B id:a\n",
        &["a x 2026-09-28 B id:a"],
    ),
    agent_row(
        "del TERM removes it",
        Op::DelTerm("wontdo:true"),
        TAGGED,
        "x 2026-09-28 A b id:a\n",
        &["a x 2026-09-28 A b id:a"],
    ),
    agent_row(
        "edit removes it",
        Op::Edit("x 2026-09-28 A b id:a"),
        TAGGED,
        "x 2026-09-28 A b id:a\n",
        &["a x 2026-09-28 A b id:a"],
    ),
    agent_row(
        "append adds it",
        Op::Append("wontdo:true"),
        UNTAGGED,
        "A wontdo:true id:a\n",
        &["a A wontdo:true id:a"],
    ),
    agent_row(
        "prepend adds it",
        Op::Prepend("wontdo:true"),
        UNTAGGED,
        "wontdo:true A id:a\n",
        &["a wontdo:true A id:a"],
    ),
    agent_row(
        "edit adds it",
        Op::Edit("A id:a wontdo:true"),
        UNTAGGED,
        "A id:a wontdo:true\n",
        &["a A id:a wontdo:true"],
    ),
    Case {
        name: "the tag in backticks is only text",
        actor: AGENT,
        op: Op::Append("`wontdo:true`"),
        todo: UNTAGGED,
        refs: "id:a",
        want: "A `wontdo:true` id:a\n",
        msgs: &["a A `wontdo:true` id:a"],
        ..BASE
    },
    Case {
        name: "an edit that keeps it goes through",
        actor: AGENT,
        op: Op::Append("c"),
        todo: "x 2026-09-28 A wontdo:true id:a\n",
        refs: "id:a",
        want: "x 2026-09-28 A wontdo:true c id:a\n",
        msgs: &["a x 2026-09-28 A wontdo:true c id:a"],
        ..BASE
    },
    agent_row(
        "an agent deletes a won't do task",
        Op::Del,
        TAGGED,
        "\n",
        &["a deleted"],
    ),
    Case {
        name: "an agent reopens a won't do task",
        actor: AGENT,
        op: Op::Reopen,
        todo: "x 2026-09-28 A wontdo:true id:a\n",
        refs: "id:a",
        want: "A id:a\n",
        msgs: &["a reopened"],
        ..BASE
    },
    Case {
        name: "an agent restores a won't do task",
        actor: AGENT,
        op: Op::Restore,
        done: "x 2026-09-28 A wontdo:true id:a\n",
        refs: "id:a",
        want: "A id:a\n",
        want_done: "\n",
        msgs: &["a restored as 1"],
        ..BASE
    },
    agent_row(
        "reopen takes off the owner of a task never started",
        Op::Reopen,
        DECIDED,
        "A id:a\n",
        &["a reopened"],
    ),
    Case {
        name: "restore takes off the owner of a task never started",
        actor: AGENT,
        op: Op::Restore,
        done: DECIDED,
        refs: "id:a",
        want: "A id:a\n",
        want_done: "\n",
        msgs: &["a restored as 1"],
        ..BASE
    },
    agent_row(
        "reopen keeps the claim of a started task",
        Op::Reopen,
        "x 2026-09-29 A id:a started:2026-09-29T0800Z owner:agent1 \
         wontdo:true ended:2026-09-29T0900Z\n",
        "A id:a started:2026-09-29T0800Z owner:agent1\n",
        &["a reopened; back to agent1"],
    ),
    agent_row(
        "reopen of a plain done task keeps its owner",
        Op::Reopen,
        "x 2026-09-29 A id:a owner:bob\n",
        "A id:a owner:bob\n",
        &["a reopened"],
    ),
    Case {
        name: "an agent reopens a plain done task",
        actor: AGENT,
        op: Op::Reopen,
        todo: "x 2026-09-28 A id:a\n",
        refs: "id:a",
        want: "A id:a\n",
        msgs: &["a reopened"],
        ..BASE
    },
    Case {
        name: "an agent restores a plain done task",
        actor: AGENT,
        op: Op::Restore,
        done: "x 2026-09-28 A id:a\n",
        refs: "id:a",
        want: "A id:a\n",
        want_done: "\n",
        msgs: &["a restored as 1"],
        ..BASE
    },
];

#[test]
fn deciding_writes_the_tag_in_one_write() {
    DECIDING.iter().for_each(check);
}

#[test]
fn a_task_already_done_is_a_no_op_that_names_the_route() {
    ALREADY.iter().for_each(check);
}

#[test]
fn archiving_goes_as_for_do() {
    DONE_FILE.iter().for_each(check);
}

#[test]
fn reopening_removes_the_tag() {
    REOPENING.iter().for_each(check);
}

#[test]
fn an_agent_makes_and_unmakes_the_decision() {
    AGENTS.iter().for_each(check);
}

#[test]
fn an_agent_on_another_actors_claim_needs_takeover() {
    let dir = folder(OTHERS, None);
    let mut env = dir.env();
    env.actor = Actor::named(AGENT).expect("actor");
    let err = call(&env, &dir, "id:a", Op::Wontdo(Toggle::Config))
        .expect_err("claimed");
    assert_eq!(err.code(), Code::Claimed);
    assert_eq!(dir.todo(), OTHERS);
}

#[test]
fn the_tag_on_an_open_line_changes_nothing_else() {
    let dir = folder("A\n", None);
    let targets = parse_refs("1").expect("refs");
    let word = ["wontdo:true".to_owned()];
    tag(&dir.env(), &targets, &word).expect("tag");
    assert_eq!(dir.todo(), "A wontdo:true\n");
    let got =
        reopen(&dir.env(), &targets, ClaimOpts::default()).expect("reopen");
    assert_eq!(got.messages(), said(&["1 is already open"]));
    assert_eq!(dir.todo(), "A wontdo:true\n");
}
