//! The tasks list with `wrap` on: how many rows each task takes, what
//! the later rows show, and which tasks are in view.

use std::rc::Rc;

use lsty_core::{
    theme::Slot,
    todotxt::{Kind, Line},
};

use crate::{
    columns::Layout,
    model::Model,
    rows::{Inline, text_spans},
    sizes::Sizes,
    text::{ELLIPSIS, Span, cut, fit},
    wrap::wrap_words,
};

/// The most rows a task takes; the last one ends with an ellipsis when
/// the text goes on.
pub(crate) const MAX_ROWS: usize = 3;
/// How far in the rows after the first start, from the text's left edge,
/// when there is no number column.
pub(crate) const INDENT: usize = 2;

/// How the tasks of the list fall on rows.
pub(crate) struct Tall<'a> {
    model: &'a Model,
    /// The cells before the text on the later rows.
    pub(crate) lead: usize,
    /// The width of the text on the first row and on the later rows.
    width: (usize, usize),
}

/// What an entry of the list pane is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    /// The task at this row of the list.
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
        match self.field.map(|f| (v.checked_sub(f.at), f.replacing)) {
            Some((Some(0), _)) => Entry::Field,
            Some((Some(_), false)) => Entry::Row(v - 1),
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
    /// How the tasks fall on rows, when the list wraps.
    fn tall(&self) -> Option<Tall<'_>> {
        if !self.deps.config.wrap || self.rows.is_empty() {
            return None;
        }
        let w = usize::from(self.panes(self.dims()).list.width);
        let layout: Layout = self.list_layout(w);
        let lead = layout.lead();
        // the number column already shows where a task starts
        let indent = if layout.gutter > 0 { 0 } else { INDENT };
        Some(Tall {
            model: self,
            lead: 1 + lead + indent,
            width: (layout.text, layout.text.saturating_sub(indent)),
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

    /// The task at line `line` of the list pane, if there is one.
    pub(crate) fn task_at_line(&self, line: usize) -> Option<usize> {
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
    /// The rows task `i` takes, 1 when there is no such task.
    pub(crate) fn height(&self, i: usize) -> usize {
        let Some(row) = self.model.rows.get(i) else {
            return 1;
        };
        if row.text_width <= self.width.0 {
            return 1;
        }
        self.lines(i).len()
    }

    /// The text of task `i` on its rows, the last cut with an ellipsis.
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

    /// The text of task `i` wrapped, the last row cut with an ellipsis.
    fn wrap(&self, i: usize) -> Vec<Vec<Span<'static>>> {
        let m = self.model;
        let Some(row) = m.rows.get(i) else {
            return vec![Vec::new()];
        };
        let Some(task) = m.snap.task(row.id) else {
            return vec![Vec::new()];
        };
        let inline = Inline::new(&m.deps.config);
        let mut lines =
            lay_out(m.snap.line(task), row.base, inline, self.width);
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        lines
    }
}

/// The text of `line` on rows `first` then `later` cells wide, no more
/// than `MAX_ROWS`, the last cut with an ellipsis. When that cut would
/// leave a project or context out or cut it, they follow the ellipsis
/// instead.
pub(crate) fn lay_out(
    line: &Line,
    base: Option<Slot>,
    inline: Inline,
    (first, later): (usize, usize),
) -> Vec<Vec<Span<'static>>> {
    let spans = text_spans(line, base, inline);
    let row = move |r| if r == 0 { first } else { later };
    // three rows hold their cells and two break spaces: the spans that
    // start by the cell after lay out the three rows as the whole text
    let room = first + 2 * later + 2;
    let mut at = 0;
    let held = spans.iter().take_while(|s| {
        at += s.cells();
        at - s.cells() <= room + 1
    });
    let text = &spans[..held.count()];
    let lines = wrap_words(text, row);
    if lines.len() <= MAX_ROWS {
        return lines;
    }
    let space = Span::new(" ", base.or(Some(Slot::Text)));
    // token `j` is span `2 * j`, a space between each two
    let kinds: Vec<Kind> = line
        .tokens()
        .filter(|t| inline.in_text(t))
        .map(|t| t.kind)
        .collect();
    // all are shown whole when the last ends before the last two cells
    // of the third row, one for a space and one for the ellipsis
    let edge = |r| {
        if r < 2 {
            row(r)
        } else {
            later.saturating_sub(2)
        }
    };
    let last = (0..spans.len()).rev().find(|&n| is_tag(&kinds, n));
    let whole = |n| wrap_words(&spans[..=n], edge).len() <= MAX_ROWS;
    if last.is_none_or(|n| n < text.len() && whole(n)) {
        return clip(lines, later, &space);
    }
    with_tags(&spans, &kinds, row, (&space, room))
}

/// Whether span `n` is a project or a context.
fn is_tag(kinds: &[Kind], n: usize) -> bool {
    n.is_multiple_of(2)
        && matches!(kinds.get(n / 2), Some(Kind::Project | Kind::Context))
}

/// The text cut with an ellipsis as late as lets every project and
/// context it does not hold whole follow it in `MAX_ROWS` rows, contexts
/// first, each once in the order written and none the text holds a copy
/// of whole; the last cut when they do not fit.
fn with_tags(
    spans: &[Span<'_>],
    kinds: &[Kind],
    row: impl Fn(usize) -> usize + Copy,
    (space, room): (&Span<'static>, usize),
) -> Vec<Vec<Span<'static>>> {
    let ends: Vec<usize> = spans
        .iter()
        .scan(0, |at, s| {
            *at += s.cells();
            Some(*at)
        })
        .collect();
    let ends = &ends[..];
    let mut tags: Vec<usize> = (0..spans.len())
        .filter(|&n| is_tag(kinds, n))
        .filter(|&n| {
            !(0..n).any(|m| is_tag(kinds, m) && spans[m].text == spans[n].text)
        })
        .collect();
    tags.sort_by_key(|&n| kinds[n / 2] == Kind::Project);
    let tags = &tags[..];
    // the projects and contexts the text up to cell `c` does not hold
    let moved = |c: usize| tags.iter().copied().filter(move |&n| ends[n] > c);
    let build = |c: usize| {
        let held = ends.partition_point(|&e| e < c) + 1;
        let mut out = cut(spans[..held.min(spans.len())].to_vec(), c);
        let slot = out.last().or_else(|| spans.first()).and_then(|s| s.slot);
        out.push(Span::new(ELLIPSIS, slot));
        for n in moved(c) {
            out.extend([space.clone(), spans[n].clone()]);
        }
        out
    };
    // cut inside or after a word, never after a space nor inside or
    // after a project or context; what is needed only shrinks as the cut
    // moves left
    let need =
        |c| c + 1 + moved(c).map(|n| 1 + spans[n].cells()).sum::<usize>();
    (0..spans.len())
        .rev()
        .filter(|&n| spans[n].text != " " && !is_tag(kinds, n))
        .flat_map(|n| (ends[n] - spans[n].cells() + 1..=ends[n]).rev())
        .skip_while(|&c| need(c) > room)
        .map(|c| wrap_words(&build(c), row))
        .find(|lines| lines.len() <= MAX_ROWS)
        .unwrap_or_else(|| clip(wrap_words(&build(0), row), row(1), space))
}

/// `lines` cut to `MAX_ROWS`, the rows past the last joined onto it and
/// cut to `later` cells with an ellipsis, `space` between them.
fn clip(
    mut lines: Vec<Vec<Span<'static>>>,
    later: usize,
    space: &Span<'static>,
) -> Vec<Vec<Span<'static>>> {
    let rest = lines.split_off(lines.len().min(MAX_ROWS - 1));
    if !rest.is_empty() {
        let last = rest.join(space);
        lines.push(fit(last, later));
    }
    lines
}
