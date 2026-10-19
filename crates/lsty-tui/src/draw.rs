//! Drawing the screen: the list pane with its border and rows.

use std::borrow::Cow;

use lsty_core::{
    config::LineNumbers, errs::Source, query::Category, theme::Slot,
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    columns::{Layout, MIN_GUTTER},
    focus::Focus,
    layout::{BORDER, FRAME, Size, cells},
    model::Model,
    reldate::Cell,
    rows::{Inline, Row, text_spans},
    tall::{Entry, Tall},
    text::{RULE, Span, fit, pad, prefix, put, width},
};

pub(crate) const MIN_WIDTH: u16 = 20;
pub(crate) const MIN_HEIGHT: u16 = 5;
const TOO_SMALL: &str = "terminal too small";
const NOTHING: &str = "nothing matches";

/// How wide the list pane is, where it starts and how its columns are
/// laid out.
#[derive(Clone, Copy)]
struct Pane {
    x: u16,
    layout: Layout,
    inner: usize,
    slot: Option<Slot>,
    /// The first and last row of the selection.
    selected: Option<(usize, usize)>,
}

impl Model {
    /// Writes `spans` at `at` within `w` cells, unless the row is off
    /// the screen.
    pub(crate) fn put_at(
        &self,
        buf: &mut Buffer,
        at: (impl TryInto<u16>, impl TryInto<u16>),
        w: usize,
        spans: &[Span<'_>],
    ) {
        let (Ok(x), Ok(y)) = (at.0.try_into(), at.1.try_into()) else {
            return;
        };
        if y < self.height.min(buf.area.height) {
            put(buf, (x, y), w, spans, &self.styles);
        }
    }

    /// Draws the whole screen, within the buffer when that is smaller
    /// than the size the model was told.
    pub(crate) fn draw(&self, buf: &mut Buffer) {
        let width = self.width.min(buf.area.width);
        let height = self.height.min(buf.area.height);
        if width == 0 || height == 0 {
            return;
        }
        if width < MIN_WIDTH || height < MIN_HEIGHT {
            let text = prefix(TOO_SMALL, usize::from(width));
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
        self.draw_popup(buf);
        self.draw_output(buf);
        self.draw_picker(buf);
        self.draw_themes(buf);
        self.draw_above(buf, (width, height - 2));
        self.draw_status(buf, (width, height - 1));
    }

    fn draw_list(&self, buf: &mut Buffer, area: Rect) {
        let w = usize::from(area.width);
        let inner = w.saturating_sub(BORDER);
        let slot = Some(self.edge(&[Focus::List]));
        self.draw_top(buf, area, slot);
        let layout = self.list_layout(w);
        let pane = Pane {
            x: area.x,
            layout,
            inner,
            slot,
            selected: self.selected_rows(),
        };
        let room = usize::from(area.height).saturating_sub(BORDER);
        let end = area.y + cells(room) + 1;
        let mut y = area.y + 1;
        self.stacked(|stack, size| {
            let tall = stack.tall.as_ref();
            for (v, shown) in size.shown(self.top) {
                let shown = shown.min(usize::from(end.saturating_sub(y)));
                let at = (y, shown);
                match stack.entry(v) {
                    Entry::Row(i) => self.draw_task(buf, (at, i), pane, tall),
                    Entry::Field => self.draw_field(buf, at, pane),
                }
                y += cells(shown);
            }
        });
        for y in y..end {
            self.draw_sides(buf, (area.x, y), inner, slot);
        }
        if self.rows.is_empty() && self.edit.is_none() {
            let spans = self.empty_row();
            put(buf, (area.x + 1, area.y + 1), inner, &spans, &self.styles);
        }
        let bottom = area.y + area.height.saturating_sub(1);
        let rule = format!("└{}┘", RULE.repeat(inner));
        let span = Span::new(rule, slot);
        put(buf, (area.x, bottom), w, &[span], &self.styles);
    }

    /// How the columns of a row fall in a list pane `w` cells wide.
    pub(crate) fn list_layout(&self, w: usize) -> Layout {
        let room = w.saturating_sub(FRAME);
        Layout::new(self.natural, self.gutter_width(), room)
    }

    /// `┌ [1]─todo.txt ───┐`: the top border, titled by `Model::title`.
    fn draw_top(&self, buf: &mut Buffer, area: Rect, slot: Option<Slot>) {
        let w = usize::from(area.width);
        let room = w.saturating_sub(FRAME);
        let title =
            fit(self.title(Some(self.title_slot(Focus::List)), room), room);
        let dashes = w.saturating_sub(FRAME + width(&title));
        let mut spans = vec![Span::new("┌ ", slot)];
        spans.extend(title);
        spans.push(Span::new(" ", slot));
        spans.push(Span::new(format!("{}┐", RULE.repeat(dashes)), slot));
        put(buf, (area.x, area.y), w, &spans, &self.styles);
    }

    /// Draws the first `shown` rows of list row `index` from screen row
    /// `y` on: more than one when `tall` wraps it.
    fn draw_task(
        &self,
        buf: &mut Buffer,
        ((y, shown), index): ((u16, usize), usize),
        pane: Pane,
        tall: Option<&Tall<'_>>,
    ) {
        let Pane {
            x,
            layout,
            inner,
            slot,
            selected,
        } = pane;
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let wrapped = tall.map(|t| (t.lines(index), t.lead));
        let chosen =
            selected.is_some_and(|(lo, hi)| (lo..=hi).contains(&index));
        for n in 0..shown {
            let spans = match &wrapped {
                Some((lines, indent)) if n > 0 => {
                    let mut spans = vec![Span::blanks(*indent, None)];
                    spans.extend(lines.get(n).into_iter().flatten().cloned());
                    spans
                }
                Some((lines, _)) => {
                    let first = lines.first().cloned();
                    self.row_spans(row, index, layout, first)
                }
                None => self.row_spans(row, index, layout, None),
            };
            let y = y + cells(n);
            self.draw_sides(buf, (x, y), inner, slot);
            let area = Rect::new(x + 1, y, cells(inner), 1);
            if chosen {
                buf.set_style(area, self.styles.fill(Slot::Selection));
            } else if self.is_current(index) {
                buf.set_style(area, self.styles.fill(Slot::Cursorline));
            }
            put(buf, (x + 1, y), inner, &spans, &self.styles);
        }
    }

    /// Draws `shown` rows of the field of INSERT mode from screen row
    /// `y` on, from the first row in its window.
    fn draw_field(
        &self,
        buf: &mut Buffer,
        (y, shown): (u16, usize),
        pane: Pane,
    ) {
        let Some(draft) = &self.edit else {
            return;
        };
        let lines = draft.field.lines(self.field_box().1);
        for n in 0..shown {
            let spans =
                lines.get(draft.window + n).cloned().unwrap_or_default();
            self.draw_field_row(buf, y + cells(n), (spans, n == 0), pane);
        }
    }

    /// The borders at both ends of the row at `at`.
    fn draw_sides(
        &self,
        buf: &mut Buffer,
        at: (u16, u16),
        inner: usize,
        slot: Option<Slot>,
    ) {
        let (x, y) = at;
        let side = Span::new("│", slot);
        let right = x + cells(inner + 1);
        put(buf, (x, y), 1, std::slice::from_ref(&side), &self.styles);
        put(
            buf,
            (right, y),
            1,
            std::slice::from_ref(&side),
            &self.styles,
        );
    }

    /// Draws a row of the field of INSERT mode on screen row `y`. The
    /// first one that is shown carries the number of its task.
    fn draw_field_row(
        &self,
        buf: &mut Buffer,
        y: u16,
        (text, first): (Vec<Span<'static>>, bool),
        pane: Pane,
    ) {
        let Pane { x, inner, slot, .. } = pane;
        self.draw_sides(buf, (x, y), inner, slot);
        let mut spans = vec![Span::new(" ", None), self.field_number(first)];
        spans.extend(text);
        let area = Rect::new(x + 1, y, cells(inner), 1);
        buf.set_style(area, self.styles.fill(Slot::Cursorline));
        put(buf, (x + 1, y), inner, &spans, &self.styles);
    }

    /// What goes before a row of the field: the number of the task it
    /// stands in the place of, or blanks.
    fn field_number(&self, first: bool) -> Span<'static> {
        let left = self.field_box().0;
        let numbered = self
            .edit
            .as_ref()
            .is_some_and(super::insert::Draft::replacing);
        self.rows
            .get(self.cursor)
            .filter(|_| first && numbered && self.gutter_width() > 0)
            .map_or_else(
                || Span::blanks(left - 1, None),
                |row| {
                    let span = self.number(row, self.cursor);
                    Span::new(format!("{} ", span.text), span.slot)
                },
            )
    }

    /// The message of a list that holds nothing though the file has
    /// tasks.
    fn empty_row(&self) -> Vec<Span<'static>> {
        let archived = self
            .active_query()
            .is_some_and(|q| q.asks_for(Category::Archived));
        let shows = self.shows;
        if !self.snap.tasks().iter().any(|i| shows.lists(i, archived)) {
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

    /// Whether row `index` is the current one: the cursor row, unless an
    /// open new task stands beside it.
    fn is_current(&self, index: usize) -> bool {
        index == self.cursor
            && self
                .edit
                .as_ref()
                .is_none_or(super::insert::Draft::replacing)
    }

    /// How far row `index` is from the current row; an open new task
    /// stands between the rows above it and the rows from `field_at` on.
    fn distance(&self, index: usize) -> usize {
        match &self.edit {
            Some(d) if !d.replacing() => {
                let at = self.field_at();
                (index + usize::from(index >= at)).abs_diff(at)
            }
            _ => index.abs_diff(self.cursor),
        }
    }

    fn number(&self, row: &Row, index: usize) -> Span<'static> {
        let gutter = self.gutter_width();
        if row.source == Source::Done {
            return Span::blanks(gutter, Some(Slot::Muted));
        }
        let style = self.deps.config.line_numbers;
        let nr = row.nr.get() as usize;
        let (n, slot) = if self.is_current(index) {
            let n = if style == LineNumbers::Relative {
                0
            } else {
                nr
            };
            (n, Slot::Text)
        } else if style == LineNumbers::Absolute {
            (nr, Slot::Muted)
        } else {
            (self.distance(index), Slot::Muted)
        };
        Span::new(format!("{n:>gutter$}"), Some(slot))
    }

    /// The spans of one row, with `text` in the task's column instead of
    /// the task cut to fit it.
    pub(crate) fn row_spans<'a>(
        &'a self,
        row: &'a Row,
        index: usize,
        layout: Layout,
        text: Option<Vec<Span<'a>>>,
    ) -> Vec<Span<'a>> {
        let Some(task) = self.snap.task(row.id) else {
            return Vec::new();
        };
        let line = self.snap.line(task);
        let gap = Span::new(" ", row.base);
        let mut out = vec![Span::new(" ", None)];
        if layout.gutter > 0 {
            out.push(self.number(row, index));
            out.push(Span::new(" ", None));
        }
        out.push(row.marker.clone());
        out.push(gap.clone());
        if layout.priority > 0 {
            let mut pri: Vec<Span<'_>> =
                row.priority.iter().cloned().collect();
            pad(&mut pri, layout.priority, row.base);
            out.extend(pri);
            out.push(gap);
        }
        let inline = Inline::new(&self.deps.config);
        let mut text = text.unwrap_or_else(|| {
            fit(text_spans(line, row.base, inline), layout.text)
        });
        pad(&mut text, layout.text, row.base);
        out.extend(text);
        push_columns(&mut out, row, layout);
        out
    }

