use std::sync::Arc;

use super::Line;
use super::line::{Eol, Kind};
use super::parse::{parse_file_within, parse_line_within};
use super::tokens::{LIMIT, Narrow, Token};

fn token(start: usize, end: usize, colon: usize) -> Token {
    Token {
        kind: Kind::Tag,
        start,
        end,
        colon,
    }
}

#[test]
fn offsets_up_to_u32_max_are_narrow() {
    let max = LIMIT;
    let fits = token(max - 3, max, max - 1);
    assert_eq!(Narrow::of(fits, LIMIT).map(Narrow::widen), Some(fits));
    assert_eq!(Narrow::of(token(0, max + 1, 1), LIMIT), None);
    assert_eq!(Narrow::of(token(0, 8, 2), 7), None);
    assert_eq!(
        Narrow::of(token(0, 7, 2), 7).map(Narrow::widen),
        Some(token(0, 7, 2))
    );
}

#[test]
fn a_wide_line_reads_as_a_narrow_one() {
    let raw: Arc<[u8]> =
        Arc::from(&b"(A) 2026-01-02 call +mum @home due:2026-03-04 x"[..]);
    let narrow = Line::parse(raw.to_vec());
    for limit in [0, 5, 20, 30] {
        let wide = parse_line_within(&raw, limit);
        assert_eq!(wide, narrow);
        assert_eq!(wide.tags(), narrow.tags());
        assert_eq!(wide.token_count(), 5);
        let views: Vec<_> = wide.tokens().collect();
        assert_eq!(views, narrow.tokens().collect::<Vec<_>>());
    }
}

#[test]
fn a_line_shows_and_compares_only_its_own_tokens() {
    let file = parse_file_within(b"a b\nc +d e:f\na b\n", LIMIT);
    let [first, second, third] = file.lines.as_slice() else {
        panic!("three lines");
    };
    assert_eq!(first, third);
    assert_ne!(first, second);
    let mut alone = Line::parse("a b");
    alone.set_eol(Eol::Lf);
    assert_eq!(*first, alone);
    let shown = format!("{second:?}");
    assert_eq!(shown.matches("kind:").count(), 3, "{shown}");
    let texts: Vec<_> = second.tokens().map(|t| t.text).collect();
    assert_eq!(texts, [&b"c"[..], b"+d", b"e:f"]);
}
