//! Drawing the screen: the list pane with its border and rows.

use std::borrow::Cow;

use lsty_core::config::LineNumbers;
use lsty_core::errs::Source;
use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::columns::{Layout, MIN_GUTTER, PRIORITY};
use crate::focus::Focus;
use crate::layout::{BORDER, Size};
use crate::model::Model;
use crate::rows::{Row, priority_span, tag_spans, text_spans};
use crate::text::{Span, fit, pad, put, width};

const MIN_WIDTH: u16 = 20;
const MIN_HEIGHT: u16 = 5;
const TOO_SMALL: &str = "terminal too small";
const NOTHING: &str = "nothing matches the filters";

/// How wide the list pane is, where it starts and how its columns are
/// laid out.
#[derive(Clone, Copy)]
struct Pane {
    x: u16,
    layout: Layout,
    inner: usize,
    slot: Option<Slot>,
}

impl Model {
    /// Draws the whole screen, within the buffer when that is smaller
    /// than the size the model was told.
    pub(crate) fn draw(&self, buf: &mut Buffer) {
        let width = self.width.min(buf.area.width);
        let height = self.height.min(buf.area.height);
        if width == 0 && height == 0 {
            return;
        }
        if width < MIN_WIDTH || height < MIN_HEIGHT {
            let text = crate::text::prefix(TOO_SMALL, usize::from(width));
            buf.set_string(0, 0, text, ratatui::style::Style::default());
            return;
        }
        let dims = Size {
            w: usize::from(width),
            h: usize::from(height),
        };
        let panes = self.panes(dims);
        self.draw_list(buf, panes.list);
        if let Some(area) = panes.detail {
            self.draw_detail(buf, area);
        }
        if let Some(side) = panes.sidebar {
            if side.overlay {
                clear(buf, side.rect);
            }
            self.draw_sidebar(buf, side.rect);
        }
        self.draw_above(buf, (width, height - 2));
        self.draw_status(buf, (width, height - 1));
    }

    fn draw_list(&self, buf: &mut Buffer, area: Rect) {
        let w = usize::from(area.width);
        let inner = w.saturating_sub(BORDER);
        let slot = Some(self.edge(&[Focus::List]));
        self.draw_top(buf, area, slot);
        let layout = Layout::new(
            self.natural,
            self.gutter_width(),
            inner.saturating_sub(BORDER),
        );
        let pane = Pane {
            x: area.x,
            layout,
            inner,
            slot,
        };
        let room = usize::from(area.height).saturating_sub(BORDER);
        for line in 0..room {
            let y = area.y + 1 + u16::try_from(line).unwrap_or(u16::MAX);
            self.draw_row(buf, (y, self.top + line), pane);
        }
        let bottom = area.y + area.height.saturating_sub(1);
        let rule = format!("└{}┘", "─".repeat(inner));
        let span = Span::new(rule, slot);
        put(buf, (area.x, bottom), w, &[span], &self.styles);
    }

    /// `┌ todo.txt ───┐`: the title is the name of the file.
    fn draw_top(&self, buf: &mut Buffer, area: Rect, slot: Option<Slot>) {
        let w = usize::from(area.width);
        let title = fit(self.title(slot), w.saturating_sub(2 * BORDER));
        let dashes = w.saturating_sub(2 * BORDER + width(&title));
        let mut spans = vec![Span::new("┌ ", slot)];
        spans.extend(title);
        spans.push(Span::new(" ", slot));
        spans.push(Span::new(format!("{}┐", "─".repeat(dashes)), slot));
        put(buf, (area.x, area.y), w, &spans, &self.styles);
    }

