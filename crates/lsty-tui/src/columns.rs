//! The widths of the list's columns.

use std::collections::HashSet;

use crate::{
    rows::Row,
    text::{ending, width_of},
};

const MAX_DATE: usize = 10;
/// The share, in percent, of the cells from the text on that the text is
/// owed while the longest text of the list does not fit.
const TEXT_SHARE: usize = 60;
/// The greatest the owner column is wide, unless the ending that tells the
/// owners apart is wider.
const MAX_OWNER: usize = 12;
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
    pub(crate) owner: usize,
    /// The least the owner column narrows to: the least width at which
    /// the owners of the view still read differently (a lone owner does
    /// not narrow).
    pub(crate) owner_floor: usize,
    /// The widest state and duration of a claim; it is never cut.
    pub(crate) took: usize,
    /// `(A)` wide when any row has a priority, else 0.
    pub(crate) priority: usize,
}

/// The least width, up to `widest` (the widest owner), at which the
/// distinct `owners` (the empty one, of a human claim, is none) still
/// read differently when cut to their ending. A lone owner has nothing
/// to be told apart from, so it does not narrow: its floor is its width,
/// `MAX_OWNER` at most; owners that differ only at the front keep their
/// whole width. At `widest` every owner is whole, so a width is found.
pub(crate) fn owner_floor<'a>(
    owners: impl IntoIterator<Item = &'a str>,
    widest: usize,
) -> usize {
    let owners: HashSet<_> =
        owners.into_iter().filter(|o| !o.is_empty()).collect();
    let told_apart = |w: &usize| {
        let forms: HashSet<_> = owners.iter().map(|o| ending(o, *w)).collect();
        forms.len() == owners.len()
    };
    if owners.len() < 2 {
        return widest.min(MAX_OWNER);
    }
    (1..=widest).find(told_apart).unwrap_or(widest)
}

impl Natural {
    pub(crate) fn of(rows: &[Row]) -> Self {
        let mut widest = Self::default();
        for row in rows {
            widest.text = widest.text.max(row.text_width);
            widest.date = widest.date.max(width_of(&row.date.text));
            widest.owner = widest.owner.max(width_of(&row.owner.text));
            widest.took = widest.took.max(width_of(&row.took.text));
            widest.priority = widest
                .priority
                .max(PRIORITY * usize::from(row.priority.is_some()));
        }
        let owner_floor = owner_floor(
            rows.iter().map(|r| r.owner.text.as_str()),
            widest.owner,
        );
        Self {
            text: widest.text,
            date: widest.date.min(MAX_DATE),
            owner: widest.owner.min(owner_floor.max(MAX_OWNER)),
            owner_floor,
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
    /// The column narrows, by the cells the text lacks, down to the least
    /// width at which the owners still read differently.
    Narrow(Col),
    /// The column goes; the others start again from their full widths.
    Drop(Col),
}

/// The steps in the order they are taken, until the text has its share.
const STEPS: [Step; 4] = [
    Step::Narrow(|l| &mut l.owner),
    Step::Drop(|l| &mut l.owner),
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
            date: natural.date,
            owner: natural.owner,
            took: natural.took,
            priority: natural.priority,
        };
        let shared = avail.saturating_sub(full.lead());
        let owed = natural.text.min((shared * TEXT_SHARE).div_ceil(100));
        let room = avail - owed;
        let mut layout = full;
        for taken in 0..=STEPS.len() {
            layout = full.after(&STEPS[..taken], &natural, room);
            if layout.used() <= room {
                break;
            }
        }
        layout.text = avail.saturating_sub(layout.used());
        layout
    }

    /// The columns after `steps`: the columns that go are gone, and the
    /// others have narrowed to fit in `room`, if they can.
    fn after(
        mut self,
        steps: &[Step],
        natural: &Natural,
        room: usize,
    ) -> Self {
        for step in steps {
            if let Step::Drop(col) = step {
                *col(&mut self) = 0;
            }
        }
        for step in steps {
            let missing = self.used().saturating_sub(room);
            match *step {
                Step::Narrow(col) => {
                    let w = col(&mut self);
                    *w -= w.saturating_sub(natural.owner_floor).min(missing);
                }
                Step::Drop(_) => {}
            }
        }
        self
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
        let columns = [self.date, self.owner, self.took];
        columns
            .iter()
            .filter(|&&w| w > 0)
            .map(|w| 2 + w)
            .sum::<usize>()
            + self.lead()
    }
}
