//! Drawing the screen: the list pane with its border and rows.

use std::borrow::Cow;

use lsty_core::{
    config::{Hidden, LineNumbers},
    errs::Source,
    query::Category,
    theme::Slot,
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    columns::{Layout, MIN_GUTTER},
    focus::Focus,
    layout::{BORDER, FRAME, Size, cells},
    model::Model,
    reldate::Cell,
    rows::{Inline, Row, Shape, text_spans},
    styles::Styles,
    tall::{Entry, Tall, lay_out},
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
    /// The rows of the task at the top the list cuts.
    skip: usize,
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
        buf.set_style(
            Rect::new(0, 0, width, height),
            self.styles.fill(Slot::Background),
        );
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
                clear(buf, side.rect, &self.styles);
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
            skip: self.sized(|size| size.cut.1),
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
            ..
        } = pane;
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let chosen =
            selected.is_some_and(|(lo, hi)| (lo..=hi).contains(&index));
        let from = if index == self.top { pane.skip } else { 0 };
        for n in from..from + shown {
            let spans = self.line_spans(row, (index, n), layout, tall);
            let y = y + cells(n - from);
            self.draw_sides(buf, (x, y), inner, slot);
            let area = Rect::new(x + 1, y, cells(inner), 1);
            let current = !chosen && self.is_current(index);
            if chosen {
                buf.set_style(area, self.styles.fill(Slot::Selection));
            } else if current {
                buf.set_style(area, self.styles.fill(Slot::Cursorline));
            }
            put(buf, (x + 1, y), inner, &spans, self.styles.row(current));
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
        let lines = draft.field.lines(self.field_box().1, self.typed());
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
        put(buf, (x + 1, y), inner, &spans, self.styles.row(true));
    }

    /// What goes before a row of the field: its place in the list, or
    /// `0` in relative mode on a task it replaces, or blanks.
    fn field_number(&self, first: bool) -> Span<'static> {
        let left = self.field_box().0;
        let gutter = self.gutter_width();
        let replacing = !self.new_field();
        let n = if self.deps.config.line_numbers == LineNumbers::Relative {
            replacing.then_some(0)
        } else {
            Some(self.field_at() + 1)
        };
        match n.filter(|_| first && gutter > 0) {
            Some(n) => Span::new(format!("{n:>gutter$} "), Some(Slot::Text)),
            None => Span::blanks(left - 1, None),
        }
    }

    /// The message of a list that holds nothing though the file has
    /// tasks.
    fn empty_row(&self) -> Vec<Span<'static>> {
        let archived = self.scope().asked.contains(&Category::Archived);
        let shows = self.shows;
        if !self.snap.tasks().iter().any(|i| shows.lists(i, archived)) {
            return Vec::new();
        }
        vec![Span::new(format!(" {NOTHING}"), Some(Slot::Muted))]
    }

    /// The width of the number column, 0 when numbers are off: that of the
    /// lines of the files shown or of the rows drawn (a new task's
    /// included), whichever is more, whatever the filter.
    pub(crate) fn gutter_width(&self) -> usize {
        if self.deps.config.line_numbers == LineNumbers::Off {
            return 0;
        }
        let mut lines = self.snap.todo().lines.len();
        let archived = self.shows.on(Hidden::Archive)
            || self.scope().asked.contains(&Category::Archived);
        if archived {
            lines += self.snap.file(Source::Done).lines.len();
        }
        let shown = self.rows.len() + usize::from(self.new_field());
        lines.max(shown).to_string().len().max(MIN_GUTTER)
    }

    /// Whether a new task's field is open, in a row of its own.
    pub(crate) fn new_field(&self) -> bool {
        self.edit.as_ref().is_some_and(|d| !d.replacing())
    }

    /// The place of row `index` in the list as drawn, from 1: an open new
    /// task takes the place it stands in and moves the rows from there on.
    fn place(&self, index: usize) -> usize {
        index + 1 + usize::from(self.new_field() && index >= self.field_at())
    }

    /// Whether row `index` is the current one: the cursor row, unless an
    /// open new task stands beside it.
    fn is_current(&self, index: usize) -> bool {
        index == self.cursor && !self.new_field()
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

    /// The number of a row: its place in the list or its distance to the
    /// current row.
    fn number(&self, index: usize, gutter: usize) -> Span<'static> {
        let style = self.deps.config.line_numbers;
        let (n, slot) = if self.is_current(index) {
            let n = if style == LineNumbers::Relative {
                0
            } else {
                self.place(index)
            };
            (n, Slot::Text)
        } else if style == LineNumbers::Absolute {
            (self.place(index), Slot::Muted)
        } else {
            (self.distance(index), Slot::Muted)
        };
        Span::new(format!("{n:>gutter$}"), Some(slot))
    }

    /// The spans of line `n` of task `index`: the first through
    /// `row_spans`, the later ones as `tall` wrapped them.
    fn line_spans<'a>(
        &'a self,
        row: &'a Row,
        (index, n): (usize, usize),
        layout: Layout,
        tall: Option<&Tall<'_>>,
    ) -> Vec<Span<'a>> {
        match tall {
            Some(t) if n > 0 => t.later(index, n),
            Some(_) | None => {
                let first = tall.and_then(|t| t.lines(index).first().cloned());
                self.row_spans(row, index, layout, first)
            }
        }
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
            out.push(self.number(index, layout.gutter));
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
            let paint = (row.base, row.parts);
            if row.text_width <= layout.text {
                return text_spans(line, paint, inline);
            }
            let shape = Shape {
                widths: (layout.text, layout.text),
                rows: 1,
            };
            let mut one = lay_out(line, paint, inline, shape);
            one.pop().unwrap_or_default()
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

/// Wipes the cells of `area` to the screen fill, so that what is drawn
/// over them does not inherit their style.
pub(crate) fn clear(buf: &mut Buffer, area: Rect, styles: &Styles) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
                cell.set_style(styles.fill(Slot::Background));
            }
        }
    }
}