    /// Draws the row at screen row `at.0` that shows list row `at.1`.
    fn draw_row(&self, buf: &mut Buffer, at: (u16, usize), pane: Pane) {
        let (y, index) = at;
        let Pane {
            x,
            layout,
            inner,
            slot,
        } = pane;
        let side = Span::new("│", slot);
        let right = x + u16::try_from(inner + 1).unwrap_or(u16::MAX);
        put(buf, (x, y), 1, std::slice::from_ref(&side), &self.styles);
        put(
            buf,
            (right, y),
            1,
            std::slice::from_ref(&side),
            &self.styles,
        );
        let spans = match self.rows.get(index) {
            Some(row) => self.row_spans(row, index, layout),
            None => self.empty_row(index),
        };
        let area = Rect::new(x + 1, y, u16::try_from(inner).unwrap_or(0), 1);
        if index == self.cursor && index < self.rows.len() {
            buf.set_style(area, self.styles.fill(Slot::Cursorline));
        }
        put(buf, (x + 1, y), inner, &spans, &self.styles);
    }

    /// The message of a list that holds nothing though the file has
    /// items.
    fn empty_row(&self, index: usize) -> Vec<Span<'static>> {
        if index > 0 || !self.rows.is_empty() {
            return Vec::new();
        }
        let scope = self.scope();
        if !self.snap.items().iter().any(|i| scope.lists(i)) {
            return Vec::new();
        }
        vec![Span::new(format!(" {NOTHING}"), Some(Slot::Muted))]
    }

    /// The width of the number column, 0 when numbers are off.
    pub(crate) fn gutter_width(&self) -> usize {
        if self.deps.config.line_numbers == LineNumbers::Off {
            return 0;
        }
        let lines = self.snap.todo().lines.len();
        lines.to_string().len().max(MIN_GUTTER)
    }

    fn number(&self, row: &Row, index: usize) -> Span<'static> {
        let gutter = self.gutter_width();
        if row.source == Source::Done {
            return Span::blanks(gutter, Some(Slot::Muted));
        }
        let style = self.deps.config.line_numbers;
        let distance = index.abs_diff(self.cursor);
        let (n, slot) = if index == self.cursor {
            let n = if style == LineNumbers::Relative {
                0
            } else {
                usize::try_from(row.nr).unwrap_or(0)
            };
            (n, Slot::Text)
        } else if style == LineNumbers::Absolute {
            (usize::try_from(row.nr).unwrap_or(0), Slot::Muted)
        } else {
            (distance, Slot::Muted)
        };
        Span::new(format!("{n:>gutter$}"), Some(slot))
    }

    /// The spans of one row, before padding to the pane's width.
    fn row_spans<'a>(
        &'a self,
        row: &'a Row,
        index: usize,
        layout: Layout,
    ) -> Vec<Span<'a>> {
        let Some(item) = self.snap.item(row.id) else {
            return Vec::new();
        };
        let line = self.snap.line(item);
        let gap = Span::new(" ", row.base);
        let mut out = vec![Span::new(" ", None)];
        if layout.gutter > 0 {
            out.push(self.number(row, index));
            out.push(Span::new(" ", None));
        }
        out.push(row.marker.clone());
        out.push(gap.clone());
        let mut pri: Vec<Span<'_>> =
            priority_span(item, line, row.base).into_iter().collect();
        pad(&mut pri, PRIORITY, row.base);
        out.extend(pri);
        out.push(gap);
        let mut text = fit(text_spans(line, row.base), layout.text);
        pad(&mut text, layout.text, row.base);
        out.extend(text);
        push_columns(&mut out, row, line, layout);
        out
    }
}

fn push_columns<'a>(
    out: &mut Vec<Span<'a>>,
    row: &'a Row,
    line: &'a lsty_core::todotxt::Line,
    layout: Layout,
) {
    let date = Span::new(Cow::Borrowed(row.date.text.as_str()), row.date.slot);
    let claim =
        Span::new(Cow::Borrowed(row.claim.text.as_str()), row.claim.slot);
    let columns = [
        (tag_spans(line, row.base), layout.tags),
        (vec![date], layout.date),
        (vec![claim], layout.claim),
    ];
    for (spans, w) in columns {
        if w == 0 {
            continue;
        }
        out.push(Span::new("  ", row.base));
        let mut cell = fit(spans, w);
        pad(&mut cell, w, row.base);
        out.extend(cell);
    }
}

/// Wipes the cells of `area`, so that what is drawn over them does not
/// inherit their style.
fn clear(buf: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
            }
        }
    }
}
