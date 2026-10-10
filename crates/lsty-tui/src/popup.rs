//! Where the popup is on the screen, and drawing it over the list.

use lsty_core::theme::Slot;
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    complete::{POPUP_ROWS, Popup},
    layout::BORDER,
    model::Model,
    text::{Span, fit, pad, put, width_of},
};

impl Model {
    /// How wide the popup is: its widest candidate, at most half of the
    /// screen.
    fn popup_width(&self, popup: &Popup) -> usize {
        let widest = popup
            .cands
            .iter()
            .map(|c| width_of(&c.label))
            .max()
            .unwrap_or(0);
        widest.min(usize::from(self.width) / 2)
    }

    /// The cells the completion popup covers, if it is open: below the
    /// row of the text cursor with its left edge at the start of the
    /// word, or above the row when what it would show does not all fit
    /// below and there is more room above.
    pub(crate) fn popup_rect(&self) -> Option<Rect> {
        let popup = self.popup()?;
        let draft = self.edit.as_ref()?;
        let w = self.popup_width(popup);
        let list = self.panes(self.dims()).list;
        let (wanted, _) = self.field_cell(popup.from)?;
        let x = wanted.min(usize::from(self.width) - w);
        let (_, cur) = self.field_cell(draft.field.cursor)?;
        let inner = usize::from(list.height).saturating_sub(BORDER);
        let below = inner.saturating_sub(cur);
        let above = cur.saturating_sub(1);
        let rows = popup.cands.len().min(POPUP_ROWS);
        let (y, rows) = if rows <= below || below >= above {
            (cur + 1, rows.min(below))
        } else {
            let rows = rows.min(above);
            (cur - rows, rows)
        };
        (rows > 0).then_some(())?;
        let cells = |n: usize| u16::try_from(n).ok();
        Some(Rect::new(cells(x)?, cells(y)?, cells(w)?, cells(rows)?))
    }

    /// Draws the popup over the rows of the screen.
    pub(crate) fn draw_popup(&self, buf: &mut Buffer) {
        let Some(r) = self.popup_rect() else {
            return;
        };
        let Some(popup) = self.popup() else {
            return;
        };
        let w = usize::from(r.width);
        let (top, shown) = popup.shown(usize::from(r.height));
        for (i, (y, cand)) in (r.y..).zip(shown).enumerate() {
            let slot = if top + i == popup.at {
                Slot::Selection
            } else {
                Slot::StatusBg
            };
            let mut line =
                fit(vec![Span::new(cand.label.as_str(), Some(Slot::Text))], w);
            pad(&mut line, w, Some(Slot::Text));
            if y >= self.height.min(buf.area.height) {
                break;
            }
            put(buf, (r.x, y), w, &line, &self.styles);
            let cells = Rect::new(r.x, y, r.width, 1);
            buf.set_style(cells, self.styles.fill(slot));
        }
    }
}
