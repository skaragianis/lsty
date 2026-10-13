//! The widths of the list's columns.

use crate::{rows::Row, text::width_of};

const MAX_TAGS: usize = 24;
const MAX_DATE: usize = 14;
const MAX_OWNER: usize = 20;
/// The least the owner column narrows to: two letters and an ellipsis.
const MIN_OWNER: usize = 3;
const MIN_TEXT: usize = 10;
/// The least the tags column narrows to: a short tag and its ellipsis.
const MIN_TAGS: usize = 8;
/// The width of `(A)`.
const PRIORITY: usize = 3;
/// The least a gutter is wide.
pub(crate) const MIN_GUTTER: usize = 2;

/// The widest value of each optional column, within its limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Natural {
    pub(crate) tags: usize,
    pub(crate) date: usize,
    pub(crate) owner: usize,
    /// The widest state and duration of a claim; it is never cut.
    pub(crate) took: usize,
    /// `(A)` wide when any row has a priority, else 0.
    pub(crate) priority: usize,
}

impl Natural {
    pub(crate) fn of(rows: &[Row]) -> Self {
        let mut widest = Self::default();
        for row in rows {
            widest.tags = widest.tags.max(row.tags_width);
            widest.date = widest.date.max(width_of(&row.date.text));
            widest.owner = widest.owner.max(width_of(&row.owner.text));
            widest.took = widest.took.max(width_of(&row.took.text));
            widest.priority = widest
                .priority
                .max(PRIORITY * usize::from(row.priority.is_some()));
        }
        Self {
            tags: widest.tags.min(MAX_TAGS),
            date: widest.date.min(MAX_DATE),
            owner: widest.owner.min(MAX_OWNER),
            took: widest.took,
            priority: widest.priority,
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
    pub(crate) owner: usize,
    pub(crate) took: usize,
    pub(crate) priority: usize,
}

impl Layout {
    /// Gives the text column the rest of `avail` cells. While it would be
    /// too narrow, the owner narrows by the cells missing (and goes when
    /// it would be under `MIN_OWNER`), then the tags column; only when
    /// that is not enough is a column dropped, from the right, and
    /// narrowing tried again.
    pub(crate) fn new(natural: Natural, gutter: usize, avail: usize) -> Self {
        let mut layout = Self {
            gutter,
            text: 0,
            tags: natural.tags,
            date: natural.date,
            owner: natural.owner,
            took: natural.took,
            priority: natural.priority,
        };
        let drops: [fn(&mut Self); 4] = [
            |_| {},
            |l| (l.owner, l.took) = (0, 0),
            |l| l.date = 0,
            |l| l.tags = 0,
        ];
        for drop in drops {
            drop(&mut layout);
            let narrowed = layout.narrowed(avail);
            if narrowed.text >= MIN_TEXT {
                return narrowed;
            }
        }
        layout.set_text(avail);
        layout
    }

    /// This layout with the owner and then the tags column (down to
    /// `MIN_TAGS`) narrowed by the cells the text lacks.
    fn narrowed(mut self, avail: usize) -> Self {
        let short = |l: &Self| (l.used() + MIN_TEXT).saturating_sub(avail);
        let missing = short(&self);
        self.owner = match self.owner.saturating_sub(missing) {
            c if missing > 0 && c < MIN_OWNER => 0,
            c => c,
        };
        self.tags -= (self.tags.saturating_sub(MIN_TAGS)).min(short(&self));
        self.set_text(avail);
        self
    }

    fn set_text(&mut self, avail: usize) {
        self.text = avail.saturating_sub(self.used());
    }

    /// The cells of everything but the text.
    fn used(&self) -> usize {
        let mut used = 1 + 1;
        if self.priority > 0 {
            used += self.priority + 1;
        }
        if self.gutter > 0 {
            used += self.gutter + 1;
        }
        for w in [self.tags, self.date, self.owner, self.took] {
            if w > 0 {
                used += 2 + w;
            }
        }
        used
    }
}
