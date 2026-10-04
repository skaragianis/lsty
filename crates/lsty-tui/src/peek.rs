//! Scrolling the detail panel with the wheel.

use crate::detail::{detail_last, detail_size};
use crate::model::Model;
use crate::track::{Place, Remap};

/// How far the detail panel is scrolled, and for which item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scrolled {
    at: Place,
    top: usize,
}

impl Model {
    /// The first line of the document that the detail panel shows. It
    /// starts again at the top for another item.
    pub(crate) fn detail_top(&self) -> usize {
        match (self.cursor_place(), &self.pointer.peek) {
            (Some(at), Some(peek)) if peek.at == at => peek.top,
            _ => 0,
        }
    }

    /// Forgets the scroll once the cursor is on another item.
    pub(crate) fn drop_peek(&mut self) {
        let at = self.cursor_place();
        if self.pointer.peek.is_some_and(|peek| at != Some(peek.at)) {
            self.pointer.peek = None;
        }
    }

    /// Follows the scrolled item through a reading of the files.
    pub(crate) fn remap_peek(&mut self, moved: &Remap) {
        let Some(peek) = &mut self.pointer.peek else {
            return;
        };
        match moved.track(peek.at) {
            Some(at) => peek.at = at,
            None => self.pointer.peek = None,
        }
    }

    /// Scrolls the detail panel by `by` lines, between the top and the
    /// view whose last line is the document's last.
    pub(crate) fn wheel_detail(&mut self, by: isize) {
        let (Some(at), Some(area)) =
            (self.cursor_place(), self.panes(self.dims()).detail)
        else {
            return;
        };
        let (cells, rows) = detail_size(area);
        let last = detail_last(self.detail_doc(cells, rows).lines.len(), rows);
        let top = self.detail_top().saturating_add_signed(by).min(last);
        self.pointer.peek = Some(Scrolled { at, top });
    }
}
