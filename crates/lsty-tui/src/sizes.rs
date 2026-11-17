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
    /// The entry at the top of the view and the cells of it cut at the
    /// top edge.
    pub(crate) cut: (usize, usize),
}

impl Sizes<'_> {
    /// A pane where every entry takes one row.
    pub(crate) const fn flat(len: usize, rows: usize) -> Self {
        Self {
            len,
            rows,
            heights: None,
            cut: (0, 0),
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
        let skip = self.skipped(top);
        let mut out = self.run(top, self.rows + skip);
        if let Some(first) = out.first_mut() {
            first.1 -= skip;
        }
        out
    }

    /// The cells of the entry at `top` that the view cuts.
    fn skipped(&self, top: usize) -> usize {
        if top == self.cut.0 { self.cut.1 } else { 0 }
    }

    /// The first entry of a view where entry `at` starts on line `line`,
    /// and the cells of it cut at the top edge. The line moves up when
    /// `at` would not fit, and down when entries above are out of view
    /// and the view would end short.
    pub(crate) fn anchored(&self, at: usize, line: usize) -> (usize, usize) {
        let climb = |line: usize| {
            let (mut top, mut used) = (at, 0);
            while used < line && top > 0 {
                top -= 1;
                used += self.height(top);
            }
            (top, used, line)
        };
        let line = line.min(self.rows.saturating_sub(self.height(at)));
        let (top, used, line) = climb(line);
        if top == 0 && used <= line {
            return (top, 0);
        }
        let below: usize =
            self.run(at, self.rows).iter().map(|&(_, n)| n).sum();
        let (top, used, line) = climb(line.max(self.rows - below));
        (top, used.saturating_sub(line))
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
    /// keeps entry `at` whole in view, and never past the last page. A
    /// view that cuts the top entry and shows `at` whole stays.
    pub(crate) fn window(&self, top: usize, at: usize) -> usize {
        if self.skipped(top) > 0 && self.seen(top, at) {
            return top;
        }
        top.max(self.first_fitting(at)).min(at).min(self.last_top())
    }

    /// `top` moved by `by`, never past the last page.
    pub(crate) fn scrolled(&self, top: usize, by: isize) -> usize {
        top.saturating_add_signed(by).min(self.last_top())
    }

    /// The number of entries from `top` on that are whole in view, at
    /// least 1 when there is one. A cut top entry counts, and its cut
    /// cells are out of view.
    pub(crate) fn whole(&self, top: usize) -> usize {
        self.reach(top, self.rows + self.skipped(top), true)
            .min(self.len.saturating_sub(top))
    }

    /// Whether entry `at` is whole in the view that starts at `top`.
    pub(crate) fn seen(&self, top: usize, at: usize) -> bool {
        let room =
            (self.rows + self.skipped(top)).saturating_sub(self.height(at));
        top <= at && self.above(at, room) <= top
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
