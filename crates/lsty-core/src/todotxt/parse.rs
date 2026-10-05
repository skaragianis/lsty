use std::sync::Arc;

use super::file::File;
use super::line::{Eol, Kind, Line, Priority, Stamp, Token};

/// The UTF-8 byte order mark.
pub(super) const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub(super) fn parse_file(data: &[u8]) -> File {
    let (bom, data) = match data.strip_prefix(&BOM) {
        Some(rest) => (true, rest),
        None => (false, data),
    };
    let lines = data
        .split_inclusive(|b| *b == b'\n')
        .map(|chunk| {
            let (raw, eol) = match chunk.strip_suffix(b"\n") {
                Some(raw) => match raw.strip_suffix(b"\r") {
                    Some(raw) => (raw, Eol::CrLf),
                    None => (raw, Eol::Lf),
                },
                None => (chunk, Eol::None),
            };
            let mut line = parse_line(&Arc::from(raw));
            line.eol = eol;
            line
        })
        .collect();
    File { lines, bom }
}

pub(super) fn parse_line(raw: &Arc<[u8]>) -> Line {
    let mut line = Line {
        blank: Line::is_blank_text(raw),
        desc_start: raw.len(),
        raw: Arc::clone(raw),
        eol: Eol::None,
        done: false,
        priority: None,
        completed: None,
        created: None,
        tokens: Vec::new(),
    };
    if !line.blank {
        let mut pos = 0;
        let rest = |pos: usize| raw.get(pos..).unwrap_or_default();

        if cut_done(rest(pos)) {
            line.done = true;
            pos += 2;
        }
        if let Some(p) = cut_priority(rest(pos)) {
            line.priority = Some(p);
            pos += 4;
        }
        // A done line's first date is its completion date, and only then
        // can a creation date follow.
        let first = cut_date(rest(pos));
        pos += first.map_or(0, |_| 11);
        if line.done {
            let second = first.and_then(|_| cut_date(rest(pos)));
            pos += second.map_or(0, |_| 11);
            line.completed = first;
            line.created = second;
        } else {
            line.created = first;
        }
        line.desc_start = pos;
        line.tokens = tokenise(raw, pos);
    }
    line
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

fn tokenise(raw: &[u8], from: usize) -> Vec<Token> {
    let is_sep = |b: &u8| matches!(b, b' ' | b'\t');
    let mut tokens = Vec::new();
    let mut i = from;
    while i < raw.len() {
        let rest = raw.get(i..).unwrap_or_default();
        let skip = rest.iter().take_while(|b| is_sep(b)).count();
        let len = rest.iter().skip(skip).take_while(|b| !is_sep(b)).count();
        let start = i + skip;
        let end = start + len;
        if len > 0 {
            tokens.push(classify(
                raw.get(start..end).unwrap_or_default(),
                start,
            ));
        }
        i = end.max(i + 1);
    }
    tokens
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
    let last_colon = word.iter().rposition(|b| *b == b':');
    match (word.first(), colon) {
        (Some(b'+'), _) if word.len() > 1 => token.kind = Kind::Project,
        (Some(b'@'), _) if word.len() > 1 => token.kind = Kind::Context,
        (_, Some(c))
            if c > 0 && c + 1 < word.len() && last_colon == Some(c) =>
        {
            token.kind = Kind::Tag;
            token.colon = start + c;
        }
        _ => {}
    }
    token
}
