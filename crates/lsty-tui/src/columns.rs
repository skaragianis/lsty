//! The widths of the list's columns.

use crate::rows::Row;
use crate::text::width_of;

const MAX_TAGS: usize = 24;
const MAX_DATE: usize = 14;
const MAX_CLAIM: usize = 16;
const MIN_TEXT: usize = 10;
/// The width of `(A)`.
pub(crate) const PRIORITY: usize = 3;
/// The least a gutter is wide.
pub(crate) const MIN_GUTTER: usize = 2;

/// The widest value of each optional column, within its limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Natural {
    pub(crate) tags: usize,
    pub(crate) date: usize,
    pub(crate) claim: usize,
}

impl Natural {
    pub(crate) fn of(rows: &[Row]) -> Self {
        let mut widest = Self::default();
        for row in rows {
            widest.tags = widest.tags.max(row.tags_width);
            widest.date = widest.date.max(width_of(&row.date.text));
            widest.claim = widest.claim.max(width_of(&row.claim.text));
        }
        Self {
            tags: widest.tags.min(MAX_TAGS),
            date: widest.date.min(MAX_DATE),
            claim: widest.claim.min(MAX_CLAIM),
        }
    }
}

/// The width of each column of the list pane. A column of width 0 is
/// left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) gutter: usize,
    pub(crate) text: usize,
    pub(crate) tags: usize,
    pub(crate) date: usize,
    pub(crate) claim: usize,
}

impl Layout {
    /// Gives the text column the rest of `avail` cells, dropping columns
    /// from the right while it would be too narrow.
    pub(crate) fn new(natural: Natural, gutter: usize, avail: usize) -> Self {
        let mut layout = Self {
            gutter,
            text: 0,
            tags: natural.tags,
            date: natural.date,
            claim: natural.claim,
        };
        layout.set_text(avail);
        for drop in 0..3 {
            if layout.text >= MIN_TEXT {
                break;
            }
            match drop {
                0 => layout.claim = 0,
                1 => layout.date = 0,
                _ => layout.tags = 0,
            }
            layout.set_text(avail);
        }
        layout
    }

    fn set_text(&mut self, avail: usize) {
        let mut used = 1 + 1 + PRIORITY + 1;
        if self.gutter > 0 {
            used += self.gutter + 1;
        }
        for w in [self.tags, self.date, self.claim] {
            if w > 0 {
                used += 2 + w;
            }
        }
        self.text = avail.saturating_sub(used);
    }
}