    /// Whether a box of `owners` is drawn with focus: one of them has it
    /// and the terminal has not reported losing it.
    fn lit(&self, owners: &[Focus]) -> bool {
        !self.unfocused && owners.contains(&self.focus)
    }

    /// The slot of a border shared by `owners`: active when they are lit.
    pub(crate) fn edge(&self, owners: &[Focus]) -> Slot {
        if self.lit(owners) {
            Slot::PanelBorderActive
        } else {
            Slot::PanelBorder
        }
    }

    /// The slot of the title of the box `own`: the active border when it
    /// is lit, else muted.
    pub(crate) fn title_slot(&self, own: Focus) -> Slot {
        if self.lit(&[own]) {
            Slot::PanelBorderActive
        } else {
            Slot::Muted
        }
    }
}

fn push_columns<'a>(out: &mut Vec<Span<'a>>, row: &'a Row, layout: Layout) {
    let cell = |c: &'a Cell| Span::new(Cow::Borrowed(c.text.as_str()), c.slot);
    let columns = [
        (&row.date, layout.date),
        (&row.id_cell, layout.id),
        (&row.duration, layout.duration),
    ];
    for (c, w) in columns {
        if w == 0 {
            continue;
        }
        out.push(Span::new("  ", row.base));
        let mut spans = fit(vec![cell(c)], w);
        pad(&mut spans, w, row.base);
        out.extend(spans);
    }
}

/// Wipes the cells of `area`, so that what is drawn over them does not
/// inherit their style.
pub(crate) fn clear(buf: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
            }
        }
    }
}
