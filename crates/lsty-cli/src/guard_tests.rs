//! Identity and `--expect` through the whole command line.

use std::fs;

use super::run_tests::Fake;

const LIST: &str = "Call client +work\nAdd CLI ls id:k3f9\nWrite docs\n";

fn fake() -> Fake {
    Fake::new()
        .file("todo.txt", LIST)
        .var("TODO_FILE", "todo.txt")
}

fn todo(fake: &Fake) -> String {
    fs::read_to_string(fake.path().join("todo.txt")).unwrap()
}

/// The agent's command line, and the stderr it earns.
const AGENT_NUMBERS: &[(&[&str], &str)] = &[
    (
        &["pri", "3", "A"],
        "lsty: agents name items by id\n\
         hint: use id: \u{2014} lsty id 3 --expect \"Call client\"\n",
    ),
    (
        &["pri", "id:k3f9", "A", "2", "B"],
        "lsty: agents name items by id\n\
         hint: use id: \u{2014} lsty id 2 --expect \"Call client\"\n",
    ),
    (
        &["pri", "2", "A", "--expect", "Add CLI ls"],
        "lsty: agents name items by id\n\
         hint: use id: \u{2014} lsty id 2 --expect \"Call client\"\n",
    ),
    (
        &["id", "1"],
        "lsty: agents name items by id\n\
         hint: use id: \u{2014} lsty id 1 --expect \"Call client\"\n",
    ),
    (
        &["id", "1,2", "--expect", "Call client"],
        "lsty: 1 --expect for 2 NR references\n\
         hint: usage: lsty id REFS [ID]\n",
    ),
];

#[test]
fn an_agent_names_items_by_id() {
    for (args, err) in AGENT_NUMBERS {
        let fake = fake().var("LSTY_OWNER", "agent1");
        let mut line = vec!["--no-config"];
        line.extend_from_slice(args);
        let ran = fake.run(&line);
        assert_eq!((ran.code, ran.err.as_str()), (2, *err), "{args:?}");
        assert_eq!(todo(&fake), LIST, "{args:?}");
    }
}

#[test]
fn an_agent_may_guard_each_number_of_id() {
    let fake = fake().var("LSTY_OWNER", "agent1");
    let ran = fake.run(&[
        "id",
        "1,3",
        "--expect",
        "Call client",
        "--expect",
        "Write docs",
        "--no-config",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.lines().count(), 2);
}

#[test]
fn guards_pair_with_the_numbers_and_skip_ids() {
    let fake = fake();
    let ran = fake.run(&[
        "pri",
        "1",
        "A",
        "id:k3f9",
        "B",
        "3",
        "C",
        "--human",
        "--expect",
        "Call",
        "--expect",
        "docs",
        "--no-config",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        todo(&fake),
        "(A) Call client +work\n(B) Add CLI ls id:k3f9\n(C) Write docs\n"
    );
}

#[test]
fn the_json_mismatch_carries_the_item_and_both_texts() {
    let fake = fake();
    let ran = fake.run(&[
        "pri",
        "2",
        "A",
        "--human",
        "--expect",
        "Add",
        "--json",
        "--no-config",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let ran = fake.run(&[
        "id",
        "1",
        "--owner",
        "agent1",
        "--expect",
        "Call",
        "--json",
        "--no-config",
    ]);
    assert_eq!(ran.code, 3);
    assert!(ran.out.contains(
        "\"hint\":null,\"nr\":1,\"source\":\"todo\",\"id\":null,\
         \"actual\":\"Call client\",\"expected\":\"Call\"}"
    ));
}

#[test]
fn an_identity_error_comes_before_the_files_are_looked_for() {
    let fake = Fake::new();
    let ran = fake.run(&["pri", "1", "A", "--json", "--no-config"]);
    assert_eq!(ran.code, 2);
    assert!(ran.out.contains("\"file\":null,\"done_file\":null"));
    assert!(ran.out.contains("\"code\":\"no_identity\""));
}

#[test]
fn a_read_may_be_guarded_and_never_needs_an_identity() {
    let fake = fake();
    let ok = fake.run(&["show", "3", "--expect", "docs", "--no-config"]);
    assert_eq!(ok.code, 0, "{}", ok.err);
    let bad = fake.run(&["show", "3", "--expect", "nope", "--no-config"]);
    assert_eq!(bad.code, 3);
    assert_eq!(
        bad.err,
        "lsty: item 3 is \"Write docs\", expected \"nope\"\n"
    );
    let twice = fake.run(&["show", "3", "--expect", "a", "--expect", "b"]);
    assert_eq!(twice.code, 2);
}

/// A reference no item can have: line 0 and a number beyond any file.
const NO_LINE: &[(&str, &str)] = &[("0", "0"), ("99999999999", "99999999999")];

#[test]
fn a_guard_on_a_line_that_cannot_exist_has_the_item_shape() {
    for (arg, nr) in NO_LINE {
        let fake = fake();
        let ran = fake.run(&[
            "pri",
            arg,
            "A",
            "--human",
            "--expect",
            "x",
            "--json",
            "--no-config",
        ]);
        assert_eq!(ran.code, 3, "{arg}");
        let want = format!(
            "\"message\":\"item {arg} does not exist, expected \\\"x\\\"\",\
             \"hint\":null,\"nr\":{nr},\"source\":\"todo\",\"id\":null,\
             \"actual\":null,\"expected\":\"x\"}}}}"
        );
        assert!(ran.out.contains(&want), "{}", ran.out);
    }
}
