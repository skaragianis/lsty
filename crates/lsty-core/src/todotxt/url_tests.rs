use super::url_in;

/// A word, and the URL in it.
type Row = (&'static str, Option<&'static str>);

const ROWS: [Row; 20] = [
    ("https://x.org", Some("https://x.org")),
    ("http://h:80/p?q=1#f", Some("http://h:80/p?q=1#f")),
    ("x-y+z.1://a", Some("x-y+z.1://a")),
    // Before the scheme.
    ("(https://x.org)", Some("https://x.org")),
    ("see:https://x.org", Some("https://x.org")),
    ("<https://x.org>", Some("https://x.org")),
    // After the URL.
    ("https://x.org/a.,;:!?", Some("https://x.org/a")),
    ("https://x.org/a'.\"", Some("https://x.org/a")),
    ("https://x.org/a)]}", Some("https://x.org/a")),
    (
        "https://en.wikipedia.org/wiki/Rust_(language)",
        Some("https://en.wikipedia.org/wiki/Rust_(language)"),
    ),
    (
        "(https://en.wikipedia.org/wiki/Rust_(language)).",
        Some("https://en.wikipedia.org/wiki/Rust_(language)"),
    ),
    // No URL.
    ("https://", None),
    ("https://.,)", None),
    ("://x", None),
    ("1a://x", Some("a://x")),
    ("-https://x.org", Some("https://x.org")),
    ("1.https://x.org", Some("https://x.org")),
    ("1-2://x", None),
    ("mailto:a@b", None),
    ("https://a/\x1b[0m", None),
];

#[test]
fn the_url_in_a_word() {
    for (word, want) in ROWS {
        let got = url_in(word.as_bytes()).map(String::from_utf8_lossy);
        assert_eq!(got.as_deref(), want, "{word}");
    }
}

#[test]
fn bytes_that_are_not_utf8_stay_as_they_are() {
    assert_eq!(url_in(b"https://a/\xff."), Some(&b"https://a/\xff"[..]));
}
