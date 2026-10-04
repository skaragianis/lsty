//! The popup that asks which link of an item to open.

use lsty_core::keys::{Action, Fed, Key, Outcome, Stroke};
use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::layout::BORDER;
use crate::model::Model;
use crate::msg::Cmd;
use crate::open::LinkRef;
use crate::text::{Span, fit, pad, put, width};

const TITLE: &str = "Open";

/// The links to choose from, the selected one and the first one shown.
#[derive(Debug)]
pub(crate) struct Picker {
    pub(crate) links: Vec<LinkRef>,
    at: usize,
    top: usize,
}

impl Picker {
    pub(crate) const fn new(links: Vec<LinkRef>) -> Self {
        Self {
            links,
            at: 0,
            top: 0,
        }
    }

    /// The first link shown when `rows` of them fit.
    fn window(&self, rows: usize) -> usize {
        let last = self.links.len().saturating_sub(rows);
        self.top
            .max((self.at + 1).saturating_sub(rows))
            .min(self.at)
            .min(last)
    }

    fn lines(&self) -> Vec<String> {
        self.links
            .iter()
            .enumerate()
            .map(|(i, link)| {
                let raw = String::from_utf8_lossy(&link.raw);
                format!(" {} {} {raw}", i + 1, link.mark())
            })
            .collect()
    }
}

/// `left`, `title`, dashes up to `w` cells, `right`.
fn titled(left: &str, title: &str, right: &str, w: usize) -> String {
    let used = 2 + width(&[Span::new(title, None)]);
    format!("{left}{title}{}{right}", "─".repeat(w.saturating_sub(used)))
}

impl Model {
    /// The keys of the picker: the table's, then the digits.
    pub(crate) fn picker_keys(
        &mut self,
        stroke: Stroke,
        fed: &Fed,
    ) -> Vec<Cmd> {
        match fed.outcome {
            Outcome::Matched { action, .. } => self.picker_action(action),
            Outcome::NoMatch => {
                digit(stroke).map_or_else(Vec::new, |n| self.choose_link(n))
            }
            Outcome::Pending => Vec::new(),
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the picker takes four actions"
    )]
    fn picker_action(&mut self, action: Action) -> Vec<Cmd> {
        let rows = self
            .picker_box()
            .map_or(0, |b| usize::from(b.height).saturating_sub(BORDER));
        let Some(p) = &mut self.opening else {
            return Vec::new();
        };
        match action {
            Action::CursorDown => {
                p.at = (p.at + 1).min(p.links.len().saturating_sub(1));
            }
            Action::CursorUp => p.at = p.at.saturating_sub(1),
            Action::Accept => {
                let n = p.at + 1;
                return self.choose_link(n);
            }
            Action::Close => {
                self.opening = None;
                return Vec::new();
            }
            _ => {}
        }
        p.top = p.window(rows);
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

    /// The popup, centred over the list.
    fn picker_box(&self) -> Option<Rect> {
        let picker = self.opening.as_ref()?;
        let pane = self.panes(self.dims()).list;
        let inner = picker
            .lines()
            .iter()
            .map(|l| width(&[Span::new(l.as_str(), None)]))
            .max()
            .unwrap_or(0);
        let cells = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        let w = cells(inner + BORDER).min(pane.width);
        let h = cells(picker.links.len() + BORDER).min(pane.height);
        Some(Rect::new(
            pane.x + (pane.width - w) / 2,
            pane.y + (pane.height - h) / 2,
            w,
            h,
        ))
    }

    /// Draws the picker over the list.
    pub(crate) fn draw_picker(&self, buf: &mut Buffer) {
        let (Some(picker), Some(at)) = (&self.opening, self.picker_box())
        else {
            return;
        };
        crate::draw::clear(buf, at);
        let w = usize::from(at.width);
        let rows = usize::from(at.height).saturating_sub(BORDER);
        let inner = w.saturating_sub(BORDER);
        let slot = Some(Slot::PanelBorderActive);
        let top = Span::new(titled("┌", TITLE, "┐", w), slot);
        self.put_at(buf, (at.x, at.y), w, &[top]);
        let first = picker.window(rows);
        let text = picker.lines();
        for i in 0..rows {
            let line = text.get(first + i).map_or("", String::as_str);
            let mut spans =
                fit(vec![Span::new(line, Some(Slot::Text))], inner);
            pad(&mut spans, inner, None);
            let mut row = vec![Span::new("│", slot)];
            row.extend(spans);
            row.push(Span::new("│", slot));
            let y = at.y + 1 + u16::try_from(i).unwrap_or(0);
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

    fn put_at(
        &self,
        buf: &mut Buffer,
        at: (u16, u16),
        w: usize,
        spans: &[Span<'_>],
    ) {
        if at.1 < self.height.min(buf.area.height) {
            put(buf, at, w, spans, &self.styles);
        }
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
