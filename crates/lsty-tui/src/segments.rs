//! The parts of a raw line that a person cannot change in INSERT mode,
//! and where editing starts. Positions count characters.

use lsty_core::{
    errs::lossy,
    todotxt::{Kind, Line},
};

use crate::field::{Field, Range};

/// Characters of the done mark and its space.
const DONE_MARK: usize = 2;

/// The read-only ranges of a line, the first position the cursor may
/// take, and where the description begins.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Segments {
    pub(crate) ro: Vec<Range>,
    pub(crate) floor: usize,
    pub(crate) desc: usize,
}

/// The characters of `text` before the byte offset `at`.
fn chars_before(text: &str, at: usize) -> usize {
    text.get(..at).map_or(0, |head| head.chars().count())
}

impl Segments {
    /// The segments of `text`: the done mark is fixed, and so is the
    /// first `id:` tag; the dates are text.
    pub(crate) fn of(text: &str) -> Self {
        let line = Line::parse(text);
        let mut ro = Vec::new();
        let mut floor = 0;
        if line.is_done() {
            ro.push((0, DONE_MARK));
            floor = DONE_MARK;
        }
        if let Some(tok) = line.tokens().find(|t| t.is_tag(b"id")) {
            let from = chars_before(text, tok.start);
            let len = lossy(tok.text).chars().count();
            ro.push((from, from + len));
        }
        Self {
            ro,
            floor,
            desc: chars_before(text, text.len() - line.description().len()),
        }
    }
}

/// The field of `text` with the cursor at `at`.
pub(crate) fn line_field(text: &str, at: usize) -> Field {
    let seg = Segments::of(text);
    Field::locked(text, seg.ro, seg.floor, at)
}

/// The field of `text` with the cursor at its end, before the `id:` that
/// ends it, as `append` puts text.
pub(crate) fn at_end(text: &str) -> Field {
    line_field(text, chars_before(text, Line::parse(text).text_end()))
}

/// The field of `text` with the cursor at the start of its description.
pub(crate) fn described(text: &str) -> Field {
    let seg = Segments::of(text);
    Field::locked(text, seg.ro, seg.floor, seg.desc)
}

/// `text` without the free text of its description, and the position
/// where that text began. A space keeps the tags apart from it,
/// and a tag that touches the one before it stays against it.
pub(crate) fn cleared(text: &str) -> (String, usize) {
    let seg = Segments::of(text);
    let line = Line::parse(text);
    let head: String = text.chars().take(seg.desc).collect();
    let mut kept: Vec<String> = Vec::new();
    let (mut first, mut end) = (None, 0);
    for tok in line.tokens() {
        if tok.kind != Kind::Word {
            let text = lossy(tok.text);
            match kept.last_mut() {
                Some(last) if tok.start == end => last.push_str(&text),
                _ => kept.push(text),
            }
            end = tok.start + tok.text.len();
        } else if first.is_none() {
            first = Some(kept.len());
        }
    }
    let split = first.unwrap_or(0);
    let (before, after) = kept.split_at(split);
    let mut before = before.join(" ");
    let mut after = after.join(" ");
    if !before.is_empty() {
        before.push(' ');
    }
    if !after.is_empty() {
        after.insert(0, ' ');
    }
    let at = head.chars().count() + before.chars().count();
    (format!("{head}{before}{after}"), at)
}
