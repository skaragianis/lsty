//! The mutation commands through the command line.

use std::fs;

use serde_json::Value;

use super::matrix::MATRIX;
use super::run_tests::Fake;

const FIXTURE: &str = "Call client +work\nAdd CLI ls +spec02 id:k3f9\n\
                       (B) Write docs\nFix tests dep:k3f9\n";

fn fake(content: &str) -> Fake {
    Fake::new()
        .file("todo.txt", content)
        .file("done.txt", "x 2026-09-01 Old\n")
        .var("TODO_FILE", "todo.txt")
}

fn todo(fake: &Fake) -> String {
    fs::read_to_string(fake.path().join("todo.txt")).unwrap()
}

fn with_flags<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut line = args.to_vec();
    if !line.contains(&"--owner") {
        line.push("--human");
    }
    line.extend(["--json", "--no-config"]);
    line
}

/// Runs one command with `--json` and parses what it prints.
fn mutation(content: &str, args: &[&str]) -> Value {
    let fake = fake(content);
    let ran = fake.run(&with_flags(args));
    assert_eq!(ran.code, 0, "{args:?}: {}", ran.err);
    serde_json::from_str(&ran.out).unwrap()
}

fn nonempty(value: &Value, key: &str) -> bool {
    value[key].as_array().is_some_and(|a| !a.is_empty())
}

/// The command lines run for each row of the matrix.
type Args = &'static [&'static str];
type Row = (&'static str, &'static [Args]);

const CASES: &[Row] = &[
    (
        "add",
        &[
            &["add", "N"],
            &["add", "N", "--top"],
            &["add", "N", "--after", "1"],
            &[
                "add",
                "Call client +work",
                "--if-absent",
                "--no-date",
                "--id=cc",
            ],
        ],
    ),
    (
        "addm",
        &[
            &["addm", "A"],
            &["addm", "A", "--top"],
            &[
                "addm",
                "Call client +work id:cc",
                "--if-absent",
                "--no-date",
            ],
        ],
    ),
    (
        "append",
        &[&["append", "id:k3f9", "x"], &["append", "1,2", "x"]],
    ),
    ("prepend", &[&["prepend", "id:k3f9", "URGENT"]]),
    ("replace", &[&["replace", "id:k3f9", "Ring"]]),
    ("pri", &[&["pri", "1", "A", "2", "B"]]),
    ("depri", &[&["depri", "1"]]),
    ("due", &[&["due", "id:k3f9", "fri"]]),
    ("tag", &[&["tag", "id:k3f9", "+work"]]),
    (
        "untag",
        &[&["untag", "1", "+work"], &["untag", "id:k3f9", "id:k3f9"]],
    ),
    ("id", &[&["id", "1"]]),
    ("del REF", &[&["del", "2"], &["del", "1,3"]]),
    ("del REF TERM", &[&["del", "1", "client"]]),
    (
        "do",
        &[
            &["do", "1"],
            &["do", "1", "--archive"],
            &["do", "3", "--no-archive"],
        ],
    ),
    ("reopen", &[&["reopen", "1"]]),
    (
        "dep, undep",
        &[&["dep", "1", "on", "3"], &["undep", "4", "on", "2"]],
    ),
    ("start", &[&["start", "1"]]),
    ("stop", &[&["stop", "1"]]),
    ("restore", &[&["restore", "done:1"]]),
    (
        "up, down",
        &[&["up", "2"], &["down", "1"], &["down", "1,2", "2"]],
    ),
    (
        "reorder",
        &[&["reorder", "1", "3"], &["reorder", "4", "--before", "1"]],
    ),
    ("archive", &[&["archive"]]),
    ("deduplicate", &[&["deduplicate"]]),
];

const RECURRING_FIRST: &str = "Call client rec:1w\nB\nC\n";
const DONE_FIRST: &str = "x 2026-09-28 Gone id:dd\n\
                          Call client +work\nAdd CLI ls +spec02 id:k3f9\n\
                          (B) Write docs\nFix tests dep:k3f9\n";

