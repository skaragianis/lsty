use std::sync::Arc;

use super::{
    file::File,
    line::{Eol, Kind, Line, Priority, Stamp},
    name::trim_name,
    share,
    tokens::{Builder, Token, Tokens},
};

/// The UTF-8 byte order mark.
pub(crate) const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub(super) fn parse_file(data: &[u8]) -> File {
    let (bom, data) = match data.strip_prefix(&BOM) {
        Some(rest) => (true, rest),
        None => (false, data),
    };
    let (breaks, gaps, quoting) = scan(data);
    // A line has one token more than it has gaps at most, unless a word
    // splits into names; the vector then grows.
    let mut tokens = Builder::new(gaps + breaks + 1);
    let mut scanned = Vec::with_capacity(breaks + 1);
    for chunk in data.split_inclusive(|b| *b == b'\n') {
        let (raw, eol) = match chunk.strip_suffix(b"\n") {
            Some(raw) => match raw.strip_suffix(b"\r") {
                Some(raw) => (raw, Eol::CrLf),
                None => (raw, Eol::Lf),
            },
            None => (chunk, Eol::None),
        };
        let raw = share(raw);
        let head = Head::of(&raw);
        let first = tokens.len();
        if quoting {
            quoting_words(&raw, head.desc_start).for_each(|t| tokens.push(t));
        } else {
            words(&raw, head.desc_start).for_each(|t| tokens.push(t));
        }
        let len = tokens.len() - first;
        scanned.push((raw, eol, head, first, len));
    }
    let store = tokens.finish();
    let lines = scanned
        .into_iter()
        .map(|(raw, eol, head, first, len)| {
            head.line(raw, eol, store.tokens(first, len))
        })
        .collect();
    File { lines, bom }
}

/// The number of newlines, and of spaces and tabs, in `data`, and
/// whether it holds a backtick. Counted in runs short enough for a byte,
/// which the compiler turns into vector instructions; a run is a whole
/// number of vectors, so that only the end of `data` is counted a byte
/// at a time.
fn scan(data: &[u8]) -> (usize, usize, bool) {
    /// Bytes per run: the largest multiple of 64 a byte counts.
    const RUN: usize = 3 * 64;
    let (mut breaks, mut gaps, mut ticks) = (0, 0, 0_u8);
    for run in data.chunks(RUN) {
        let (mut b, mut g) = (0_u8, 0_u8);
        for byte in run {
            b += u8::from(*byte == b'\n');
            g += u8::from(*byte == b' ') + u8::from(*byte == b'\t');
            ticks |= u8::from(*byte == b'`');
        }
        breaks += usize::from(b);
        gaps += usize::from(g);
    }
    (breaks, gaps, ticks > 0)
}

pub(super) fn parse_line(raw: &Arc<[u8]>) -> Line {
    let head = Head::of(raw);
    let tokens = Tokens::alone(quoting_words(raw, head.desc_start));
    head.line(Arc::clone(raw), Eol::None, tokens)
}

/// The done mark, priority and dates at the start of a line.
struct Head {
    blank: bool,
    done: bool,
    priority: Option<Priority>,
    completed: Option<Stamp>,
    created: Option<Stamp>,
    desc_start: usize,
}

impl Head {
    /// The line of `raw`, with this head, its ending and its tokens.
    const fn line(self, raw: Arc<[u8]>, eol: Eol, tokens: Tokens) -> Line {
        Line {
            blank: self.blank,
            desc_start: self.desc_start,
            eol,
            done: self.done,
            priority: self.priority,
            completed: self.completed,
            created: self.created,
            tokens,
            raw,
        }
    }

    /// The head of `raw`; a blank line has none, and its description
    /// starts at its end.
    fn of(raw: &[u8]) -> Self {
        let blank = Line::is_blank_text(raw);
        let rest = |pos: usize| raw.get(pos..).unwrap_or_default();
        let done = raw.starts_with(b"x ");
        let mut pos = if done { 2 } else { 0 };
        let priority = cut_priority(rest(pos));
        pos += priority.map_or(0, |_| 4);
        // A done line's first date is its completion date, and only then
        // can a creation date follow.
        let first = cut_date(rest(pos));
        pos += first.map_or(0, |_| 11);
        let (completed, created) = if done {
            let second = first.and_then(|_| cut_date(rest(pos)));
            pos += second.map_or(0, |_| 11);
            (first, second)
        } else {
            (None, first)
        };
        Self {
            blank,
            done,
            priority,
            completed,
            created,
            desc_start: if blank { raw.len() } else { pos },
        }
    }
}

fn cut_priority(s: &[u8]) -> Option<Priority> {
    match s {
        [b'(', letter, b')', b' ', ..] => Priority::new(*letter),
        _ => None,
    }
}

/// A date and the space after it, at the start of `s`.
fn cut_date(s: &[u8]) -> Option<Stamp> {
    let head: &[u8; 10] = s.get(..10)?.try_into().ok()?;
    (s.get(10) == Some(&b' ') && is_date(head)).then_some(Stamp(*head))
}

