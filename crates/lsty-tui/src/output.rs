//! The popup that shows what a command of the `:` line printed.

use lsty_core::errs::lossy;
use lsty_core::keys::{Action, Fed, Mode, Outcome};
use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;

use crate::layout::BORDER;
use crate::model::Model;
use crate::text::{Span, fit, pad, put, width};

const MARGIN_X: usize = 2;
const MARGIN_Y: usize = 1;
const MIN_ROWS: usize = 3;
const MIN_WIDTH: usize = 20;

/// Lines of a read command, in a box over the list.
#[derive(Debug)]
pub(crate) struct Output {
    title: String,
    lines: Vec<String>,
    top: usize,
}

/// Where the popup is and how big.
struct Spot {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Output {
    /// The first line that is asked for.
    #[cfg(test)]
    pub(crate) const fn top(&self) -> usize {
        self.top
    }

    /// Scrolls by `by` lines, staying between the first and the last
    /// page.
    fn shift(&mut self, by: isize, rows: usize) {
        let last = self.lines.len().saturating_sub(rows);
        self.top = self.top.saturating_add_signed(by).min(last);
    }

    /// The first line shown when `rows` of them fit.
    fn first(&self, rows: usize) -> usize {
        self.top.min(self.lines.len().saturating_sub(rows))
    }

    /// Which lines show when not all of them do.
    fn foot(&self, rows: usize) -> String {
        if self.lines.len() <= rows {
            return String::new();
        }
        let first = self.first(rows);
        let last = (first + rows).min(self.lines.len());
        format!(" {}-{last} of {} ", first + 1, self.lines.len())
    }
}

/// `left`, `title`, dashes up to `w` cells, `right`.
fn titled(left: &str, title: &str, right: &str, w: usize) -> String {
    let used = left.chars().count() + right.chars().count() + width_of(title);
    format!("{left}{title}{}{right}", "─".repeat(w.saturating_sub(used)))
}

fn width_of(text: &str) -> usize {
    width(&[Span::new(text, None)])
}

impl Model {
    /// Shows `lines` under `title`.
    pub(crate) fn open_output(&mut self, title: &str, lines: &[Vec<u8>]) {
        self.output = Some(Output {
            title: title.to_owned(),
            lines: lines.iter().map(|l| lossy(l)).collect(),
            top: 0,
        });
        if let Some(matcher) = self.matchers.get_mut(&Mode::Help) {
            matcher.reset();
        }
    }

    fn output_spot(&self) -> Option<Spot> {
        let out = self.output.as_ref()?;
        let (width, height) =
            (usize::from(self.width), usize::from(self.height));
        let body = height.saturating_sub(1);
        let room = body.saturating_sub(2 * MARGIN_Y).max(body.min(MIN_ROWS));
        let w = width.saturating_sub(2 * MARGIN_X).max(width.min(MIN_WIDTH));
        let h = (out.lines.len() + BORDER).min(room);
        Some(Spot {
            x: width.saturating_sub(w) / 2,
            y: body.saturating_sub(h) / 2,
            w,
            h,
        })
    }

    /// The cells the popup covers, if it is open.
    pub(crate) fn output_rect(&self) -> Option<ratatui::layout::Rect> {
        let at = self.output_spot()?;
        let cells = |n: usize| u16::try_from(n).ok();
        Some(ratatui::layout::Rect::new(
            cells(at.x)?,
            cells(at.y)?,
            cells(at.w)?,
            cells(at.h)?,
        ))
    }

    /// Scrolls the popup by `by` lines, as the wheel does.
    pub(crate) fn wheel_output(&mut self, by: isize) {
        let rows =
            self.output_spot().map_or(0, |s| s.h.saturating_sub(BORDER));
        if let Some(out) = &mut self.output {
            out.shift(by, rows);
        }
    }

    /// The keys of the popup: scrolling, and closing.
    pub(crate) fn output_keys(&mut self, fed: &Fed) {
        let Outcome::Matched { action, count } = fed.outcome else {
            return;
        };
        let count = count
            .map_or(0, |c| usize::try_from(c.get()).unwrap_or(usize::MAX));
        let rows =
            self.output_spot().map_or(0, |s| s.h.saturating_sub(BORDER));
        if action == Action::Close {
            self.output = None;
        } else if let Some(out) = &mut self.output {
            out.shift(scroll_by(action, count, rows), rows);
        }
    }

    /// Draws the popup over the list.
    pub(crate) fn draw_output(&self, buf: &mut Buffer) {
        let (Some(out), Some(at)) = (&self.output, self.output_spot()) else {
            return;
        };
        if let Some(area) = self.output_rect() {
            crate::draw::clear(buf, area);
        }
        let rows = at.h.saturating_sub(BORDER);
        let inner = at.w.saturating_sub(BORDER);
        let slot = Some(Slot::PanelBorderActive);
        let top = titled("┌", &format!(" {} ", out.title), "┐", at.w);
        self.put_row(buf, (at.x, at.y), at.w, &[Span::new(top, slot)]);
        let first = out.first(rows);
        for i in 0..rows {
            let line = out.lines.get(first + i).map_or("", String::as_str);
            let mut spans =
                fit(vec![Span::new(line, Some(Slot::Text))], inner);
            pad(&mut spans, inner, None);
            let mut row = vec![Span::new("│", slot)];
            row.extend(spans);
            row.push(Span::new("│", slot));
            self.put_row(buf, (at.x, at.y + 1 + i), at.w, &row);
        }
        let foot = fit(vec![Span::new(out.foot(rows), None)], inner);
        let foot: String = foot.iter().map(|s| s.text.as_ref()).collect();
        let bottom = Span::new(titled("└", &foot, "┘", at.w), slot);
        self.put_row(buf, (at.x, at.y + 1 + rows), at.w, &[bottom]);
    }

    fn put_row(
        &self,
        buf: &mut Buffer,
        at: (usize, usize),
        w: usize,
        spans: &[Span<'_>],
    ) {
        let (Ok(x), Ok(y)) = (u16::try_from(at.0), u16::try_from(at.1)) else {
            return;
        };
        if y < self.height.min(buf.area.height) {
            put(buf, (x, y), w, spans, &self.styles);
        }
    }
}

/// How many lines a key of the popup scrolls, up or down.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "only the scrolling keys move the popup"
)]
fn scroll_by(action: Action, count: usize, rows: usize) -> isize {
    let lines = |n: usize| isize::try_from(n).unwrap_or(isize::MAX);
    let step = lines(count.max(1));
    let half = lines(count.max(rows / 2));
    match action {
        Action::CursorDown => step,
        Action::CursorUp => -step,
        Action::HalfPageDown => half,
        Action::HalfPageUp => -half,
        _ => 0,
    }
}
