use std::path::Path;

use super::run::{script, unquote};

/// A todo line that is not UTF-8, as the file keeps it.
fn transcript(expected: &[u8]) -> Vec<u8> {
    let mut text = b"start\nfile todo.txt kept\n-- todo.txt --\ncaf".to_vec();
    text.extend_from_slice(&[0xe9, b'\n']);
    text.extend_from_slice(b"-- kept --\n");
    text.extend_from_slice(expected);
    text
}

#[test]
fn a_file_that_is_not_text_is_compared_byte_for_byte() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (done, _) = script(&root, "t", &transcript(b"caf\xe9\n"), false);
    assert_eq!(done, Ok(()));
    // U+FFFD is not the byte it replaces.
    let (done, _) =
        script(&root, "t", &transcript("caf\u{fffd}\n".as_bytes()), false);
    assert!(done.is_err());
}

#[test]
fn updating_keeps_the_bytes_of_the_sections() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (done, updated) = script(&root, "t", &transcript(b"x\n"), true);
    assert_eq!(done, Ok(()));
    let want = transcript(b"caf\xe9\n");
    assert_eq!(updated, Some(want));
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

/// A transcript that starts on two items and runs `commands`.
fn mouse_script(commands: &str) -> Vec<u8> {
    let mut text = b"size 40 8\nstart\n".to_vec();
    text.extend_from_slice(commands.as_bytes());
    text.extend_from_slice(b"\n-- config.toml --\nshow_sidebar = false\n");
    text.extend_from_slice(b"show_detail = false\n-- todo.txt --\nA\nB\n");
    text
}

fn mouse_result(commands: &str) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    script(&root, "t", &mouse_script(commands), false).0
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
