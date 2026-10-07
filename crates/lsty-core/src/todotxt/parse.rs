use std::sync::Arc;

use super::file::File;
use super::line::{Eol, Kind, Line, Priority, Stamp};
use super::tokens::{self, Builder, Token, Tokens};

/// The UTF-8 byte order mark.
pub(crate) const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub(super) fn parse_file(data: &[u8]) -> File {
    parse_file_within(data, tokens::LIMIT)
}

/// [`parse_file`], keeping token offsets up to `limit` narrow.
pub(super) fn parse_file_within(data: &[u8], limit: usize) -> File {
    let (bom, data) = match data.strip_prefix(&BOM) {
        Some(rest) => (true, rest),
        None => (false, data),
    };
    let mut lines = Vec::new();
    // About one token in six bytes, which most lists exceed.
    let mut tokens = Builder::new(limit, data.len() / 6);
    for chunk in data.split_inclusive(|b| *b == b'\n') {
        let (raw, eol) = match chunk.strip_suffix(b"\n") {
            Some(raw) => match raw.strip_suffix(b"\r") {
                Some(raw) => (raw, Eol::CrLf),
                None => (raw, Eol::Lf),
            },
            None => (chunk, Eol::None),
        };
        let mut line = scan(Arc::from(raw), &mut tokens);
        line.eol = eol;
        lines.push(line);
    }
    let store = tokens.finish();
    for line in &mut lines {
        line.tokens.place(&store);
    }
    File { lines, bom }
}

pub(super) fn parse_line(raw: &Arc<[u8]>) -> Line {
    parse_line_within(raw, tokens::LIMIT)
}

/// [`parse_line`], keeping token offsets up to `limit` narrow.
pub(super) fn parse_line_within(raw: &Arc<[u8]>, limit: usize) -> Line {
    let mut tokens = Builder::new(limit, 0);
    let mut line = scan(Arc::clone(raw), &mut tokens);
    line.tokens.place(&tokens.finish());
    line
}

/// Parses `raw`, adding its tokens to `tokens`; the line's tokens are
/// placed once `tokens` is finished.
fn scan(raw: Arc<[u8]>, tokens: &mut Builder) -> Line {
    let from = tokens.len();
    let blank = Line::is_blank_text(&raw);
    let head = if blank {
        Head::blank(&raw)
    } else {
        Head::of(&raw)
    };
    if !blank {
        tokenise(&raw, head.desc_start, tokens);
    }
    Line {
        blank,
        desc_start: head.desc_start,
        eol: Eol::None,
        done: head.done,
        priority: head.priority,
        completed: head.completed,
        created: head.created,
        tokens: Tokens::pending(from, tokens.len() - from),
        raw,
    }
}

/// The done mark, priority and dates at the start of a line.
struct Head {
    done: bool,
    priority: Option<Priority>,
    completed: Option<Stamp>,
    created: Option<Stamp>,
    desc_start: usize,
}

impl Head {
    const fn blank(raw: &[u8]) -> Self {
        Self {
            done: false,
            priority: None,
            completed: None,
            created: None,
            desc_start: raw.len(),
        }
    }

    fn of(raw: &[u8]) -> Self {
        let rest = |pos: usize| raw.get(pos..).unwrap_or_default();
        let done = cut_done(raw);
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
            done,
            priority,
            completed,
            created,
            desc_start: pos,
        }
    }
}

fn cut_done(s: &[u8]) -> bool {
    s.starts_with(b"x ")
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
    if s.get(10) == Some(&b' ') && is_date(head) {
        Some(Stamp(*head))
    } else {
        None
    }
}

/// Whether `s` is `YYYY-MM-DD` and a real calendar date.
pub(crate) fn is_date(s: &[u8; 10]) -> bool {
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

fn tokenise(raw: &[u8], from: usize, tokens: &mut Builder) {
    let is_sep = |b: &u8| matches!(b, b' ' | b'\t');
    let mut i = from;
    while let Some(skip) =
        raw.get(i..).and_then(|r| r.iter().position(|b| !is_sep(b)))
    {
        let start = i + skip;
        let rest = raw.get(start..).unwrap_or_default();
        let len = rest.iter().position(is_sep).unwrap_or(rest.len());
        tokens.push(classify(rest.get(..len).unwrap_or_default(), start));
        i = start + len;
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
    match (word.first(), colon) {
        (Some(b'+'), _) if word.len() > 1 => token.kind = Kind::Project,
        (Some(b'@'), _) if word.len() > 1 => token.kind = Kind::Context,
        (_, Some(c)) if c > 0 && c + 1 < word.len() && one_colon(c) => {
            token.kind = Kind::Tag;
            token.colon = start + c;
        }
        _ => {}
    }
    token
}
