use std::{collections::HashMap, ffi::OsString, fs, path::Path};

use super::Todosh;
use crate::{errs::Code, fsio::test_util::dir};

fn parse(text: &str, env: &[(&str, &str)]) -> Todosh {
    let env: HashMap<String, OsString> = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
        .collect();
    let vars = |k: &str| env.get(k).cloned();
    Todosh::parse(
        Path::new("cfg"),
        text.as_bytes(),
        &vars,
        Some(Path::new("/home/u")),
    )
}

fn value(file: &Todosh, name: &str) -> Option<String> {
    file.get(name).map(|(v, _)| v.to_owned())
}

/// One row: the line, the value `TODO_DIR` gets (`None` when the line
/// sets nothing) and the warning, if any.
type Row = (&'static str, Option<&'static str>, &'static str);

const FORMS: [Row; 48] = [
    ("TODO_DIR=/a/b", Some("/a/b"), ""),
    ("export TODO_DIR=/a/b", Some("/a/b"), ""),
    ("  export   TODO_DIR=/a/b  ", Some("/a/b"), ""),
    ("TODO_DIR=", Some(""), ""),
    ("TODO_DIR='/a b/$HOME'", Some("/a b/$HOME"), ""),
    ("TODO_DIR=\"/a b\"", Some("/a b"), ""),
    ("TODO_DIR=\"$HOME/x\"", Some("/env/home/x"), ""),
    ("TODO_DIR=\"${HOME}x\"", Some("/env/homex"), ""),
    ("TODO_DIR=$HOME/x", Some("/env/home/x"), ""),
    ("TODO_DIR=\"a\\\"b\\$c\\\\d\\e\"", Some("a\"b$c\\d\\e"), ""),
    ("TODO_DIR=a\\ b", Some("a b"), ""),
    ("TODO_DIR=~", Some("/home/u"), ""),
    ("TODO_DIR=~/lists", Some("/home/u/lists"), ""),
    ("TODO_DIR=\"~/lists\"", Some("~/lists"), ""),
    ("TODO_DIR=/a # comment", Some("/a"), ""),
    ("TODO_DIR=/a#b", Some("/a#b"), ""),
    ("TODO_DIR=\"/a\"'/b'c", Some("/a/bc"), ""),
    ("TODO_DIR=cost$", Some("cost$"), ""),
    (
        "TODO_DIR=$(dirname \"$0\")",
        None,
        "line 1: cannot run a command: TODO_DIR=$(dirname \"$0\")",
    ),
    (
        "TODO_DIR=`pwd`",
        None,
        "line 1: cannot run a command: TODO_DIR=`pwd`",
    ),
    (
        "TODO_DIR=${X:-/a}",
        None,
        "line 1: cannot work out ${X:-/a}: TODO_DIR=${X:-/a}",
    ),
    (
        "TODO_DIR=$NOPE/x",
        None,
        "line 1: NOPE is not set: TODO_DIR=$NOPE/x",
    ),
    (
        "TODO_DIR='open",
        None,
        "line 1: quote not closed: TODO_DIR='open",
    ),
    (
        "TODO_DIR=a b",
        None,
        "line 1: text after the value: TODO_DIR=a b",
    ),
    (
        "TODO_DIR=${HOME",
        None,
        "line 1: cannot work out ${HOME}: TODO_DIR=${HOME",
    ),
    (
        "TODO_DIR=$1",
        None,
        "line 1: cannot work out $1: TODO_DIR=$1",
    ),
    (
        "TODO_DIR=/x/a;",
        None,
        "line 1: cannot work out ;: TODO_DIR=/x/a;",
    ),
    (
        "TODO_DIR=/x/f&",
        None,
        "line 1: cannot work out &: TODO_DIR=/x/f&",
    ),
    (
        "TODO_DIR=/x/g|cat",
        None,
        "line 1: cannot work out |: TODO_DIR=/x/g|cat",
    ),
    (
        "TODO_DIR=/x/y>/dev/null",
        None,
        "line 1: cannot work out >: TODO_DIR=/x/y>/dev/null",
    ),
    (
        "TODO_DIR=/x/y<in",
        None,
        "line 1: cannot work out <: TODO_DIR=/x/y<in",
    ),
    (
        "TODO_DIR=/x/z(1)",
        None,
        "line 1: cannot work out (: TODO_DIR=/x/z(1)",
    ),
    (
        "TODO_DIR=/x/z)",
        None,
        "line 1: cannot work out ): TODO_DIR=/x/z)",
    ),
    (
        "TODO_DIR=$'/x/c'",
        None,
        "line 1: cannot work out $': TODO_DIR=$'/x/c'",
    ),
    (
        "TODO_DIR=$\"/x/c\"",
        None,
        "line 1: cannot work out $\": TODO_DIR=$\"/x/c\"",
    ),
    (
        "TODO_DIR=~root/d",
        None,
        "line 1: cannot work out ~: TODO_DIR=~root/d",
    ),
    (
        "TODO_DIR=/x/o:~/p",
        None,
        "line 1: cannot work out :~: TODO_DIR=/x/o:~/p",
    ),
    (
        "TODO_DIR=/x/$[1+2]",
        None,
        "line 1: cannot work out $[: TODO_DIR=/x/$[1+2]",
    ),
    (
        "TODO_DIR=\"/x/$[2+2]\"",
        None,
        "line 1: cannot work out $[: TODO_DIR=\"/x/$[2+2]\"",
    ),
    (
        "export X=a;export TODO_DIR=/x/bb",
        None,
        "line 1: cannot work out ;: export X=a;export TODO_DIR=/x/bb",
    ),
    (
        "export X=$(echo a) TODO_DIR=/x/cc",
        None,
        "line 1: cannot run a command: export X=$(echo a) TODO_DIR=/x/cc",
    ),
    (
        "X=a TODO_DIR=/x/dd",
        None,
        "line 1: text after the value: X=a TODO_DIR=/x/dd",
    ),
    (
        "X=a && TODO_DIR=/x/ee",
        None,
        "line 1: text after the value: X=a && TODO_DIR=/x/ee",
    ),
    ("TODO_DIR=\"/x;&|<>()\"", Some("/x;&|<>()"), ""),
    ("TODO_DIR=/x/file~", Some("/x/file~"), ""),
    ("TODO_DIR=/x/a\t# note", Some("/x/a"), ""),
    ("TODO_DIR=/x/q\u{a0}#r", Some("/x/q\u{a0}#r"), ""),
    ("TODO_DIR=/x/a\r", Some("/x/a"), ""),
];

#[test]
fn each_form_of_a_value() {
    for (line, want, warning) in FORMS {
        let file = parse(line, &[("HOME", "/env/home")]);
        assert_eq!(value(&file, "TODO_DIR").as_deref(), want, "{line}");
        let warnings: Vec<&str> =
            file.warnings.iter().map(String::as_str).collect();
        let expected: &[&str] =
            if warning.is_empty() { &[] } else { &[warning] };
        assert_eq!(warnings, expected, "{line}");
    }
}

#[test]
fn a_line_that_is_not_an_assignment_is_skipped_with_a_warning() {
    let file = parse(
        "# comment\n\n  # another\ncd /x\nexport A\nTODO-DIR=1\nif true; then\n=x\n",
        &[],
    );
    assert_eq!(
        file.warnings,
        [
            "line 4: not an assignment: cd /x",
            "line 5: not an assignment: export A",
            "line 6: not an assignment: TODO-DIR=1",
            "line 7: not an assignment: if true; then",
            "line 8: not an assignment: =x",
        ]
    );
}

#[test]
fn a_line_that_is_not_utf8_is_skipped_with_a_warning() {
    let env = |_: &str| None;
    let file =
        Todosh::parse(Path::new("cfg"), b"A=\xff\nTODO_DIR=/a\n", &env, None);
    assert_eq!(file.warnings, ["line 1: not UTF-8"]);
    assert_eq!(value(&file, "TODO_DIR").as_deref(), Some("/a"));
}

#[test]
fn a_variable_lsty_ignores_is_never_warned_about() {
    let file = parse(
        "export PRI_A=$GREEN\nTODOTXT_SORT_COMMAND='sort -f'\nX=$(date)\nY=`z`\nZ='open\n",
        &[],
    );
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    assert_eq!(value(&file, "PRI_A"), None);
    assert_eq!(
        value(&file, "TODOTXT_SORT_COMMAND").as_deref(),
        Some("sort -f")
    );
}

#[test]
fn the_variable_of_an_earlier_line_beats_the_environment() {
    let text = "A=/file\nTODO_DIR=$A/x\nTODO_FILE=\"${TODO_DIR}/todo.txt\"";
    let file = parse(text, &[("A", "/env"), ("TODO_DIR", "/env/dir")]);
    assert_eq!(value(&file, "TODO_DIR").as_deref(), Some("/file/x"));
    assert_eq!(
        value(&file, "TODO_FILE").as_deref(),
        Some("/file/x/todo.txt")
    );
    assert_eq!(file.get("TODO_FILE"), Some(("/file/x/todo.txt", 3)));
    let file =
        parse("TODO_FILE=$TODO_DIR/todo.txt", &[("TODO_DIR", "/env/dir")]);
    assert_eq!(
        value(&file, "TODO_FILE").as_deref(),
        Some("/env/dir/todo.txt")
    );
}

#[test]
fn the_template_of_todo_sh_sets_nothing_it_cannot_work_out() {
    let text = "export TODO_DIR=$(dirname \"$0\")\nexport TODO_FILE=\"$TODO_DIR/todo.txt\"\nexport DONE_FILE=\"$TODO_DIR/done.txt\"\nexport TODOTXT_AUTO_ARCHIVE=1\n";
    let file = parse(text, &[]);
    assert_eq!(value(&file, "TODO_FILE"), None);
    assert_eq!(value(&file, "DONE_FILE"), None);
    assert_eq!(value(&file, "TODOTXT_AUTO_ARCHIVE").as_deref(), Some("1"));
    assert_eq!(file.warnings.len(), 3);
    assert!(file.warnings[1].starts_with("line 2: TODO_DIR is not set: "));
}

#[test]
fn a_later_line_wins() {
    let file = parse("TODO_DIR=/a\nTODO_DIR=/b\n", &[]);
    assert_eq!(file.get("TODO_DIR"), Some(("/b", 2)));
}

fn places(files: &[&str], env: &[(&str, &str)]) -> Result<Todosh, Code> {
    let (_tmp, root) = dir();
    let home = root.join("home");
    for file in files {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("W={file}\n")).unwrap();
    }
    let env: HashMap<String, OsString> = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), root.join(v).into_os_string()))
        .collect();
    let vars = |k: &str| env.get(k).cloned();
    let got = Todosh::read(None, &vars, Some(&home), &root);
    got.map_err(|e| e.code())
}

