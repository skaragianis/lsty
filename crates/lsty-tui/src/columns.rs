//! The widths of the list's columns.

use crate::{rows::Row, text::width_of};

const MAX_DATE: usize = 10;
/// The greatest the id column is wide; a longer id is cut.
const MAX_ID: usize = 8;
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
    /// The widest task text of the view, in cells.
    pub(crate) text: usize,
    pub(crate) date: usize,
    pub(crate) id: usize,
    /// The widest state and duration of a claim; it is never cut.
    pub(crate) duration: usize,
    /// `(A)` wide when any row has a priority, else 0.
    pub(crate) priority: usize,
}

impl Natural {
    pub(crate) fn of(rows: &[Row]) -> Self {
        let mut widest = Self::default();
        for row in rows {
            widest.text = widest.text.max(row.text_width);
            widest.date = widest.date.max(width_of(&row.date.text));
            widest.id = widest.id.max(width_of(&row.id_cell.text));
            widest.duration =
                widest.duration.max(width_of(&row.duration.text));
            widest.priority = widest
                .priority
                .max(PRIORITY * usize::from(row.priority.is_some()));
        }
        Self {
            text: widest.text,
            date: widest.date.min(MAX_DATE),
            id: widest.id.min(MAX_ID),
            duration: widest.duration,
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
    pub(crate) date: usize,
    pub(crate) id: usize,
    pub(crate) duration: usize,
    pub(crate) priority: usize,
}

/// The width of a column that gives way to the text.
type Col = fn(&mut Layout) -> &mut usize;

/// The columns that give way to the text, in the order they go, until the
/// text has its share.
const ORDER: [Col; 3] = [|l| &mut l.duration, |l| &mut l.date, |l| &mut l.id];

impl Layout {
    /// Gives the text column the rest of `avail` cells. The text is owed
    /// `TEXT_SHARE` percent of the cells from its start on, or its own
    /// longest width when that is less. While it has less, the columns of
    /// `ORDER` go in order. After the last the text has all there is.
    pub(crate) fn new(natural: Natural, gutter: usize, avail: usize) -> Self {
        let mut layout = Self {
            gutter,
            text: 0,
            date: natural.date,
            id: natural.id,
            duration: natural.duration,
            priority: natural.priority,
        };
        let shared = avail.saturating_sub(layout.lead());
        let owed = natural.text.min((shared * TEXT_SHARE).div_ceil(100));
        let room = avail - owed;
        for col in ORDER {
            if layout.used() <= room {
                break;
            }
            *col(&mut layout) = 0;
        }
        layout.text = avail.saturating_sub(layout.used());
        layout
    }

    /// The cells before the text.
    pub(crate) fn lead(&self) -> usize {
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
        let columns = [self.date, self.id, self.duration];
        columns
            .iter()
            .filter(|&&w| w > 0)
            .map(|w| 2 + w)
            .sum::<usize>()
            + self.lead()
    }
}
