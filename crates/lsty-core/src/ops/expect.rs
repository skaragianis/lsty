//! The `--expect` guard: "is item NR still the one I mean?".

use super::refs::Target;
use crate::errs::{Code, Error, Field, Result};
use crate::list::{Item, Snapshot};

/// The text a caller expects an item to have, and how strictly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    text: String,
    exact: bool,
}

impl Expect {
    /// A guard for `text`. An exact guard holds when the text equals the
    /// item's text; a loose one when it is a fragment of the raw line or
    /// of the text.
    pub fn new(text: impl Into<String>, exact: bool) -> Self {
        Self {
            text: text.into(),
            exact,
        }
    }

    /// The text as the caller gave it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the text must equal the item's text.
    pub const fn is_exact(&self) -> bool {
        self.exact
    }

    fn holds(&self, snap: &Snapshot, item: &Item) -> bool {
        let line = snap.line(item);
        let want = squash(&self.text).into_bytes();
        let text = squash_bytes(&line.text());
        if self.exact {
            return text == want;
        }
        contains(&squash_bytes(line.raw()), &want) || contains(&text, &want)
    }
}

/// Collapses every run of whitespace to one space and trims both ends.
pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// [`squash`] for bytes: a byte that is not UTF-8 is part of a word.
pub(super) fn squash_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut gap = false;
    let word = |out: &mut Vec<u8>, gap: &mut bool, part: &[u8]| {
        if *gap && !out.is_empty() {
            out.push(b' ');
        }
        *gap = false;
        out.extend_from_slice(part);
    };
    for chunk in bytes.utf8_chunks() {
        let mut buf = [0u8; 4];
        for c in chunk.valid().chars() {
            if c.is_whitespace() {
                gap = true;
            } else {
                word(&mut out, &mut gap, c.encode_utf8(&mut buf).as_bytes());
            }
        }
        if !chunk.invalid().is_empty() {
            word(&mut out, &mut gap, chunk.invalid());
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// Checks the guard of `target` against the line it found: `None` for a
/// blank line.
pub(super) fn check(
    snap: &Snapshot,
    item: Option<&Item>,
    target: &Target,
) -> Result<()> {
    let Some(expect) = target.expect() else {
        return Ok(());
    };
    match item {
        Some(item) if expect.holds(snap, item) => Ok(()),
        _ => Err(mismatch(snap, item, target, expect)),
    }
}

/// The error for a guarded reference that names no line at all.
pub(super) fn missing(snap: &Snapshot, target: &Target) -> Option<Error> {
    let expect = target.expect()?;
    Some(mismatch(snap, None, target, expect))
}

fn mismatch(
    snap: &Snapshot,
    item: Option<&Item>,
    target: &Target,
    expect: &Expect,
) -> Error {
    let wanted = format!("expected \"{}\"", expect.text);
    let handle = target.handle();
    let Some(item) = item else {
        let message = format!("item {handle} does not exist, {wanted}");
        return Error::new(Code::ExpectMismatch, message)
            .with_place(target.nr(), target.source())
            .with_field("expected", Field::Str(expect.text.clone()))
            .with_field("actual", Field::Null);
    };
    let actual = crate::errs::lossy(&snap.line(item).text());
    let message = format!("item {handle} is \"{actual}\", {wanted}");
    Error::new(Code::ExpectMismatch, message)
        .with_item(snap.ref_of(item))
        .with_field("expected", Field::Str(expect.text.clone()))
        .with_field("actual", Field::Str(actual))
}
