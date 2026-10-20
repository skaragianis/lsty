use std::fs;

use super::{
    Actor, ClaimOpts, EditPlan, Target, edit, joined_lines, prepare_edit,
    test_util::{Dir, folder, parts, said},
};
use crate::{
    errs::{Code, Source},
    list::{Params, load},
    lock::Cancel,
};

/// Name, file when picked, file when saved, typed text, and the result:
/// the file and the messages, or the code of the refusal.
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Result<(&'static str, &'static str), Code>,
);

const ROWS: &[Row] = &[
    (
        "edits the task",
        "Call client\nBuy\n",
        "Call client\nBuy\n",
        "Ring client",
        Ok(("Ring client\nBuy\n", "1 Ring client")),
    ),
    (
        "27.17 follows an insert above",
        "Call client\nBuy\n",
        "New\nCall client\nBuy\n",
        "Ring client",
        Ok(("New\nRing client\nBuy\n", "1 Ring client")),
    ),
    (
        "27.16 changed meanwhile",
        "Call client\nBuy\n",
        "Phone client\nBuy\n",
        "Ring client",
        Err(Code::EditConflict),
    ),
    (
        "27.18 deleted meanwhile",
        "Call client\nBuy\n",
        "Buy\n",
        "Ring client",
        Err(Code::EditConflict),
    ),
    (
        "27.19 stamp",
        "Call client\n",
        "Call client\n",
        "x 2026-09-29 Call client",
        Err(Code::WrongState),
    ),
    (
        "a date changed",
        "2026-09-20 Call client\n",
        "2026-09-20 Call client\n",
        "2026-09-25 Call client",
        Ok(("2026-09-25 Call client\n", "1 2026-09-25 Call client")),
    ),
    (
        "a date deleted",
        "2026-09-20 Call client\n",
        "2026-09-20 Call client\n",
        "Call client",
        Ok(("Call client\n", "1 Call client")),
    ),
    (
        "a date made into text",
        "2026-09-20 Call client\n",
        "2026-09-20 Call client\n",
        "2026-09-2x Call client",
        Ok(("2026-09-2x Call client\n", "1 2026-09-2x Call client")),
    ),
    (
        "a completion date changed",
        "x 2026-09-28 2026-09-20 Call client\n",
        "x 2026-09-28 2026-09-20 Call client\n",
        "x 2026-09-27 2026-09-20 Call client",
        Ok((
            "x 2026-09-27 2026-09-20 Call client\n",
            "1 x 2026-09-27 2026-09-20 Call client",
        )),
    ),
    (
        "27.20 id changed",
        "Call id:k3f9\n",
        "Call id:k3f9\n",
        "Call id:zzzz",
        Err(Code::WrongState),
    ),
    (
        "27.21 relative dates",
        "Call client\n",
        "Call client\n",
        "Call client due:fri",
        Ok((
            "Call client due:2026-10-02\n",
            "1 Call client due:2026-10-02",
        )),
    ),
    (
        "same after expansion",
        "Call due:2026-10-02\n",
        "Call due:2026-10-02\n",
        "Call due:fri",
        Ok(("Call due:2026-10-02\n", "1 unchanged")),
    ),
    (
        "a line cleared meanwhile",
        "Call client\nBuy\n",
        "\nBuy\n",
        "Ring client",
        Err(Code::EditConflict),
    ),
];

fn plan_for(
    dir: &Dir,
    actor: &Actor,
    target: &Target,
    opts: ClaimOpts,
) -> crate::errs::Result<EditPlan> {
    let snap = load(
        &dir.found,
        dir.config.lock_timeout,
        &Cancel::new(),
        &Params::new(dir.clock.clone(), &dir.config),
    )?;
    prepare_edit(&snap, actor, target, opts)
}

#[test]
fn follows_the_task_through_the_editor_session() {
    for (name, picked, saved, typed, want) in ROWS {
        let dir = folder(picked, None);
        let target = Target::todo(1);
        let plan =
            plan_for(&dir, &Actor::Human, &target, ClaimOpts::default())
                .unwrap();
        fs::write(&dir.found.todo, saved).unwrap();
        let got =
            edit(&dir.env(), &plan, typed.as_bytes(), ClaimOpts::default());
        match (got, want) {
            (Ok(outcome), Ok((file, message))) => {
                assert_eq!(dir.todo(), *file, "{name}");
                assert_eq!(outcome.messages(), said(&[message]), "{name}");
            }
            (Err(err), Err(code)) => {
                assert_eq!(err.code(), *code, "{name}: {}", err.message());
                assert_eq!(dir.todo(), *saved, "{name}");
            }
            (Ok(_), Err(code)) => panic!("{name}: wanted {code:?}"),
            (Err(err), Ok(_)) => panic!("{name}: {}", err.message()),
        }
    }
}

#[test]
fn a_conflict_names_the_task() {
    let dir = folder("Call client\nBuy\n", None);
    let plan =
        plan_for(&dir, &Actor::Human, &Target::todo(1), ClaimOpts::default())
            .unwrap();
    fs::write(&dir.found.todo, "Phone client\nBuy\n").unwrap();
    let err =
        edit(&dir.env(), &plan, b"Ring", ClaimOpts::default()).unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::EditConflict,
            "1 changed while it was being edited",
            ""
        )
    );
}

