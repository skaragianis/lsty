use std::path::Path;

use super::run::{Done, Files, script, unquote};

/// A transcript whose todo line is not UTF-8 and whose `kept` section is
/// `expected`.
fn transcript(expected: &[u8]) -> (&'static str, Files) {
    let files = vec![
        ("todo.txt".to_owned(), b"caf\xe9\n".to_vec()),
        ("kept".to_owned(), expected.to_vec()),
    ];
    ("start\nfile todo.txt kept\n", files)
}

fn run(expected: &[u8], update: bool) -> (Done, Option<Files>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (comment, files) = transcript(expected);
    script(&root, "t", comment, files, update)
}

#[test]
fn a_file_that_is_not_text_is_compared_byte_for_byte() {
    assert_eq!(run(b"caf\xe9\n", false).0, Ok(()));
    // U+FFFD is not the byte it replaces.
    assert!(run("caf\u{fffd}\n".as_bytes(), false).0.is_err());
}

#[test]
fn updating_keeps_the_bytes_of_the_sections() {
    let (done, updated) = run(b"x\n", true);
    assert_eq!(done, Ok(()));
    assert_eq!(updated, Some(transcript(b"caf\xe9\n").1));
}

const UNQUOTED: &[(&str, Option<&str>)] = &[
    (r#""one\ntwo""#, Some("one\ntwo")),
    (r#""a\r\n\tb\\c\"d""#, Some("a\r\n\tb\\c\"d")),
    (r#""\a\b\f\v""#, Some("\u{7}\u{8}\u{c}\u{b}")),
    (r#""\x01\x41""#, Some("\u{1}A")),
    (r#""\101\0""#, None),
    (r#""\101\000""#, Some("A\0")),
    (r#""\u00e9\U0001F600""#, Some("\u{e9}\u{1F600}")),
    (r#""\xe9""#, None),
    (r#""\400""#, None),
    (r#""\ud800""#, None),
    (r#""\'""#, None),
    (r#""\q""#, None),
    (r#""\x4""#, None),
    (r#""a"b""#, None),
    ("abc", None),
];

#[test]
fn a_quoted_string_takes_backslash_escapes() {
    for (text, want) in UNQUOTED {
        assert_eq!(unquote(text).as_deref(), *want, "{text}");
    }
}

/// The result of a transcript that starts on two items and runs `commands`.
fn mouse_result(commands: &str) -> Done {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let comment = format!("size 40 8\nstart\n{commands}\n");
    let files = vec![
        (
            "config.toml".to_owned(),
            b"show_sidebar = false\nshow_detail = false\n".to_vec(),
        ),
        ("todo.txt".to_owned(), b"A\nB\n".to_vec()),
    ];
    script(&root, "t", &comment, files, false).0
}

#[test]
fn mouse_commands_find_text_and_take_cells() {
    let ok = [
        "clickon B\nstatus NORMAL",
        "clickon \"B\"\nshiftclickon A\nstatus VISUAL",
        "dblclickon A\nstatus INSERT",
        "rightclickon A\nstatus NORMAL",
        "dragon A B\nstatus VISUAL",
        "drag 5 1 5 2\nstatus VISUAL",
        "down 5 1\nmove 5 2\nup 5 2\nstatus VISUAL",
        "wheel down 5 2 3\nwheel up 5 2",
        "click 5 1\nshiftclick 5 2\ndblclick 5 2",
    ];
    for commands in ok {
        assert_eq!(mouse_result(commands), Ok(()), "{commands}");
    }
}

#[test]
fn mouse_commands_refuse_what_they_do_not_understand() {
    let bad = [
        "clickon nowhere",
        "clickon",
        "clickon A B",
        "click 1",
        "click x y",
        "drag 1 2 3",
        "wheel sideways 1 1",
        "wheel down 1",
        "wheel down 1 1 many",
        "dragon A",
    ];
    for commands in bad {
        assert!(mouse_result(commands).is_err(), "{commands}");
    }
}