/// The list a row runs on: most use the common one.
fn content_of(command: &str) -> &'static str {
    match command {
        "reopen" | "archive" => DONE_FIRST,
        "do" => RECURRING_FIRST,
        _ => FIXTURE,
    }
}

#[test]
fn the_matrix_agrees_with_the_results() {
    for row in MATRIX {
        let cases = CASES
            .iter()
            .find(|(command, _)| *command == row.command)
            .map_or(&[][..], |(_, cases)| *cases);
        assert!(!cases.is_empty(), "{}", row.command);
        let (mut renumbered, mut changed) = (false, false);
        for args in cases {
            let got = mutation(content_of(row.command), args);
            renumbered |= got["renumbered"] == Value::Bool(true);
            changed |= nonempty(&got, "changed");
        }
        assert_eq!(renumbered, row.renumbers, "{} renumbered", row.command);
        assert_eq!(changed, row.changes, "{} changed", row.command);
    }
}

#[test]
fn the_matrix_has_a_row_for_each_command() {
    let rows: Vec<_> = MATRIX.iter().map(|r| r.command).collect();
    assert_eq!(
        rows,
        [
            "add",
            "addm",
            "append",
            "prepend",
            "replace",
            "pri",
            "depri",
            "due",
            "tag",
            "untag",
            "id",
            "del REF",
            "del REF TERM",
            "do",
            "reopen",
            "dep, undep",
            "start",
            "stop",
            "restore",
            "up, down",
            "reorder",
            "archive",
            "deduplicate",
        ]
    );
}

/// A command that finds nothing to do, with the file it runs on.
const NO_OPS: &[(&str, &[&str])] = &[
    ("add", &["add", "Write docs", "--if-absent", "--no-date"]),
    ("addm", &["addm", "Write docs", "--if-absent", "--no-date"]),
    ("replace", &["replace", "1", "Call client +work"]),
    ("pri", &["pri", "3", "B"]),
    ("depri", &["depri", "1"]),
    ("due", &["due", "1", ""]),
    ("id", &["id", "id:k3f9"]),
    ("tag", &["tag", "1", "+work"]),
    ("untag", &["untag", "1", "@nowhere"]),
    ("stop", &["stop", "1"]),
    ("reopen", &["reopen", "1"]),
    ("undep", &["undep", "1", "on", "2"]),
    ("up", &["up", "1"]),
    ("down", &["down", "4"]),
    ("reorder", &["reorder", "1", "1"]),
    ("del term", &["del", "1", "nothing"]),
];

#[test]
fn a_no_op_reports_the_item_unchanged() {
    for (name, args) in NO_OPS {
        let got = mutation(FIXTURE, args);
        assert!(nonempty(&got, "items"), "{name}");
        assert_eq!(got["items"], got["before"], "{name}");
        assert!(!nonempty(&got, "changed"), "{name}");
        assert!(!nonempty(&got, "spawned"), "{name}");
        assert_eq!(got["renumbered"], Value::Bool(false), "{name}");
    }
}

