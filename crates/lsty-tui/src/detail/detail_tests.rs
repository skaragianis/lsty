use lsty_core::theme::Slot;

use super::wrap_words;
use crate::text::Span;

/// Name, the words, the width and the lines.
type Wrap = (
    &'static str,
    &'static [&'static str],
    usize,
    &'static [&'static str],
);

const WRAPS: &[Wrap] = &[
    ("words join on a line", &["ab", "cd"], 8, &["ab cd"]),
    ("a line may fill the width", &["ab", "cd"], 5, &["ab cd"]),
    (
        "the break space is dropped",
        &["ab", "cd"],
        4,
        &["ab", "cd"],
    ),
    (
        "a word as wide as the line",
        &["abcd", "ef"],
        4,
        &["abcd", "ef"],
    ),
    (
        "a long word is cut",
        &["abcdefghij"],
        4,
        &["abcd", "efgh", "ij"],
    ),
    (
        "the cut rest takes a word",
        &["abcde", "f"],
        4,
        &["abcd", "e f"],
    ),
    (
        "wide clusters are not split",
        &["日本語"],
        4,
        &["日本", "語"],
    ),
    (
        "a wide cluster in one cell",
        &["日", "本"],
        1,
        &["日", "本"],
    ),
    (
        "a wide cluster in no cells",
        &["日", "本"],
        0,
        &["日", "本"],
    ),
    ("no words", &[], 4, &[]),
];

#[test]
fn words_break_into_lines_of_the_width() {
    for (name, words, w, want) in WRAPS {
        let words: Vec<_> =
            words.iter().map(|w| Span::new(*w, None)).collect();
        let lines: Vec<String> = wrap_words(&words, *w)
            .iter()
            .map(|line| line.iter().map(|s| s.text.as_ref()).collect())
            .collect();
        assert_eq!(&lines, want, "{name}");
    }
}

#[test]
fn a_word_cut_over_lines_keeps_its_slot() {
    let words = vec![
        Span::new("abcdef", Some(Slot::Muted)),
        Span::new("g", Some(Slot::Text)),
    ];
    let lines = wrap_words(&words, 4);
    let got: Vec<Vec<_>> = lines
        .iter()
        .map(|l| l.iter().map(|s| (s.text.as_ref(), s.slot)).collect())
        .collect();
    let (m, t) = (Some(Slot::Muted), Some(Slot::Text));
    assert_eq!(
        got,
        [vec![("abcd", m)], vec![("ef", m), (" ", t), ("g", t)]]
    );
}
