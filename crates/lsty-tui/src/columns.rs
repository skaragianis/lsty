//! The widths of the list's columns.

use crate::{reldate::claim_in, rows::Row, text::width_of};

const MAX_TAGS: usize = 24;
const MAX_DATE: usize = 14;
const MAX_CLAIM: usize = 20;
const MIN_TEXT: usize = 10;
/// The least the tags column narrows to: a short tag and its ellipsis.
const MIN_TAGS: usize = 8;
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
    /// The widest state and duration of a claim: the least it narrows to.
    pub(crate) duration: usize,
}

impl Natural {
    pub(crate) fn of(rows: &[Row]) -> Self {
        let mut widest = Self::default();
        for row in rows {
            widest.tags = widest.tags.max(row.tags_width);
            widest.date = widest.date.max(width_of(&row.date.text));
            widest.claim = widest.claim.max(width_of(&row.claim.text));
            let tail = width_of(&claim_in(&row.claim.text, 0));
            widest.duration = widest.duration.max(tail);
        }
        Self {
            tags: widest.tags.min(MAX_TAGS),
            date: widest.date.min(MAX_DATE),
            claim: widest.claim.min(MAX_CLAIM),
            duration: widest.duration.min(MAX_CLAIM),
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
    /// Gives the text column the rest of `avail` cells. While it would be
    /// too narrow, the claim, then the tags column narrow by the cells
    /// missing; only when that is not enough is a column dropped, from
    /// the right, and narrowing tried again.
    pub(crate) fn new(natural: Natural, gutter: usize, avail: usize) -> Self {
        let mut layout = Self {
            gutter,
            text: 0,
            tags: natural.tags,
            date: natural.date,
            claim: natural.claim,
        };
        let drops: [fn(&mut Self); 4] =
            [|_| {}, |l| l.claim = 0, |l| l.date = 0, |l| l.tags = 0];
        for drop in drops {
            drop(&mut layout);
            let narrowed = layout.narrowed(natural.duration, avail);
            if narrowed.text >= MIN_TEXT {
                return narrowed;
            }
        }
        layout.set_text(avail);
        layout
    }

    /// This layout with the claim (down to `duration`) and then the tags
    /// column (down to `MIN_TAGS`) narrowed by the cells the text lacks.
    fn narrowed(mut self, duration: usize, avail: usize) -> Self {
        let mut short = (self.used() + MIN_TEXT).saturating_sub(avail);
        for (w, floor) in
            [(&mut self.claim, duration), (&mut self.tags, MIN_TAGS)]
        {
            let give = w.saturating_sub(floor).min(short);
            *w -= give;
            short -= give;
        }
        self.set_text(avail);
        self
    }

    fn set_text(&mut self, avail: usize) {
        self.text = avail.saturating_sub(self.used());
    }

    /// The cells of everything but the text.
    fn used(&self) -> usize {
        let mut used = 1 + 1 + PRIORITY + 1;
        if self.gutter > 0 {
            used += self.gutter + 1;
        }
        for w in [self.tags, self.date, self.claim] {
            if w > 0 {
                used += 2 + w;
            }
        }
        used
    }
}
