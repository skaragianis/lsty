//! Text in terminal cells: widths, cutting and pieces of text that each
//! have a slot.

use std::borrow::Cow;

use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{sizes::Sizes, styles::Styles};

/// The ellipsis that ends a value that was cut.
pub(crate) const ELLIPSIS: &str = "…";
/// The horizontal rule of a frame.
pub(crate) const RULE: &str = "─";

/// Spaces to borrow from.
const SPACES: &str = concat!(
    "                                                                ",
    "                                                                ",
    "                                                                ",
    "                                                                ",
);

/// A piece of text in the style of one slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Span<'a> {
    pub(crate) text: Cow<'a, str>,
    pub(crate) slot: Option<Slot>,
}

impl<'a> Span<'a> {
    pub(crate) fn new(
        text: impl Into<Cow<'a, str>>,
        slot: Option<Slot>,
    ) -> Self {
        Self {
            text: visible(text.into()),
            slot,
        }
    }

    /// `n` blanks in `slot`.
    pub(crate) fn blanks(n: usize, slot: Option<Slot>) -> Self {
        Self {
            text: blanks(n),
            slot,
        }
    }

    pub(crate) fn cells(&self) -> usize {
        self.text.width()
    }
}

/// The picture of a control character that the terminal would act on:
/// U+2400 plus its code for C0 (U+2421 for DEL), U+FFFD for C1.
fn picture(c: char) -> char {
    let code = u32::from(c);
    let shown = match code {
        0..=0x1f => char::from_u32(0x2400 + code),
        0x7f => Some('\u{2421}'),
        _ => None,
    };
    shown.unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// `text` with every control character drawn as a one-cell picture, so
/// that nothing raw reaches the terminal and widths agree with drawing.
/// Text without control characters is returned as it is.
pub(crate) fn visible(text: Cow<'_, str>) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return text;
    }
    let shown = text
        .chars()
        .map(|c| if c.is_control() { picture(c) } else { c })
        .collect();
    Cow::Owned(shown)
}

/// `n` spaces, borrowed when there are few.
pub(crate) fn blanks(n: usize) -> Cow<'static, str> {
    match SPACES.get(..n) {
        Some(spaces) => Cow::Borrowed(spaces),
        None => Cow::Owned(" ".repeat(n)),
    }
}

/// The first of `len` entries to show when `rows` fit: `top` moved the
/// least that keeps entry `at` in view, and never past the last page.
pub(crate) fn window(top: usize, at: usize, len: usize, rows: usize) -> usize {
    Sizes::flat(len, rows).window(top, at)
}

/// `top` moved by `by`, never past the last page of `len` entries when
/// `rows` fit.
pub(crate) fn scrolled(
    top: usize,
    by: isize,
    len: usize,
    rows: usize,
) -> usize {
    Sizes::flat(len, rows).scrolled(top, by)
}

/// The width of all the spans in cells.
pub(crate) fn width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::cells).sum()
}

/// The longest start of `s` made of whole grapheme clusters that fits in
/// `w` cells.
pub(crate) fn prefix(s: &str, w: usize) -> &str {
    let mut used = 0;
    let mut end = 0;
    for (at, grapheme) in s.grapheme_indices(true) {
        used += grapheme.width();
        if used > w {
            break;
        }
        end = at + grapheme.len();
    }
    s.get(..end).unwrap_or_default()
}

/// The longest end of `s` made of whole grapheme clusters that fits in
/// `w` cells.
pub(crate) fn suffix(s: &str, w: usize) -> &str {
    let mut used = 0;
    let mut start = s.len();
    for (at, grapheme) in s.grapheme_indices(true).rev() {
        used += grapheme.width();
        if used > w {
            break;
        }
        start = at;
    }
    s.get(start..).unwrap_or_default()
}

/// `s` in `w` cells: whole, or its end behind an ellipsis.
pub(crate) fn ending(s: &str, w: usize) -> String {
    if width_of(s) <= w {
        return s.to_owned();
    }
    format!("{ELLIPSIS}{}", suffix(s, w.saturating_sub(1)))
}

/// `s` in `w` cells: whole, or its start before an ellipsis.
pub(crate) fn head(s: &str, w: usize) -> String {
    if width_of(s) <= w {
        return s.to_owned();
    }
    format!("{}{ELLIPSIS}", prefix(s, w.saturating_sub(1)))
}

/// The start of `text`, `n` cells at most, in the same kind of string.
fn shorten(text: Cow<'_, str>, n: usize) -> Cow<'_, str> {
    match text {
        Cow::Borrowed(s) => Cow::Borrowed(prefix(s, n)),
        Cow::Owned(s) => Cow::Owned(prefix(&s, n).to_owned()),
    }
}

/// Extends `spans` with blanks in `slot` up to `w` cells.
pub(crate) fn pad(spans: &mut Vec<Span<'_>>, w: usize, slot: Option<Slot>) {
    let gap = w.saturating_sub(width(spans));
    if gap > 0 {
        spans.push(Span::blanks(gap, slot));
    }
}

/// Shortens `spans` to `w` cells, ending with an ellipsis when it cuts
/// (one that already ends the cut serves).
pub(crate) fn fit(spans: Vec<Span<'_>>, w: usize) -> Vec<Span<'_>> {
    if w == 0 {
        return Vec::new();
    }
    if width(&spans) <= w {
        return spans;
    }
    let mut out = cut(spans, w - 1);
    let ends = out
        .iter()
        .rfind(|s| !s.text.is_empty())
        .is_some_and(|s| s.text.ends_with(ELLIPSIS));
    if let Some(last) = out.last_mut().filter(|_| !ends) {
        last.text.to_mut().push_str(ELLIPSIS);
    }
    out
}

/// Keeps the first `w` cells of `spans`, with no ellipsis.
pub(crate) fn cut(spans: Vec<Span<'_>>, mut w: usize) -> Vec<Span<'_>> {
    let mut out = Vec::with_capacity(spans.len());
    for span in spans {
        let cells = span.cells();
        if cells <= w {
            w -= cells;
            out.push(span);
            continue;
        }
        out.push(Span::new(shorten(span.text, w), span.slot));
        break;
    }
    out
}

/// Writes `spans` from column `x` of row `y`, never past `limit` cells.
pub(crate) fn put(
    buf: &mut Buffer,
    at: (u16, u16),
    limit: usize,
    spans: &[Span<'_>],
    styles: &Styles,
) {
    let (mut x, y) = at;
    let end = usize::from(x) + limit;
    for span in spans {
        let room = end.saturating_sub(usize::from(x));
        if room == 0 {
            return;
        }
        let style = styles.get(span.slot);
        x = buf.set_stringn(x, y, &span.text, room, style).0;
    }
}

/// The cells `text` takes.
pub(crate) fn width_of(text: &str) -> usize {
    width(&[Span::new(text, None)])
}

/// `left`, `title`, dashes up to `w` cells, `right`.
pub(crate) fn titled(
    left: &str,
    title: &str,
    right: &str,
    w: usize,
) -> String {
    let used = width_of(left) + width_of(right) + width_of(title);
    format!(
        "{left}{title}{}{right}",
        RULE.repeat(w.saturating_sub(used))
    )
}
