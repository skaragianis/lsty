//! The `--expect` guard: "is item NR still the one I mean?".

use std::fmt::Display;

use super::refs::Target;
use crate::{
    errs::{Code, Error, Field, Ref, Result, Source, lossy, quote},
    list::{Item, Snapshot},
    query::contains,
};

/// The text a caller expects an item to have, and how strictly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Expect {
    text: Vec<u8>,
    exact: bool,
}

impl Expect {
    /// A guard for `text`. An exact guard holds when the text equals the
    /// item's text; a loose one when it is a fragment of the raw line or
    /// of the text.
    pub(crate) fn new(text: impl Into<Vec<u8>>, exact: bool) -> Self {
        Self {
            text: text.into(),
            exact,
        }
    }

    fn holds(&self, snap: &Snapshot, item: &Item) -> bool {
        let line = snap.line(item);
        let want = squash_bytes(&self.text);
        let text = squash_bytes(&line.text());
        if self.exact {
            return text == want;
        }
        contains(&squash_bytes(line.raw()), &want, <[u8]>::eq)
            || contains(&text, &want, <[u8]>::eq)
    }
}

/// Collapses every run of whitespace to one space and trims both ends; a
/// byte that is not UTF-8 is part of a word.
pub(super) fn squash_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut spaced = Vec::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            let c = if c.is_whitespace() { ' ' } else { c };
            spaced.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
        spaced.extend_from_slice(chunk.invalid());
    }
    let words: Vec<&[u8]> = spaced
        .split(|b| *b == b' ')
        .filter(|w| !w.is_empty())
        .collect();
    words.join(&b' ')
}

/// Checks the guard of `target` against the line it found: `None` for a
/// blank line or no line at all.
pub(super) fn check(
    snap: &Snapshot,
    item: Option<&Item>,
    target: &Target,
    place: (Source, i64),
) -> Result<()> {
    let Some(expect) = target.expect() else {
        return Ok(());
    };
    match item {
        Some(item) if expect.holds(snap, item) => Ok(()),
        _ => Err(mismatch(snap, item, target, expect, place)),
    }
}

fn mismatch(
    snap: &Snapshot,
    item: Option<&Item>,
    target: &Target,
    expect: &Expect,
    place: (Source, i64),
) -> Error {
    let handle = target.handle();
    let Some(item) = item else {
        let expected = lossy(&expect.text);
        let message = format!(
            "item {handle} does not exist, expected {}",
            quote(&expect.text)
        );
        return Error::new(Code::ExpectMismatch, message)
            .with_place(place.1, place.0)
            .with_field("expected", Field::Str(expected))
            .with_field("actual", Field::Null);
    };
    mismatch_at(
        snap.ref_of(item),
        handle,
        &snap.line(item).text(),
        &expect.text,
    )
}

/// The refusal for an item whose `actual` text is not the `expected`.
pub(super) fn mismatch_at(
    item: Ref,
    handle: impl Display,
    actual: &[u8],
    expected: &[u8],
) -> Error {
    let message = format!(
        "item {handle} is {}, expected {}",
        quote(actual),
        quote(expected)
    );
    let (actual, expected) = (lossy(actual), lossy(expected));
    Error::new(Code::ExpectMismatch, message)
        .with_item(item)
        .with_field("expected", Field::Str(expected))
        .with_field("actual", Field::Str(actual))
}