/// The arguments of `addm` or `add`, the lines written and what is
/// printed.
type Written = (&'static str, Args, &'static str, &'static str);

const ADDM: &[Written] = &[
    (
        "a real newline",
        &["addm", "L1\nL2"],
        "5 added: L1\n6 added: L2\n",
        "2026-09-29 L1\n2026-09-29 L2\n",
    ),
    (
        "empty lines are dropped",
        &["addm", "L1\n\n", "L2"],
        "5 added: L1\n6 added: L2\n",
        "2026-09-29 L1\n2026-09-29 L2\n",
    ),
    (
        "backslash and n",
        &["addm", "L1\\nL2"],
        "5 added: L1\\nL2\n",
        "2026-09-29 L1\\nL2\n",
    ),
    (
        "an empty --pri is not given",
        &["add", "Call", "--pri="],
        "5 added: Call\n",
        "2026-09-29 Call\n",
    ),
    (
        "a priority",
        &["add", "Call", "--pri", "c"],
        "5 added: Call\n",
        "(C) 2026-09-29 Call\n",
    ),
];

#[test]
fn add_and_addm_write_the_lines() {
    for (name, args, out, added) in ADDM {
        let fake = fake(FIXTURE);
        let mut line = args.to_vec();
        line.extend(["--human", "--no-config"]);
        let ran = fake.run(&line);
        assert_eq!(ran.code, 0, "{name}: {}", ran.err);
        assert_eq!(ran.out, *out, "{name}");
        assert_eq!(todo(&fake), format!("{FIXTURE}{added}"), "{name}");
    }
}

#[test]
fn the_id_flags_choose_the_id() {
    let cases: [(Args, Option<&str>); 4] = [
        (&["add", "A"], None),
        (&["add", "--id", "A"], Some("")),
        (&["add", "A", "--id="], Some("")),
        (&["add", "A", "--id=mine"], Some("id:mine")),
    ];
    for (args, id) in cases {
        let fake = fake(FIXTURE);
        let mut line = args.to_vec();
        line.extend(["--human", "--no-config"]);
        let ran = fake.run(&line);
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.err);
        let text = todo(&fake);
        let last = text.lines().last().unwrap_or_default();
        match id {
            None => assert_eq!(last, "2026-09-29 A", "{args:?}"),
            Some("") => {
                assert!(last.contains(" id:"), "{args:?}: {last}");
            }
            Some(id) => assert!(last.ends_with(id), "{args:?}: {last}"),
        }
    }
}

