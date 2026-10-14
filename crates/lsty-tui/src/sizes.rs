//! The view of a pane whose entries take several rows each: which
//! entries are shown, where the view may start, how far a move goes.

/// A pane of `len` entries, `rows` cells tall, where entry `i` takes
/// `height(i)` cells (at least 1). Every move of a view is worked out
/// here, from the entries near the view only.
#[derive(Clone, Copy)]
pub(crate) struct Sizes<'a> {
    pub(crate) len: usize,
    pub(crate) rows: usize,
    /// How many cells entry `i` takes; `None` when every entry takes one.
    pub(crate) heights: Option<&'a dyn Fn(usize) -> usize>,
}

impl Sizes<'_> {
    /// A pane where every entry takes one row.
    pub(crate) const fn flat(len: usize, rows: usize) -> Self {
        Self {
            len,
            rows,
            heights: None,
        }
    }

    /// The cells entry `i` takes.
    pub(crate) fn height(&self, i: usize) -> usize {
        self.heights.map_or(1, |height| height(i))
    }

    /// The entries from `from` on that start in the first `cells`, with
    /// the cells each shows: the last may be cut.
    fn run(&self, from: usize, cells: usize) -> Vec<(usize, usize)> {
        let mut left = cells;
        let mut out =
            Vec::with_capacity(cells.min(self.len.saturating_sub(from)));
        for i in from..self.len {
            if left == 0 {
                break;
            }
            let shown = self.height(i).min(left);
            out.push((i, shown));
            left -= shown;
        }
        out
    }

    /// The entries the pane shows from `top` on, with the cells each
    /// shows: the last may be cut by the bottom.
    pub(crate) fn shown(&self, top: usize) -> Vec<(usize, usize)> {
        self.run(top, self.rows)
    }

    /// The first entry of a view with `cells` before entry `at`.
    pub(crate) fn above(&self, at: usize, cells: usize) -> usize {
        if self.heights.is_none() {
            return at.saturating_sub(cells);
        }
        let mut used = 0;
        let mut top = at;
        while top > 0 && used + self.height(top - 1) <= cells {
            top -= 1;
            used += self.height(top);
        }
        top
    }

    /// The first entry from which entries up to `end` fit, or `end`
    /// when it does not fit alone.
    fn first_fitting(&self, end: usize) -> usize {
        self.above(end, self.rows.saturating_sub(self.height(end)))
    }

    /// The first entry of the last page.
    pub(crate) fn last_top(&self) -> usize {
        self.len
            .checked_sub(1)
            .map_or(0, |last| self.first_fitting(last))
    }

    /// The first of the entries to show: `top` moved the least that
    /// keeps entry `at` whole in view, and never past the last page.
    pub(crate) fn window(&self, top: usize, at: usize) -> usize {
        top.max(self.first_fitting(at)).min(at).min(self.last_top())
    }

    /// `top` moved by `by`, never past the last page.
    pub(crate) fn scrolled(&self, top: usize, by: isize) -> usize {
        top.saturating_add_signed(by).min(self.last_top())
    }

    /// The number of entries from `top` on that are whole in view, at
    /// least 1 when there is one.
    pub(crate) fn whole(&self, top: usize) -> usize {
        self.reach(top, self.rows, true)
            .min(self.len.saturating_sub(top))
    }

    /// Whether entry `at` is whole in the view that starts at `top`.
    pub(crate) fn seen(&self, top: usize, at: usize) -> bool {
        top <= at && self.first_fitting(at) <= top
    }

    /// The most entries, but one at least, that fit whole in `cells`,
    /// going down from entry `from` or up from the entry before it.
    pub(crate) fn reach(
        &self,
        from: usize,
        cells: usize,
        down: bool,
    ) -> usize {
        let n = if down {
            let whole =
                |&&(i, shown): &&(usize, usize)| shown == self.height(i);
            self.run(from, cells).iter().filter(whole).count()
        } else {
            from - self.above(from, cells)
        };
        n.max(1)
    }
}
