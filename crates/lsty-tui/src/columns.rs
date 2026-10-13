//! The widths of the list's columns.

use crate::{rows::Row, text::width_of};

const MAX_TAGS: usize = 24;
const MAX_DATE: usize = 10;
const MAX_OWNER: usize = 20;
/// The least the owner column narrows to: two letters and an ellipsis.
const MIN_OWNER: usize = 3;
/// The least the tags column narrows to: a short tag and its ellipsis.
const MIN_TAGS: usize = 8;
/// The share, in percent, of the cells from the text on that the text is
/// owed while the longest text of the list does not fit.
const TEXT_SHARE: usize = 60;
/// The width of `(A)`.
const PRIORITY: usize = 3;
/// The least a gutter is wide.
pub(crate) const MIN_GUTTER: usize = 2;

/// The widest value of each optional column, within its limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Natural {
    /// The widest item text of the view, in cells.
    pub(crate) text: usize,
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
            widest.text = widest.text.max(row.text_width);
            widest.tags = widest.tags.max(row.tags_width);
            widest.date = widest.date.max(width_of(&row.date.text));
            widest.owner = widest.owner.max(width_of(&row.owner.text));
            widest.took = widest.took.max(width_of(&row.took.text));
            widest.priority = widest
                .priority
                .max(PRIORITY * usize::from(row.priority.is_some()));
        }
        Self {
            text: widest.text,
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

/// The width of a column that gives way to the text.
type Col = fn(&mut Layout) -> &mut usize;

/// One way the columns give way to the text.
#[derive(Clone, Copy)]
enum Step {
    /// The column narrows, by the cells the text lacks, down to a floor.
    Narrow(Col, usize),
    /// The column goes; the others start again from their full widths.
    Drop(Col),
}

/// The steps in the order they are taken, until the text has its share.
const STEPS: [Step; 6] = [
    Step::Narrow(|l| &mut l.owner, MIN_OWNER),
    Step::Narrow(|l| &mut l.tags, MIN_TAGS),
    Step::Drop(|l| &mut l.owner),
    Step::Drop(|l| &mut l.tags),
    Step::Drop(|l| &mut l.took),
    Step::Drop(|l| &mut l.date),
];

impl Layout {
    /// Gives the text column the rest of `avail` cells. The text is owed
    /// `TEXT_SHARE` percent of the cells from its start on, or its own
    /// longest width when that is less. While it has less, the steps of
    /// `STEPS` are taken in order; a column that goes lets the others
    /// start again from their full widths, and each narrows only by the
    /// cells still missing. After the last step the text has all there is.
    pub(crate) fn new(natural: Natural, gutter: usize, avail: usize) -> Self {
        let full = Self {
            gutter,
            text: 0,
            tags: natural.tags,
            date: natural.date,
            owner: natural.owner,
            took: natural.took,
            priority: natural.priority,
        };
        let shared = avail.saturating_sub(full.lead());
        let owed = natural.text.min((shared * TEXT_SHARE).div_ceil(100));
        let mut layout = full;
        for taken in 0..=STEPS.len() {
            layout = full.after(&STEPS[..taken], owed, avail);
            if layout.used() + owed <= avail {
                break;
            }
        }
        layout
    }

    /// The columns after `steps`: the columns that go are gone, and the
    /// others have narrowed so that the text has `owed` cells, if they
    /// can.
    fn after(mut self, steps: &[Step], owed: usize, avail: usize) -> Self {
        for step in steps {
            if let Step::Drop(col) = step {
                *col(&mut self) = 0;
            }
        }
        for step in steps {
            if let Step::Narrow(col, floor) = step {
                let missing = (self.used() + owed).saturating_sub(avail);
                let w = col(&mut self);
                *w -= w.saturating_sub(*floor).min(missing);
            }
        }
        self.text = avail.saturating_sub(self.used());
        self
    }

    /// The cells before the text.
    fn lead(&self) -> usize {
        let mut lead = 1 + 1;
        if self.priority > 0 {
            lead += self.priority + 1;
        }
        if self.gutter > 0 {
            lead += self.gutter + 1;
        }
        lead
    }

    /// The cells of everything but the text.
    fn used(&self) -> usize {
        let columns = [self.tags, self.date, self.owner, self.took];
        columns
            .iter()
            .filter(|&&w| w > 0)
            .map(|w| 2 + w)
            .sum::<usize>()
            + self.lead()
    }
}
