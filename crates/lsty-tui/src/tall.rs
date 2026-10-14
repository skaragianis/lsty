//! The items list with `wrap` on: how many rows each item takes, what
//! the later rows show, and which items are in view.

use std::rc::Rc;

use crate::{
    columns::Layout,
    layout::BORDER,
    model::Model,
    rows::{Inline, text_spans},
    sizes::Sizes,
    text::{Span, fit},
    wrap::wrap_words,
};

/// The most rows an item takes; the last one ends with an ellipsis when
/// the text goes on.
pub(crate) const MAX_ROWS: usize = 3;
/// How far in the rows after the first start, from the text's left edge.
pub(crate) const INDENT: usize = 2;

/// How the items of the list fall on rows.
pub(crate) struct Tall<'a> {
    model: &'a Model,
    /// The cells before the text on the first row, the first blank
    /// included, and before the text on the later rows.
    pub(crate) lead: (usize, usize),
    /// The width of the text on the first row and on the later rows.
    width: (usize, usize),
}

/// What an entry of the list pane is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    /// The item at this row of the list.
    Row(usize),
    /// The field of INSERT mode.
    Field,
}

/// Where the field stands among the rows, and how many it takes.
#[derive(Clone, Copy)]
struct Spot {
    at: usize,
    height: usize,
    replacing: bool,
}

/// The entries of the list pane: the rows, one after the other, with
/// the field of INSERT mode in place of one or before one.
pub(crate) struct Stack<'a> {
    pub(crate) tall: Option<Tall<'a>>,
    field: Option<Spot>,
}

impl Stack<'_> {
    /// What entry `v` is.
    pub(crate) fn entry(&self, v: usize) -> Entry {
        match self.field {
            Some(f) if v == f.at => Entry::Field,
            Some(f) if v > f.at && !f.replacing => Entry::Row(v - 1),
            Some(_) | None => Entry::Row(v),
        }
    }

    /// The rows entry `v` takes.
    pub(crate) fn height(&self, v: usize) -> usize {
        match (self.entry(v), self.field, &self.tall) {
            (Entry::Field, Some(f), _) => f.height,
            (Entry::Row(i), _, Some(tall)) => tall.height(i),
            _ => 1,
        }
    }
}

impl Model {
    /// How the items fall on rows, when the list wraps.
    fn tall(&self) -> Option<Tall<'_>> {
        if !self.deps.config.wrap || self.rows.is_empty() {
            return None;
        }
        let w = usize::from(self.panes(self.dims()).list.width);
        let layout: Layout = self.list_layout(w);
        let avail = w.saturating_sub(2 * BORDER);
        let lead = layout.lead();
        Some(Tall {
            model: self,
            lead: (1 + lead, 1 + lead + INDENT),
            width: (layout.text, avail.saturating_sub(lead + INDENT)),
        })
    }

    /// Runs `f` on the list pane: its entries and the arithmetic of
    /// its view, every entry as tall as it is drawn.
    pub(crate) fn stacked<R>(
        &self,
        f: impl FnOnce(&Stack<'_>, Sizes<'_>) -> R,
    ) -> R {
        let field = self.edit.as_ref().map(|draft| Spot {
            at: self.field_at(),
            height: self.field_height(),
            replacing: draft.replacing(),
        });
        let stack = Stack {
            tall: self.tall(),
            field,
        };
        let len =
            self.rows.len() + usize::from(field.is_some_and(|f| !f.replacing));
        let height = |v: usize| stack.height(v);
        let plain = stack.tall.is_none() && field.is_none();
        let size = Sizes {
            len,
            rows: self.pane_rows(),
            heights: (!plain).then_some(&height),
        };
        f(&stack, size)
    }

    /// Runs `f` on the arithmetic of the view of the list pane.
    pub(crate) fn sized<R>(&self, f: impl FnOnce(Sizes<'_>) -> R) -> R {
        self.stacked(|_, size| f(size))
    }

    /// The item at line `line` of the list pane, if there is one.
    pub(crate) fn item_at_line(&self, line: usize) -> Option<usize> {
        self.stacked(|stack, size| {
            let mut start = 0;
            let found = size.shown(self.top).into_iter().find(|&(_, n)| {
                start += n;
                line < start
            });
            match found.map(|(v, _)| stack.entry(v)) {
                Some(Entry::Row(i)) => Some(i),
                Some(Entry::Field) | None => None,
            }
        })
    }
}

impl Tall<'_> {
    /// The rows item `i` takes, 1 when there is no such item.
    pub(crate) fn height(&self, i: usize) -> usize {
        let Some(row) = self.model.rows.get(i) else {
            return 1;
        };
        if row.text_width <= self.width.0 {
            return 1;
        }
        self.lines(i).len()
    }

    /// The text of item `i` on its rows, the last cut with an ellipsis.
    /// Worked out once for the widths, as every move and draw asks again.
    pub(crate) fn lines(&self, i: usize) -> Rc<Vec<Vec<Span<'static>>>> {
        let Some(row) = self.model.rows.get(i) else {
            return Rc::default();
        };
        let mut kept = row.lines.borrow_mut();
        match &*kept {
            Some((width, lines)) if *width == self.width => Rc::clone(lines),
            Some(_) | None => {
                let lines = Rc::new(self.wrap(i));
                *kept = Some((self.width, Rc::clone(&lines)));
                lines
            }
        }
    }

    /// The text of item `i` wrapped, the last row cut with an ellipsis.
    fn wrap(&self, i: usize) -> Vec<Vec<Span<'static>>> {
        let m = self.model;
        let Some(row) = m.rows.get(i) else {
            return vec![Vec::new()];
        };
        let Some(item) = m.snap.item(row.id) else {
            return vec![Vec::new()];
        };
        let line = m.snap.line(item);
        let spans = text_spans(line, row.base, Inline::new(&m.deps.config));
        let (first, later) = self.width;
        let mut lines =
            wrap_words(&spans, |r| if r == 0 { first } else { later });
        if lines.len() > MAX_ROWS {
            let rest = lines.split_off(MAX_ROWS - 1);
            let last = rest.join(&Span::new(" ", None));
            lines.push(fit(last, later));
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        lines
    }
}
