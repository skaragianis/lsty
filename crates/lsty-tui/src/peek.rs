//! Scrolling the detail panel with the wheel.

use lsty_core::keys::Action;

use crate::detail::{detail_last, detail_rows, detail_size};
use crate::model::Model;
use crate::track::{Place, Remap};

/// How far the detail panel is scrolled, and which link is selected, for
/// which item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scrolled {
    at: Place,
    top: usize,
    link: usize,
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

    /// The selected link of the item under the cursor, counted from 0.
    pub(crate) fn detail_link(&self) -> usize {
        match (self.cursor_place(), &self.pointer.peek) {
            (Some(at), Some(peek)) if peek.at == at => peek.link,
            _ => 0,
        }
        .min(self.cursor_links().len().saturating_sub(1))
    }

    /// Whether the detail panel is on the screen with an item in it.
    pub(crate) fn detail_shown(&self) -> bool {
        self.panes(self.dims()).detail.is_some()
            && self.cursor < self.rows.len()
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
        let link = self.detail_link();
        self.pointer.peek = Some(Scrolled { at, top, link });
    }

    /// Selects link `to` (the last one if there are fewer) and scrolls
    /// the panel to keep it in view.
    pub(crate) fn show_link(&mut self, to: usize) {
        let (Some(at), Some(area)) =
            (self.cursor_place(), self.panes(self.dims()).detail)
        else {
            return;
        };
        let (cells, rows) = detail_size(area);
        let doc = self.detail_doc(cells, rows);
        let Some(files) = doc.files else {
            return;
        };
        let line = Some(files + 1 + to);
        let last = detail_last(doc.lines.len(), rows);
        let top = (0..=last)
            .filter(|&t| detail_rows(doc.lines.len(), rows, t).contains(&line))
            .min_by_key(|t| t.abs_diff(self.detail_top()))
            .unwrap_or_else(|| self.detail_top());
        self.pointer.peek = Some(Scrolled { at, top, link: to });
    }

    /// Moves the selected link by `by`, stopping at the ends; an item
    /// without links scrolls the panel by lines.
    pub(crate) fn step_link(&mut self, by: isize) {
        let count = self.cursor_links().len();
        if count == 0 {
            self.wheel_detail(by);
            return;
        }
        let to = self.detail_link().saturating_add_signed(by);
        self.show_link(to.min(count - 1));
    }

    /// Does `<Tab>` (`NextLink`) or `<S-Tab>` in the detail panel: the
    /// next link, or the next place of the ring past the last one.
    pub(crate) fn tab_link(&mut self, action: Action) {
        let back = action == Action::PrevLink;
        let count = self.cursor_links().len();
        let at = self.detail_link();
        if count == 0 || (back && at == 0) || (!back && at + 1 == count) {
            self.ring_step(back);
        } else {
            self.step_link(if back { -1 } else { 1 });
        }
    }
}
