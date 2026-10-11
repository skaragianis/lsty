//! The popup that asks which link of an item to open.

use lsty_core::{
    errs::lossy,
    keys::{Action, Fed, Key, Outcome, Stroke},
    links::LinkedFile,
    theme::Slot,
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    layout::{BORDER, cells},
    model::Model,
    motion::Step,
    msg::Cmd,
    open::exists_mark,
    text::{Span, fit, pad, titled, width_of, window},
};

const TITLE: &str = "Open";

/// The links to choose from, the selected one and the first one shown.
#[derive(Debug)]
pub(crate) struct Picker {
    pub(crate) links: Vec<LinkedFile>,
    at: usize,
    top: usize,
}

impl Picker {
    pub(crate) const fn new(links: Vec<LinkedFile>) -> Self {
        Self {
            links,
            at: 0,
            top: 0,
        }
    }

    fn lines(&self) -> Vec<String> {
        self.links
            .iter()
            .enumerate()
            .map(|(i, link)| {
                let raw = lossy(&link.raw);
                format!(" {} {} {raw}", i + 1, exists_mark(link.path.exists()))
            })
            .collect()
    }
}

impl Model {
    /// The keys of the picker: the table's, then the digits.
    pub(crate) fn picker_keys(
        &mut self,
        stroke: Stroke,
        fed: &Fed,
    ) -> Vec<Cmd> {
        match fed.outcome {
            Outcome::Matched { action, count } => {
                self.picker_action(action, count)
            }
            Outcome::NoMatch => {
                digit(stroke).map_or_else(Vec::new, |n| self.choose_link(n))
            }
            Outcome::Pending => Vec::new(),
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the picker takes three actions and the motions"
    )]
    fn picker_action(&mut self, action: Action, count: usize) -> Vec<Cmd> {
        let rows = self.opening.as_ref().map_or(0, |p| {
            let height = self.picker_box(p.links.len(), 0).height;
            usize::from(height).saturating_sub(BORDER)
        });
        let Some(p) = &mut self.opening else {
            return Vec::new();
        };
        match action {
            Action::Accept => {
                let n = p.at + 1;
                return self.choose_link(n);
            }
            Action::Close => {
                self.opening = None;
                return Vec::new();
            }
            _ => {
                let pane = (p.links.len(), rows);
                if let Some(step) = Step::of(action, count, rows) {
                    (p.at, p.top) = step.to(pane, Some(p.at), p.top);
                }
            }
        }
        Vec::new()
    }

    /// Opens link `n`; the picker stays when there is no such link.
    fn choose_link(&mut self, n: usize) -> Vec<Cmd> {
        let Some(picker) = self.opening.take() else {
            return Vec::new();
        };
        let cmds = self.open_number(&picker.links, n);
        if n > picker.links.len() {
            self.opening = Some(picker);
        }
        cmds
    }

    /// The popup for `rows` lines `inner` cells wide, centred over the
    /// list pane.
    fn picker_box(&self, rows: usize, inner: usize) -> Rect {
        let pane = self.panes(self.dims()).list;
        let w = cells(inner + BORDER).min(pane.width);
        let h = cells(rows + BORDER).min(pane.height);
        Rect::new(
            pane.x + (pane.width - w) / 2,
            pane.y + (pane.height - h) / 2,
            w,
            h,
        )
    }

    /// Draws the picker over the list.
    pub(crate) fn draw_picker(&self, buf: &mut Buffer) {
        let Some(picker) = &self.opening else {
            return;
        };
        let text = picker.lines();
        let widest = text.iter().map(|l| width_of(l)).max().unwrap_or(0);
        let at = self.picker_box(text.len(), widest);
        crate::draw::clear(buf, at);
        let w = usize::from(at.width);
        let rows = usize::from(at.height).saturating_sub(BORDER);
        let inner = w.saturating_sub(BORDER);
        let slot = Some(Slot::PanelBorderActive);
        let top = Span::new(titled("┌", TITLE, "┐", w), slot);
        self.put_at(buf, (at.x, at.y), w, &[top]);
        let first = window(picker.top, picker.at, picker.links.len(), rows);
        for i in 0..rows {
            let line = text.get(first + i).map_or("", String::as_str);
            let mut spans =
                fit(vec![Span::new(line, Some(Slot::Text))], inner);
            pad(&mut spans, inner, None);
            let mut row = vec![Span::new("│", slot)];
            row.extend(spans);
            row.push(Span::new("│", slot));
            let y = at.y + 1 + cells(i);
            self.put_at(buf, (at.x, y), w, &row);
            if first + i == picker.at {
                let cells = Rect::new(at.x + 1, y, at.width - 2, 1);
                buf.set_style(cells, self.styles.fill(Slot::Selection));
            }
        }
        let bottom = Span::new(titled("└", "", "┘", w), slot);
        let y = at.y + at.height.saturating_sub(1);
        self.put_at(buf, (at.x, y), w, &[bottom]);
    }
}

/// The number of the link that a digit names.
fn digit(stroke: Stroke) -> Option<usize> {
    if !stroke.mods.is_empty() {
        return None;
    }
    match stroke.key {
        Key::Char(c @ '1'..='9') => usize::try_from(c.to_digit(10)?).ok(),
        Key::Char(_) | Key::Named(_) => None,
    }
}
