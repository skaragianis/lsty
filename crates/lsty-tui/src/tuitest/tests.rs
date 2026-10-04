use std::path::Path;

use super::run::script;

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
