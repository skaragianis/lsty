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
