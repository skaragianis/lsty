//! The mutation commands of `add` to `untag` through the command line.

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
];

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
            let got = mutation(FIXTURE, args);
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
            "add", "addm", "append", "prepend", "replace", "pri", "depri",
            "due", "tag", "untag", "id"
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