#[test]
fn messages_keep_bytes_that_are_not_utf8() {
    let fake = fake("caf\u{e9}\n");
    fs::write(fake.path().join("todo.txt"), b"caf\xe9\n").unwrap();
    let ran = fake.run(&["append", "1", "x", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out_bytes.windows(4).any(|w| w == b"caf\xe9"),
        "{:?}",
        ran.out_bytes
    );
}

/// Commands that miss or have too many arguments, and the hint.
const USAGE: &[(&[&str], &str)] = &[
    (&["add"], "usage: lsty add TEXT\u{2026}"),
    (&["addm"], "usage: lsty addm TEXT\u{2026}"),
    (&["due", "5"], "usage: lsty due REFS DATE"),
    (&["due", "5", "fri", "x"], "usage: lsty due REFS DATE"),
    (&["id"], "usage: lsty id REFS [ID]"),
    (&["id", "1", "a", "b"], "usage: lsty id REFS [ID]"),
    (&["append", "1"], "usage: lsty append REFS TEXT\u{2026}"),
    (&["tag", "1"], "usage: lsty tag REFS TAG\u{2026}"),
    (&["depri"], "usage: lsty depri REFS\u{2026}"),
    (
        &["pri", "1", "A", "2"],
        "usage: lsty pri REFS PRI [REFS PRI\u{2026}]",
    ),
];

#[test]
fn a_missing_argument_is_a_usage_error() {
    for (args, hint) in USAGE {
        let fake = fake(FIXTURE);
        let mut line = args.to_vec();
        line.extend(["--human", "--no-config"]);
        let ran = fake.run(&line);
        assert_eq!(ran.code, 2, "{args:?}");
        assert!(ran.err.contains(hint), "{args:?}: {}", ran.err);
        assert_eq!(todo(&fake), FIXTURE, "{args:?}");
    }
}

#[test]
fn due_with_an_empty_date_clears() {
    let fake = fake("Call due:2026-10-01\n");
    let ran = fake.run(&["due", "1", "", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "Call\n");
}

#[test]
fn an_empty_id_is_not_given() {
    let fake = fake("a\nb id:k3f9\n");
    let ran = fake.run(&["id", "1", "", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(todo(&fake).starts_with("a id:"), "{}", todo(&fake));
    let ran = fake.run(&["id", "2", "", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("(unchanged)"), "{}", ran.out);
}

/// Argument shapes that are usage errors, with the hint.
const SHAPES: &[(&[&str], &str)] = &[
    (&["dep", "3", "4"], "usage: lsty dep REFS on REFS"),
    (&["dep", "3", "on"], "usage: lsty dep REFS on REFS"),
    (&["dep", "on", "3"], "usage: lsty dep REFS on REFS"),
    (
        &["dep", "1", "on", "2", "on", "3"],
        "usage: lsty dep REFS on REFS",
    ),
    (&["undep", "1"], "usage: lsty undep REFS on REFS"),
    (&["archive", "now"], "archive takes no arguments"),
    (&["deduplicate", "x"], "deduplicate takes no arguments"),
    (&["do"], "usage: lsty do REFS\u{2026}"),
    (&["start", ","], "usage: lsty start REFS\u{2026}"),
    (&["reorder", "1"], "usage: lsty reorder REFS POS"),
    (
        &["reorder", "--after", "bad"],
        "usage: lsty reorder REFS POS",
    ),
    (
        &["reorder", "xyz", "--after", "bad"],
        "\"xyz\" is not a reference",
    ),
    (&["reorder", "1", "3", "--after", "2"], "not both"),
];

#[test]
fn a_wrong_shape_is_a_usage_error() {
    for (args, text) in SHAPES {
        let fake = fake(FIXTURE);
        let mut line = args.to_vec();
        line.extend(["--human", "--no-config"]);
        let ran = fake.run(&line);
        assert_eq!(ran.code, 2, "{args:?}");
        assert!(ran.err.contains(text), "{args:?}: {}", ran.err);
        assert_eq!(todo(&fake), FIXTURE, "{args:?}");
    }
}

/// How several arguments and comma lists name the same items.
const SPELLINGS: &[&[&str]] = &[
    &["do", "1", "3"],
    &["do", "1,3"],
    &["do", "1,", "3"],
    &["do", "1", ",3"],
];

#[test]
fn lists_of_references_are_one_list() {
    let mut results = Vec::new();
    for args in SPELLINGS {
        let fake = fake(FIXTURE);
        let mut line = args.to_vec();
        line.extend(["--human", "--no-config"]);
        let ran = fake.run(&line);
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.err);
        results.push((ran.out, todo(&fake)));
    }
    assert!(results.windows(2).all(|w| w[0] == w[1]), "{results:?}");
}

#[test]
fn del_takes_the_rest_as_a_term() {
    let fake = fake("Call client 5 now\nB\n");
    let ran = fake.run(&["del", "1", "5", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "Call client now\nB\n");
}

#[test]
fn up_and_down_default_to_one_step() {
    let fake = fake("A\nB\nC\n");
    let ran = fake.run(&["down", "1", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "B\nA\nC\n");
    let ran = fake.run(&["down", "2", "2", "--human", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "B\nC\nA\n");
}

#[test]
fn done_is_do() {
    let fake = fake(FIXTURE);
    let mut line = vec!["done", "1"];
    line.extend(["--json", "--no-config", "--human"]);
    let ran = fake.run(&line);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let got: Value = serde_json::from_str(&ran.out).unwrap();
    assert_eq!(got["action"], "do");
}

#[test]
fn an_agent_places_items_by_anchor() {
    let fake = fake("A id:a\nB id:b\n");
    let ran = fake.run(&[
        "reorder",
        "id:a",
        "2",
        "--owner",
        "agent1",
        "--no-config",
    ]);
    assert_eq!(ran.code, 2, "{}", ran.err);
    assert!(ran.err.contains("use --after id:"), "{}", ran.err);
}

#[test]
fn a_bare_id_flag_keeps_the_default_for_dep() {
    for flag in ["--id", "--id="] {
        let fake = fake("A\nB\n");
        let ran =
            fake.run(&["dep", "1", "on", "2", flag, "--human", "--no-config"]);
        assert_eq!(ran.code, 0, "{flag}: {}", ran.err);
        assert!(todo(&fake).contains("dep:"), "{flag}");
    }
    let fake = fake("A\nB\n");
    let ran = fake.run(&[
        "dep",
        "1",
        "on",
        "2",
        "--id=pre",
        "--human",
        "--no-config",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "A dep:pre\nB id:pre\n");
}
