//! The short forms of the projects and contexts of the tags column.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use lsty_core::{
    errs::{Source, lossy},
    list::Snapshot,
    todotxt::Kind,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::text::{Span, head, width_of};

/// The width and the least width of each project and context of the list,
/// by their text.
pub(crate) type Forms<'a> = HashMap<&'a [u8], (usize, usize)>;

/// The least width of each name, sign included: the width of its shortest
/// start that no other name starts with (at least the sign and one cell),
/// plus the ellipsis; its whole width when that is not narrower, which
/// includes a name that starts another. The names are those of the todo
/// file, as the sidebar lists them, and those of the done file when the
/// list shows `archived` items.
pub(crate) fn forms(snap: &Snapshot, archived: bool) -> Forms<'_> {
    let names: HashSet<&[u8]> = snap
        .items()
        .iter()
        .filter(|item| archived || item.source() == Source::Todo)
        .flat_map(|item| snap.line(item).tokens())
        .filter(|t| matches!(t.kind, Kind::Project | Kind::Context))
        .map(|t| t.text)
        .collect();
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort_unstable();
    let mut forms = HashMap::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let near = |at: Option<usize>| {
            let other = at.and_then(|j| names.get(j)).copied().unwrap_or(&[]);
            name.iter().zip(other).take_while(|(a, b)| a == b).count()
        };
        // the sign alone is no start: one more cell of the name shows
        let shared = near(i.checked_sub(1)).max(near(Some(i + 1))).max(1);
        let text = lossy(name);
        let width = width_of(&text);
        // a bad byte is longer in the text than in the file: no start of it
        let least = if text.as_bytes() == *name {
            width.min(start_width(&text, shared) + 1)
        } else {
            width
        };
        forms.insert(*name, (width, least));
    }
    forms
}

/// The cells of the start of `name` that reaches past its first `shared`
/// bytes, whole grapheme clusters only.
fn start_width(name: &str, shared: usize) -> usize {
    let mut cells = 0;
    for (at, grapheme) in name.grapheme_indices(true) {
        cells += width_of(grapheme);
        if at + grapheme.len() > shared {
            break;
        }
    }
    cells
}

/// The cells an entry `width` wide shows when no entry shows more than
/// `limit`: never less than its `least` form.
pub(crate) fn shown(width: usize, least: usize, limit: usize) -> usize {
    width.min(limit.max(least))
}

/// The entries among `spans` (every other one, from the first) shortened
/// to what `shown` says for `tags`, their widths and least widths.
pub(crate) fn shorten(
    spans: &mut [Span<'_>],
    tags: &[(usize, usize)],
    limit: usize,
) {
    for (span, &(width, least)) in spans.iter_mut().step_by(2).zip(tags) {
        let cells = shown(width, least, limit);
        if cells < width {
            span.text = Cow::Owned(head(&span.text, cells));
        }
    }
}
