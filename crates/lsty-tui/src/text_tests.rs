use std::borrow::Cow;

use lsty_core::visible::visible;

use crate::text::{
    Span, cut, ending, fit, head, pad, prefix, scrolled, width,
};

fn plain(text: &str) -> Vec<Span<'_>> {
    vec![Span::new(text, None)]
}

#[test]
fn fit_to_narrow_widths() {
    const ROWS: [(usize, usize); 4] = [(0, 0), (1, 1), (4, 4), (20, 8)];
    for (w, want) in ROWS {
        assert_eq!(width(&fit(plain("abcdefgh"), w)), want, "width {w}");
    }
}

#[test]
fn fit_ends_a_cut_value_with_an_ellipsis() {
    let spans = fit(plain("abcdefgh"), 5);
    let text: String = spans.iter().map(|s| s.text.as_ref()).collect();
    assert_eq!(text, "abcd…");
}

#[test]
fn fit_keeps_whole_spans_and_cuts_the_one_that_overflows() {
    let spans = vec![Span::new("ab", None), Span::new("cdef", None)];
    let text: String = fit(spans, 5).iter().map(|s| s.text.as_ref()).collect();
    assert_eq!(text, "abcd…");
}

#[test]
fn wide_characters_count_double_and_are_not_split() {
    assert_eq!(width(&plain("買い物")), 6);
    assert_eq!(prefix("買い物", 5), "買い");
    assert_eq!(prefix("買い物", 1), "");
    let text: String = fit(plain("買い物 +home"), 6)
        .iter()
        .map(|s| s.text.as_ref())
        .collect();
    assert_eq!(text, "買い…");
}

#[test]
fn cut_has_no_ellipsis() {
    let text: String = cut(plain("abcdef"), 3)
        .iter()
        .map(|s| s.text.as_ref())
        .collect();
    assert_eq!(text, "abc");
}

#[test]
fn pad_extends_to_the_width() {
    let mut spans = plain("ab");
    pad(&mut spans, 5, None);
    assert_eq!(width(&spans), 5);
    pad(&mut spans, 3, None);
    assert_eq!(width(&spans), 5);
    pad(&mut spans, 5, None);
    assert_eq!(spans.len(), 2);
}

#[test]
fn long_gaps_are_spaces_too() {
    assert_eq!(width(&[Span::blanks(300, None)]), 300);
}

#[test]
fn control_characters_are_drawn_as_one_cell_pictures() {
    const ROWS: [(&str, &str); 6] = [
        ("plain", "plain"),
        ("a\u{0}b", "a\u{2400}b"),
        ("\u{1b}[31m", "\u{241b}[31m"),
        ("\u{1f}\u{7f}", "\u{241f}\u{2421}"),
        ("\u{80}\u{9f}", "\u{fffd}\u{fffd}"),
        ("買\u{1}", "買\u{2401}"),
    ];
    for (text, want) in ROWS {
        let got = visible(Cow::Borrowed(text));
        assert_eq!(got, want, "{text:?}");
        assert_eq!(width(&[Span::new(text, None)]), width(&plain(want)));
    }
    assert!(matches!(visible(Cow::Borrowed("plain")), Cow::Borrowed(_)));
}

#[test]
fn scrolling_stays_between_the_first_and_the_last_page() {
    // (top, by, len, want) with four rows.
    const ROWS: [(usize, isize, usize, usize); 5] = [
        (0, -3, 10, 0),
        (2, 3, 10, 5),
        (5, 3, 10, 6),
        (0, 3, 3, 0),
        (6, -3, 10, 3),
    ];
    for (top, by, len, want) in ROWS {
        assert_eq!(scrolled(top, by, len, 4), want, "{top} {by}");
    }
}

#[test]
fn ending_keeps_the_end_of_a_cut_value_behind_an_ellipsis() {
    const ROWS: [(&str, usize, &str); 7] = [
        ("agent-591a", 10, "agent-591a"),
        ("agent-591a", 5, "…591a"),
        ("agent-591a", 4, "…91a"),
        ("agent-591a", 2, "…a"),
        ("agent-591a", 1, "…"),
        // a wide character is never split
        ("日本語x", 4, "…語x"),
        ("日本語x", 3, "…x"),
    ];
    for (text, w, want) in ROWS {
        assert_eq!(ending(text, w), want, "{text} in {w}");
    }
}

#[test]
fn head_keeps_the_start_of_a_cut_value_before_an_ellipsis() {
    const ROWS: [(&str, usize, &str); 5] = [
        ("@agent", 6, "@agent"),
        ("@agent", 4, "@ag…"),
        ("@agent", 2, "@…"),
        ("日本語x", 4, "日…"),
        ("日本語x", 6, "日本…"),
    ];
    for (text, w, want) in ROWS {
        assert_eq!(head(text, w), want, "{text} in {w}");
    }
}

#[test]
fn a_cut_after_an_ellipsis_adds_no_second_one() {
    let spans = vec![
        Span::new("+alpha…", None),
        Span::new(" ", None),
        Span::new("@p…", None),
    ];
    let got: String = fit(spans, 8).iter().map(|s| s.text.as_ref()).collect();
    assert_eq!(got, "+alpha…");
}
