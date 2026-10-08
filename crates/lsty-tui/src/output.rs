//! The popup that shows what a command of the `:` line printed.

use lsty_core::errs::lossy;
use lsty_core::keys::{Action, Fed, Mode, Outcome};
use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;

use crate::help::Help;
use crate::layout::BORDER;
use crate::model::Model;
use crate::text::{Span, fit, pad, titled};

const MARGIN_X: usize = 2;
const MARGIN_Y: usize = 1;
const HELP_MARGIN: usize = 2;
/// Cells from the left edge of the popup to where the filter is typed.
const PROMPT_OFFSET: usize = 3;
const MIN_ROWS: usize = 3;
const MIN_WIDTH: usize = 20;

/// Lines of a read command, in a box over the list.
#[derive(Debug)]
pub(crate) struct Output {
    title: String,
    pub(crate) lines: Vec<Vec<Span<'static>>>,
    top: usize,
    /// The key help, whose lines follow its filter.
    pub(crate) help: Option<Help>,
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

    /// Which lines show when not all of them do, after the filter of
    /// the help if there is one.
    fn foot(&self, rows: usize) -> String {
        let filter = match &self.help {
            Some(help) if help.filtered() => {
                format!(" /{} ", help.query())
            }
            Some(_) | None => String::new(),
        };
        format!("{filter}{}", self.range(rows))
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
    fn range(&self, rows: usize) -> String {
        if self.lines.len() <= rows {
            return String::new();
        }
        let first = self.first(rows);
        let last = (first + rows).min(self.lines.len());
        format!(" {}-{last} of {} ", first + 1, self.lines.len())
    }
}

impl Model {
    /// Shows `lines` under `title`.
    pub(crate) fn open_output(&mut self, title: &str, lines: &[Vec<u8>]) {
        let spans =
            |line: &Vec<u8>| vec![Span::new(lossy(line), Some(Slot::Text))];
        self.output = Some(Output {
            title: title.to_owned(),
            lines: lines.iter().map(spans).collect(),
            top: 0,
            help: None,
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
        let margin = if out.help.is_some() {
            HELP_MARGIN
        } else {
            MARGIN_X
        };
        let room = body.saturating_sub(2 * MARGIN_Y).max(body.min(MIN_ROWS));
        let w = width.saturating_sub(2 * margin).max(width.min(MIN_WIDTH));
        let h = if out.help.is_some() {
            body.saturating_sub(2 * HELP_MARGIN).max(body.min(MIN_ROWS))
        } else {
            (out.lines.len() + BORDER).min(room)
        };
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
        if self.typing_help() {
            self.help_keys(fed);
            return;
        }
        let Outcome::Matched { action, count } = fed.outcome else {
            return;
        };
        let count = count
            .map_or(0, |c| usize::try_from(c.get()).unwrap_or(usize::MAX));
        let rows =
            self.output_spot().map_or(0, |s| s.h.saturating_sub(BORDER));
        if action == Action::Close {
            self.output = None;
        } else if action == Action::Search {
            self.help_search();
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
        self.put_at(buf, (at.x, at.y), at.w, &[Span::new(top, slot)]);
        let first = out.first(rows);
        for i in 0..rows {
            let line = out.lines.get(first + i).cloned().unwrap_or_default();
            let mut spans = fit(line, inner);
            pad(&mut spans, inner, None);
            let mut row = vec![Span::new("│", slot)];
            row.extend(spans);
            row.push(Span::new("│", slot));
            self.put_at(buf, (at.x, at.y + 1 + i), at.w, &row);
        }
        let foot = fit(vec![Span::new(out.foot(rows), None)], inner);
        let foot: String = foot.iter().map(|s| s.text.as_ref()).collect();
        let bottom = Span::new(titled("└", &foot, "┘", at.w), slot);
        self.put_at(buf, (at.x, at.y + 1 + rows), at.w, &[bottom]);
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

impl Model {
    /// Lists the lines of the help again after its filter changed.
    pub(crate) fn refilter(&mut self) {
        let rows =
            self.output_spot().map_or(0, |s| s.h.saturating_sub(BORDER));
        let Some(out) = &mut self.output else {
            return;
        };
        if let Some(help) = &out.help {
            out.lines = help.lines();
        }
        out.shift(0, rows);
    }

    /// Where the text cursor is while the filter of the help is typed:
    /// in the prompt on the bottom line of the popup.
    pub(crate) fn help_cursor(&self) -> Option<(u16, u16)> {
        let field = self.output.as_ref()?.help.as_ref()?.field.as_ref()?;
        let at = self.output_rect()?;
        let x = usize::from(at.x) + PROMPT_OFFSET + field.cells();
        let last = usize::from(at.x + at.width).saturating_sub(2);
        let y = at.y + at.height.saturating_sub(1);
        Some((u16::try_from(x.min(last)).ok()?, y))
    }
}