/// Whether `s` is `YYYY-MM-DD` and a real calendar date.
pub(crate) fn is_date(s: &[u8]) -> bool {
    let [y1, y2, y3, y4, b'-', m1, m2, b'-', d1, d2] = *s else {
        return false;
    };
    let digits = [y1, y2, y3, y4, m1, m2, d1, d2];
    if !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let num = |a: u8, b: u8| u32::from(a - b'0') * 10 + u32::from(b - b'0');
    let year = num(y1, y2) * 100 + num(y3, y4);
    let (month, day) = (num(m1, m2), num(d1, d2));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// Whether `b` separates tokens: a space or a tab.
pub const fn is_gap(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// The tokens of `raw` from byte `from` on: runs of bytes that are not
/// spaces or tabs.
fn words(raw: &[u8], from: usize) -> Words<'_> {
    Words { raw, at: from }
}

#[derive(Clone)]
struct Words<'a> {
    raw: &'a [u8],
    /// Where the search for the next token starts.
    at: usize,
}

impl Iterator for Words<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        let rest = self.raw.get(self.at..)?;
        let start = self.at + rest.iter().position(|b| !is_gap(*b))?;
        let rest = self.raw.get(start..).unwrap_or_default();
        let len = word_len(rest);
        self.at = start + len;
        Some(classify(rest.get(..len).unwrap_or_default(), start))
    }
}

/// The length of the token at the start of `rest`: up to the next gap or,
/// in a word that starts with `+` or `@`, up to a later `+` or `@`
/// followed by a letter, a digit or a non-ASCII byte, where a new name
/// starts.
fn word_len(rest: &[u8]) -> usize {
    let is_sigil = |b: Option<&u8>| matches!(b, Some(b'+' | b'@'));
    let starts = is_sigil(rest.first());
    let letter = |b: Option<&u8>| {
        b.is_some_and(|b| !b.is_ascii() || b.is_ascii_alphanumeric())
    };
    let ends = |(i, b): (usize, &u8)| {
        is_gap(*b)
            || (starts
                && i > 0
                && is_sigil(Some(b))
                && letter(rest.get(i + 1)))
    };
    rest.iter().enumerate().position(ends).unwrap_or(rest.len())
}

/// `words`, but a word that lies in or touches a quoted span is a plain
/// word with its `colon` set to 1.
fn quoting_words(
    raw: &[u8],
    from: usize,
) -> impl Iterator<Item = Token> + Clone + '_ {
    let mut span = next_span(raw, from);
    words(raw, from).map(move |token| {
        while let Some((_, end)) = span.filter(|&(_, end)| end <= token.start)
        {
            span = next_span(raw, end);
        }
        match span {
            Some((open, _)) if open < token.end => Token {
                kind: Kind::Word,
                colon: 1,
                ..token
            },
            _ => token,
        }
    })
}

/// Whether a cut of `desc` just before byte `at` falls inside a quoted
/// span, so that it would split the span.
pub fn inside_span(desc: &[u8], at: usize) -> bool {
    std::iter::successors(next_span(desc, 0), |&(_, end)| next_span(desc, end))
        .any(|(open, close)| open < at && at < close)
}

/// The first span of `raw` from byte `from` on that backticks quote, as
/// a byte range that includes the backticks. A backtick opens a span and
/// the next one closes it; two in a row quote nothing, and a backtick
/// with no later one is plain.
fn next_span(raw: &[u8], from: usize) -> Option<(usize, usize)> {
    let tick = |from: usize| {
        Some(from + raw.get(from..)?.iter().position(|b| *b == b'`')?)
    };
    let mut open = tick(from)?;
    loop {
        let close = tick(open + 1)?;
        if close > open + 1 {
            return Some((open, close + 1));
        }
        open = tick(close + 1)?;
    }
}

/// The first matching row of the token table wins.
fn classify(word: &[u8], start: usize) -> Token {
    let end = start + word.len();
    let mut token = Token {
        kind: Kind::Word,
        start,
        end,
        colon: 0,
    };
    let colon = word.iter().position(|b| *b == b':');
    let one_colon =
        |c: usize| !word.get(c + 1..).unwrap_or_default().contains(&b':');
    // A value that starts with `//` is a URL, which is text; only
    // `file:` keeps the form.
    let url = |c: usize| {
        word.get(c + 1..c + 3) == Some(b"//") && word.get(..c) != Some(b"file")
    };
    let named = || !trim_name(word.get(1..).unwrap_or_default()).is_empty();
    match (word.first(), colon) {
        (Some(b'+'), _) if named() => token.kind = Kind::Project,
        (Some(b'@'), _) if named() => token.kind = Kind::Context,
        (_, Some(c))
            if c > 0 && c + 1 < word.len() && one_colon(c) && !url(c) =>
        {
            token.kind = Kind::Tag;
            token.colon = start + c;
        }
        _ => {}
    }
    token
}