fn which(got: &Todosh) -> &str {
    got.get("W").map_or("none", |(v, _)| v)
}

#[test]
fn the_first_place_that_exists_is_read() {
    let all = [
        "home/.todo/config",
        "home/todo.cfg",
        "home/.todo.cfg",
        "xdg/todo/config",
        "global",
    ];
    let env = [
        ("XDG_CONFIG_HOME", "xdg"),
        ("TODOTXT_GLOBAL_CFG_FILE", "global"),
    ];
    for at in 0..all.len() {
        let got = places(&all[at..], &env).expect("read");
        assert_eq!(which(&got), all[at]);
    }
    let got = places(&["home/.config/todo/config", "global"], &env[1..])
        .expect("read");
    assert_eq!(which(&got), "home/.config/todo/config");
    let got = places(&["home/.config/todo/config", "xdg/todo/config"], &env)
        .expect("read");
    assert_eq!(which(&got), "xdg/todo/config");
    assert_eq!(which(&places(&[], &env).expect("read")), "none");
}

#[test]
fn a_named_file_that_is_not_there_is_an_error() {
    let (_tmp, root) = dir();
    let none = |_: &str| None;
    let named = Path::new("missing");
    let err =
        Todosh::read(Some(named), &none, None, &root).expect_err("missing");
    assert_eq!(err.code(), Code::Io);
    assert_eq!(err.message(), "missing not found");
    let env =
        |k: &str| (k == "TODOTXT_CFG_FILE").then(|| OsString::from("gone"));
    let err = Todosh::read(None, &env, None, &root).expect_err("missing");
    assert_eq!(err.message(), "gone not found");
}

