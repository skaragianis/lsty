//! `open` through the whole command line, with a fake opener.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use super::run_tests::{Fake, Ran};

fn fake(todo: &str) -> Fake {
    Fake::new()
        .tty()
        .file("todo.txt", todo)
        .file("done.txt", "x 2026-09-01 Old file:notes.md\n")
        .file("a.md", "a")
        .file("b.md", "b")
        .file("c.md", "c")
        .file("tool.desktop", "x")
        .var("TODO_FILE", "todo.txt")
}

fn open(fake: &Fake, args: &[&str]) -> Ran {
    let mut line = vec!["open"];
    line.extend(args);
    line.push("--no-config");
    fake.run(&line)
}

fn at(fake: &Fake, names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(|n| fake.path().join(n)).collect()
}

/// Name, the todo file, files to make, arguments, the file the editor
/// opens and the path the opener gets.
type Case = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

const OPENS: [Case; 4] = [
    (
        "27.31 text goes to the editor",
        "Read file:notes.md\n",
        &[("notes.md", "# notes\n")],
        &["1"],
        &["notes.md"],
        &[],
    ),
    (
        "27.32 percent-decoded path",
        "Read file:docs/x%20y.pdf\n",
        &[("docs/x y.pdf", "\0PDF")],
        &["1"],
        &[],
        &["docs/x y.pdf"],
    ),
    (
        "27.33 the second link",
        "Read file:a.md file:b.md file:c.md\n",
        &[],
        &["1", "2"],
        &["b.md"],
        &[],
    ),
    (
        "27.36 archived item",
        "Open\n",
        &[("notes.md", "n")],
        &["done:1"],
        &["notes.md"],
        &[],
    ),
];

#[test]
fn a_link_opens_where_its_kind_says() {
    for (name, todo, files, args, edited, opened) in OPENS {
        let mut fake = fake(todo);
        for (file, text) in files {
            fake = fake.file(file, text);
        }
        let ran = open(&fake, args);
        assert_eq!((ran.code, ran.err.as_str()), (0, ""), "{name}");
        assert_eq!(ran.out, "", "{name}");
        assert_eq!(ran.edited, at(&fake, edited), "{name}");
        assert_eq!(ran.opened, at(&fake, opened), "{name}");
    }
}

/// Name, the todo file, arguments, the exit code and a part of stderr
/// (`{root}` is the work folder).
type Refusal = (
    &'static str,
    &'static str,
    &'static [&'static str],
    u8,
    &'static str,
);

const REFUSALS: [Refusal; 12] = [
    (
        "27.33 several links",
        "Read file:a.md file:b.md file:c.md\n",
        &["1"],
        2,
        "1 has 3 links; pass a number\n  1 {root}/a.md\n  2 {root}/b.md\n  3 {root}/c.md",
    ),
    (
        "27.33 number out of range",
        "Read file:a.md file:b.md\n",
        &["1", "9"],
        2,
        "link 9 is not between 1 and 2",
    ),
    ("number zero", "Read file:a.md\n", &["1", "0"], 2, "link 0"),
    ("not a number", "Read file:a.md\n", &["1", "x"], 2, "link x"),
    ("27.34 no links", "Read\n", &["1"], 2, "1 has no file: link"),
    (
        "27.35 executable",
        "Run file:tool.sh\n",
        &["1"],
        2,
        "won't open executable: {root}/tool.sh",
    ),
    (
        "27.35 launcher extension",
        "Run file:tool.desktop\n",
        &["1"],
        2,
        "won't open executable: {root}/tool.desktop",
    ),
    (
        "missing file",
        "Read file:gone.md\n",
        &["1"],
        1,
        "{root}/gone.md does not exist",
    ),
    ("no item", "Read\n", &["5"], 3, "no item 5"),
    (
        "too many arguments",
        "Read\n",
        &["1", "2", "3"],
        2,
        "open takes",
    ),
    ("missing reference", "Read\n", &[], 2, "missing argument"),
    ("a list", "Read\n", &["1,2"], 2, "open takes one reference"),
];

#[test]
fn a_refused_open_starts_nothing() {
    for (name, todo, args, exit, want) in REFUSALS {
        let fake = fake(todo);
        let tool = fake.path().join("tool.sh");
        fs::write(&tool, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        let ran = open(&fake, args);
        let root = fake.path().display().to_string();
        assert_eq!(ran.code, exit, "{name}: {}", ran.err);
        let want = want.replace("{root}", &root);
        assert!(ran.err.contains(&want), "{name}: {}", ran.err);
        assert!(ran.edited.is_empty() && ran.opened.is_empty(), "{name}");
    }
}

#[test]
fn open_takes_no_json() {
    let fake = fake("Read file:a.md\n");
    let ran = open(&fake, &["1", "--json"]);
    assert_eq!(ran.code, 2, "27.37");
    assert!(ran.out.contains("open does not take --json"), "{}", ran.out);
    assert_eq!(ran.edited.len(), 0);
}

#[test]
fn an_out_of_range_number_is_invalid_value_with_its_field() {
    let fake = fake("Read file:a.md\n");
    let ran = fake.run(&["open", "1", "9", "--no-config"]);
    assert_eq!(ran.code, 2);
    let ran = fake.run(&["open", "1", "9", "--no-config", "--json"]);
    assert_eq!(ran.code, 2);
}

#[test]
fn open_needs_two_terminals_and_writes_nothing() {
    let fake = Fake::new().file("todo.txt", "Read file:a.md\n");
    let ran = fake.run(&["open", "1"]);
    assert_eq!(
        (ran.code, ran.err.as_str()),
        (2, "lsty: open needs a terminal\n")
    );
    let fake = self::fake("Read file:a.md\n");
    let before = fs::read_to_string(fake.path().join("todo.txt")).unwrap();
    assert_eq!(open(&fake, &["1"]).code, 0);
    assert_eq!(
        fs::read_to_string(fake.path().join("todo.txt")).unwrap(),
        before
    );
}
