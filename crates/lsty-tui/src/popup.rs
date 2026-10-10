//! Where the popup is on the screen, and drawing it over the list.

use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::complete::{POPUP_ROWS, Popup};
use crate::layout::BORDER;
use crate::model::Model;
use crate::text::{Span, fit, pad, put, width_of};

/// Where the popup is: its left edge, its first screen row and how many
/// rows it shows.
struct Spot {
    x: usize,
    y: usize,
    rows: usize,
}

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

    /// Below the row of the text cursor with its left edge at the start
    /// of the word, or above the row when what it would show does not
    /// all fit below and there is more room above.
    fn popup_box(&self, popup: &Popup, w: usize) -> Option<Spot> {
        let draft = self.edit.as_ref()?;
        let list = self.panes(self.dims()).list;
        let (wanted, _) = self.field_cell(popup.from)?;
        let x = wanted.min(usize::from(self.width).saturating_sub(w));
        let (_, cur) = self.field_cell(draft.field.cursor)?;
        let cur = isize::try_from(cur).ok()?;
        let inner =
            isize::try_from(usize::from(list.height).saturating_sub(BORDER))
                .ok()?;
        let below = inner - cur;
        let above = cur - 1;
        let rows = isize::try_from(popup.cands.len().min(POPUP_ROWS)).ok()?;
        let (y, rows) = if rows <= below || below >= above {
            (cur + 1, rows.min(below))
        } else {
            let rows = rows.min(above);
            (cur - rows, rows)
        };
        Some(Spot {
            x,
            y: usize::try_from(y).ok()?,
            rows: usize::try_from(rows).ok()?,
        })
    }

    /// The cells the completion popup covers, if it is open.
    pub(crate) fn popup_rect(&self) -> Option<Rect> {
        let popup = self.edit.as_ref()?.comp.popup.as_ref()?;
        let w = self.popup_width(popup);
        let at = self.popup_box(popup, w).filter(|b| b.rows > 0)?;
        let cells = |n: usize| u16::try_from(n).ok();
        Some(Rect::new(
            cells(at.x)?,
            cells(at.y)?,
            cells(w)?,
            cells(at.rows)?,
        ))
    }

    /// Draws the popup over the rows of the screen.
    pub(crate) fn draw_popup(&self, buf: &mut Buffer) {
        let Some(popup) =
            self.edit.as_ref().and_then(|d| d.comp.popup.as_ref())
        else {
            return;
        };
        let w = self.popup_width(popup);
        let Some(at) = self.popup_box(popup, w).filter(|b| b.rows > 0) else {
            return;
        };
        let (top, shown) = popup.shown(at.rows);
        for (i, cand) in shown.iter().enumerate() {
            let slot = if top + i == popup.at {
                Slot::Selection
            } else {
                Slot::StatusBg
            };
            let mut line =
                fit(vec![Span::new(cand.label.as_str(), Some(Slot::Text))], w);
            pad(&mut line, w, Some(Slot::Text));
            let (Ok(x), Ok(y)) =
                (u16::try_from(at.x), u16::try_from(at.y + i))
            else {
                continue;
            };
            if y >= self.height.min(buf.area.height) {
                break;
            }
            put(buf, (x, y), w, &line, &self.styles);
            let cells = u16::try_from(w).unwrap_or(0);
            buf.set_style(Rect::new(x, y, cells, 1), self.styles.fill(slot));
        }
    }
}
