use std::path::{Path, PathBuf};

use super::FileLink;

fn resolve(value: &str, dir: &str, home: &str) -> PathBuf {
    FileLink::new(value.as_bytes()).resolve(Path::new(dir), Path::new(home))
}

#[test]
fn resolve_rows() {
    let rows = [
        (
            "02.24 relative",
            "docs/x.pdf",
            "/w/list",
            "",
            "/w/list/docs/x.pdf",
        ),
        (
            "02.25 tilde path",
            "~/x.pdf",
            "",
            "/home/u",
            "/home/u/x.pdf",
        ),
        ("02.25 bare tilde", "~", "", "/home/u", "/home/u"),
        ("02.26 absolute cleaned", "/etc/../x", "/w/list", "", "/x"),
        ("02.26 parent relative", "../x", "/w/list", "", "/w/x"),
        ("tilde user is relative", "~u/x", "/w", "/h", "/w/~u/x"),
        ("dot slashes", "./a//b/.", "/w", "", "/w/a/b"),
        ("above root", "/../x", "", "", "/x"),
        ("relative dir", "x", "w", "", "w/x"),
        ("relative dir escapes", "../../x", "w", "", "../x"),
        ("empty dir", "x", "", "", "x"),
        ("tilde with empty home", "~/x", "/w", "", "/x"),
    ];
    for (name, value, dir, home, want) in rows {
        assert_eq!(resolve(value, dir, home), Path::new(want), "{name}");
    }
}

#[test]
fn encoded_slash_is_a_path_separator() {
    assert_eq!(resolve("a%2Fb", "/w", ""), Path::new("/w/a/b"));
    assert_eq!(resolve("%2Fa", "/w", ""), Path::new("/a"));
    assert_eq!(resolve("%7E/a", "/w", "/h"), Path::new("/h/a"));
}

#[test]
fn encode_round_trips() {
    let input = "my file:v2 100%.pdf";
    let got = FileLink::encode(input);
    assert_eq!(got, b"my%20file%3Av2%20100%25.pdf", "02.27");
    assert_eq!(FileLink::decode(&got), input.as_bytes(), "02.27 back");
}

/// Name, encoded, decoded.
type DecodeRow = (&'static str, &'static [u8], &'static [u8]);

#[test]
fn decode_rows() {
    let rows: [DecodeRow; 8] = [
        ("space", b"a%20b", b"a b"),
        ("bad hex", b"a%2Gb", b"a%2Gb"),
        ("trailing percent", b"100%", b"100%"),
        ("one digit at end", b"a%2", b"a%2"),
        ("two digits at end", b"a%41", b"aA"),
        ("lower hex", b"%e9", b"\xe9"),
        ("upper hex", b"%E9", b"\xe9"),
        ("percent then escape", b"%%41", b"%A"),
    ];
    for (name, raw, want) in rows {
        assert_eq!(FileLink::decode(raw), want, "{name}");
    }
}

#[test]
fn decode_borrows_when_nothing_changes() {
    assert!(matches!(
        FileLink::decode(b"plain"),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn encode_decode_any_bytes() {
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(FileLink::decode(&FileLink::encode(&all)), all.as_slice());
}