const HELD: &str =
    "Call client id:k3f9 started:2026-09-29T0800Z owner:agent2\n";

#[test]
fn the_claim_guard_runs_before_anyone_types() {
    let dir = folder(HELD, None);
    let agent = Actor::named("agent1").unwrap();
    let target = Target::by_id(crate::todotxt::Id::new("k3f9").unwrap());
    let err =
        plan_for(&dir, &agent, &target, ClaimOpts::default()).unwrap_err();
    assert_eq!(err.code(), Code::Claimed);
    let takeover = ClaimOpts { takeover: true };
    let plan = plan_for(&dir, &agent, &target, takeover).unwrap();
    assert_eq!(plan.raw(), HELD.trim_end().as_bytes());
    assert_eq!(dir.todo(), HELD);
}

#[test]
fn an_edit_under_a_held_claim_needs_takeover_too() {
    let dir = folder(HELD, None);
    let agent = Actor::named("agent1").unwrap();
    let takeover = ClaimOpts { takeover: true };
    let target = Target::todo(1);
    let plan = plan_for(&dir, &agent, &target, takeover).unwrap();
    let mut env = dir.env();
    env.actor = agent;
    let typed = HELD.replace("Call", "Ring");
    let typed = typed.trim_end().as_bytes();
    let err = edit(&env, &plan, typed, ClaimOpts::default()).unwrap_err();
    assert_eq!(err.code(), Code::Claimed);
    assert_eq!(dir.todo(), HELD);
    let out = edit(&env, &plan, typed, takeover);
    assert!(
        out.is_ok(),
        "{:?}",
        out.err().map(|e| e.message().to_owned())
    );
}

#[test]
fn an_archived_task_cannot_be_edited() {
    let dir = folder("Open\n", Some("x 2026-09-01 Old\n"));
    let err = plan_for(
        &dir,
        &Actor::Human,
        &Target::at(Source::Done, 1).archived(),
        ClaimOpts::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), Code::WrongState);
}

#[test]
fn a_missing_task_is_no_task() {
    let dir = folder("Open\n", None);
    let err =
        plan_for(&dir, &Actor::Human, &Target::todo(5), ClaimOpts::default())
            .unwrap_err();
    assert_eq!(err.code(), Code::NoTask);
}

/// Edited file and the text it gives.
type Joined = (&'static [u8], &'static [u8]);

const JOINED: &[Joined] = &[
    (b"Ring\nclient\n", b"Ring client"),
    (b"  Ring  \r\n\n  client  ", b"Ring client"),
    (b"\n\n", b""),
    (b"", b""),
    (b"caf\xe9 x\n", b"caf\xe9 x"),
    (b"one line", b"one line"),
];

#[test]
fn lines_are_trimmed_and_joined_with_spaces() {
    for (data, want) in JOINED {
        assert_eq!(joined_lines(data), *want, "{data:?}");
    }
}

#[test]
fn bytes_that_are_not_text_survive() {
    let dir = folder(b"Call caf\xe9\nBuy\n", None);
    let plan =
        plan_for(&dir, &Actor::Human, &Target::todo(1), ClaimOpts::default())
            .unwrap();
    edit(&dir.env(), &plan, b"Ring caf\xe9", ClaimOpts::default()).unwrap();
    assert_eq!(fs::read(&dir.found.todo).unwrap(), b"Ring caf\xe9\nBuy\n");
}

#[test]
fn the_plan_carries_the_line_and_the_name_the_target_used() {
    let dir = folder("Call client id:k3f9\nBuy\n", None);
    let by_number =
        plan_for(&dir, &Actor::Human, &Target::todo(2), ClaimOpts::default())
            .unwrap();
    assert_eq!(by_number.raw(), b"Buy");
    assert_eq!(by_number.handle(), "2");
    let target = super::parse_one("edit", "id:k3f9").unwrap();
    let by_id =
        plan_for(&dir, &Actor::Human, &target, ClaimOpts::default()).unwrap();
    assert_eq!(by_id.raw(), b"Call client id:k3f9");
    assert_eq!(by_id.handle(), "k3f9");
}

#[test]
fn an_agent_edit_keeps_the_id_and_brings_none() {
    let dir = folder("Call id:k3f9\nBuy\n", None);
    let mut env = dir.env();
    env.actor = Actor::named("agent1").unwrap();
    let opts = ClaimOpts::default();
    let plan = |nr| plan_for(&dir, &env.actor, &Target::todo(nr), opts);
    let err = edit(&env, &plan(2).unwrap(), b"Buy id:b1", opts).unwrap_err();
    assert_eq!(err.text("field"), Some("id"), "{}", err.message());
    edit(&env, &plan(1).unwrap(), b"Ring id:k3f9", opts).unwrap();
    assert_eq!(dir.todo(), "Ring id:k3f9\nBuy\n");
}
