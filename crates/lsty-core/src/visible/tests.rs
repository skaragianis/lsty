use std::borrow::Cow;

use super::{visible, visible_bytes};

/// Text, and what `visible` and `visible_bytes` make of it.
const ROWS: [(&str, &str); 12] = [
    ("plain", "plain"),
    ("a\u{0}b", "a\u{2400}b"),
    ("\u{1b}[31m", "\u{241b}[31m"),
    ("\u{7}", "\u{2407}"),
    ("\u{7f}", "\u{2421}"),
    ("\u{80}\u{9b}\u{9f}", "\u{fffd}\u{fffd}\u{fffd}"),
    ("a\tb", "a\u{2409}b"),
    ("a\nb", "a\u{240a}b"),
    ("a\r\n", "a\u{240d}\u{240a}"),
    ("買\u{1}", "買\u{2401}"),
    ("e\u{301}", "e\u{301}"),
    ("\u{a0}\u{a1}\u{c2}", "\u{a0}\u{a1}\u{c2}"),
];

#[test]
fn a_control_character_becomes_a_picture() {
    for (text, want) in ROWS {
        assert_eq!(visible(Cow::Borrowed(text)), want, "{text:?}");
    }
    assert!(matches!(visible("plain".into()), Cow::Borrowed(_)));
}

#[test]
fn bytes_draw_their_line_breaks_and_their_invalid_bytes() {
    for (text, want) in ROWS {
        let got = visible_bytes(text.as_bytes());
        assert_eq!(got, want.as_bytes(), "{text:?}");
    }
    assert!(matches!(visible_bytes(b"plain"), Cow::Borrowed(_)));
    assert!(matches!(visible_bytes("é買".as_bytes()), Cow::Borrowed(_)));
    let got = visible_bytes(b"caf\xe9\x1b\x9b\xff\n");
    assert_eq!(
        got,
        "caf\u{fffd}\u{241b}\u{fffd}\u{fffd}\u{240a}".as_bytes()
    );
}

#[test]
fn a_control_byte_is_found_at_any_place_of_a_long_line() {
    for at in 0..100 {
        for byte in [0u8, 0x1f, 0x7f] {
            let mut line = vec![b'a'; 100];
            line[at] = byte;
            let got = visible_bytes(&line);
            assert!(matches!(got, Cow::Owned(_)), "{byte:#x} at {at}");
            assert!(!got.contains(&byte), "{byte:#x} at {at}");
        }
        let mut line = vec![b'a'; 100];
        line[at] = 0x20;
        assert!(matches!(visible_bytes(&line), Cow::Borrowed(_)));
        line[at] = 0x7e;
        assert!(matches!(visible_bytes(&line), Cow::Borrowed(_)));
    }
}