#[test]
fn the_named_file_beats_the_variable_and_the_places() {
    let (_tmp, root) = dir();
    fs::write(root.join("a"), "W=a\n").unwrap();
    fs::write(root.join("b"), "W=b\n").unwrap();
    let env = |k: &str| (k == "TODOTXT_CFG_FILE").then(|| OsString::from("b"));
    let got = Todosh::read(Some(Path::new("a")), &env, None, &root).unwrap();
    assert_eq!(which(&got), "a");
    let got = Todosh::read(None, &env, None, &root).unwrap();
    assert_eq!(which(&got), "b");
    assert_eq!(got.source, "b");
}

#[test]
fn text_after_a_value_warns_when_the_line_mentions_a_variable_lsty_reads() {
    let file = parse("export A=1 TODO_DIR=/x/b\nSORT=a b\nOK=\"a b\"\n", &[]);
    assert_eq!(value(&file, "TODO_DIR"), None);
    assert_eq!(
        file.warnings,
        ["line 1: text after the value: export A=1 TODO_DIR=/x/b"]
    );
}

#[test]
fn a_value_lsty_ignores_is_never_warned_about_unless_its_shape_is_wrong() {
    let file = parse("PRI_A=$(echo hi)\nX=a;b\nY=$NOPE\n", &[]);
    assert_eq!(file.warnings.len(), 0);
}
