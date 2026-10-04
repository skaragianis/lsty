//! Drawing the screen: the list pane with its border and rows.

use std::borrow::Cow;

use lsty_core::config::LineNumbers;
use lsty_core::errs::Source;
use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::columns::{Layout, MIN_GUTTER, PRIORITY};
use crate::model::Model;
use crate::rows::{Row, priority_span, tag_spans, text_spans};
use crate::text::{Span, fit, pad, put, width};

const MIN_WIDTH: u16 = 20;
const MIN_HEIGHT: u16 = 5;
const TOO_SMALL: &str = "terminal too small";
const NOTHING: &str = "nothing matches the filters";
const BORDER: usize = 2;
/// The border of the list pane: the list always has the focus here.
const BORDER_SLOT: Option<Slot> = Some(Slot::PanelBorderActive);

/// How wide the list pane is and how its columns are laid out.
#[derive(Clone, Copy)]
struct Pane {
    layout: Layout,
    inner: usize,
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
        let area = Rect::new(0, 0, width, height - 1);
        self.draw_list(buf, area);
        self.draw_status(buf, (width, height - 1));
    }

    fn draw_list(&self, buf: &mut Buffer, area: Rect) {
        let w = usize::from(area.width);
        let inner = w.saturating_sub(BORDER);
        self.draw_top(buf, w);
        let layout = Layout::new(
            self.natural,
            self.gutter_width(),
            inner.saturating_sub(BORDER),
        );
        let room = usize::from(area.height).saturating_sub(BORDER);
        for line in 0..room {
            let y = u16::try_from(line + 1).unwrap_or(u16::MAX);
            self.draw_row(buf, (y, self.top + line), Pane { layout, inner });
        }
        let bottom = area.height.saturating_sub(1);
        let rule = format!("└{}┘", "─".repeat(inner));
        let span = Span::new(rule, BORDER_SLOT);
        put(buf, (0, bottom), w, &[span], &self.styles);
    }

    /// `┌ todo.txt ───┐`: the title is the name of the file.
    fn draw_top(&self, buf: &mut Buffer, w: usize) {
        let slot = BORDER_SLOT;
        let name = Span::new(self.base_name(), slot);
        let title = fit(vec![name], w.saturating_sub(2 * BORDER));
        let dashes = w.saturating_sub(2 * BORDER + width(&title));
        let mut spans = vec![Span::new("┌ ", slot)];
        spans.extend(title);
        spans.push(Span::new(" ", slot));
        spans.push(Span::new(format!("{}┐", "─".repeat(dashes)), slot));
        put(buf, (0, 0), w, &spans, &self.styles);
    }

    /// Draws the row at screen row `at.0` that shows list row `at.1`.
    fn draw_row(&self, buf: &mut Buffer, at: (u16, usize), pane: Pane) {
        let (y, index) = at;
        let Pane { layout, inner } = pane;
        let side = Span::new("│", BORDER_SLOT);
        let right = u16::try_from(inner + 1).unwrap_or(u16::MAX);
        put(buf, (0, y), 1, std::slice::from_ref(&side), &self.styles);
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
        let area = Rect::new(1, y, u16::try_from(inner).unwrap_or(0), 1);
        if index == self.cursor && index < self.rows.len() {
            buf.set_style(area, self.styles.fill(Slot::Cursorline));
        }
        put(buf, (1, y), inner, &spans, &self.styles);
    }

    /// The message of a list that holds nothing though the file has
    /// items.
    fn empty_row(&self, index: usize) -> Vec<Span<'static>> {
        let listed = self.snap.items().iter().any(|i| self.shows.lists(i));
        if index > 0 || !self.rows.is_empty() || !listed {
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
