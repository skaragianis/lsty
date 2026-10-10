use std::sync::Arc;

use super::{
    File, Line,
    line::{Eol, Kind},
    tokens::{Narrow, Token},
};

fn token(start: usize, end: usize, colon: usize) -> Token {
    Token {
        kind: Kind::Tag,
        start,
        end,
        colon,
    }
}

#[test]
fn offsets_up_to_u32_max_are_kept_and_larger_ones_stop_there() {
    let max = u32::MAX as usize;
    let fits = token(max - 3, max, max - 1);
    assert_eq!(Narrow::of(fits).widen(), fits);
    let past = token(max + 1, max + 5, max + 2);
    assert_eq!(Narrow::of(past).widen(), token(max, max, max));
}

#[test]
fn a_line_shows_and_compares_only_its_own_tokens() {
    let file = File::parse(b"a b\nc +d e:f\na b\n");
    let [first, second, third] = file.lines.as_slice() else {
        panic!("three lines");
    };
    assert_eq!(first, third);
    assert_ne!(first, second);
    let mut alone = Line::parse("a b");
    alone.eol = Eol::Lf;
    assert_eq!(*first, alone);
    let shown = format!("{second:?}");
    assert_eq!(shown.matches("kind:").count(), 3, "{shown}");
    let texts: Vec<_> = second.tokens().map(|t| t.text).collect();
    assert_eq!(texts, [&b"c"[..], b"+d", b"e:f"]);
}

#[test]
fn a_long_line_alone_reads_as_in_a_file() {
    for count in [0, 1, 31, 32, 33, 70] {
        let words: Vec<String> =
            (0..count).map(|i| format!("w{i} +p{i} k{i}:v")).collect();
        let text = words.join(" ");
        let file = File::parse(format!("{text}\n").as_bytes());
        let [in_file] = file.lines.as_slice() else {
            panic!("one line");
        };
        let raw: Arc<[u8]> = Arc::from(text.as_bytes());
        let mut alone = Line::parse(&raw);
        assert_eq!(alone.tokens().len(), count * 3);
        alone.eol = Eol::Lf;
        assert_eq!(alone, *in_file, "{count} words");
    }
}
