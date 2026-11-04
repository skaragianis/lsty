//! Scrolling the detail panel with the wheel.

use lsty_core::keys::Action;

use crate::{
    detail::{detail_last, detail_rows, detail_size},
    model::Model,
    motion::Step,
    track::{Place, Remap},
};

/// How far the detail panel is scrolled, and which link is selected, for
/// which task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scrolled {
    at: Place,
    top: usize,
    link: usize,
}

impl Model {
    /// The first line of the document that the detail panel shows. It
    /// starts again at the top for another task.
    pub(crate) fn detail_top(&self) -> usize {
        match (self.cursor_place(), &self.pointer.peek) {
            (Some(at), Some(peek)) if peek.at == at => peek.top,
            _ => 0,
        }
    }

    /// The selected link of the task under the cursor, counted from 0.
    pub(crate) fn detail_link(&self) -> usize {
        match (self.cursor_place(), &self.pointer.peek) {
            (Some(at), Some(peek)) if peek.at == at => peek.link,
            _ => 0,
        }
        .min(self.cursor_links().len().saturating_sub(1))
    }

    /// Whether the detail panel is on the screen with a task in it.
    pub(crate) fn detail_shown(&self) -> bool {
        self.panes(self.dims()).detail.is_some()
            && self.cursor < self.rows.len()
    }

    /// Forgets the scroll once the cursor is on another task.
    pub(crate) fn drop_peek(&mut self) {
        let at = self.cursor_place();
        if self.pointer.peek.is_some_and(|peek| at != Some(peek.at)) {
            self.pointer.peek = None;
        }
    }

    /// Follows the scrolled task through a reading of the files.
    pub(crate) fn remap_peek(&mut self, moved: &Remap) {
        let Some(peek) = &mut self.pointer.peek else {
            return;
        };
        match moved.track(peek.at) {
            Some(at) => peek.at = at,
            None => self.pointer.peek = None,
        }
    }

    /// Scrolls the detail panel, between the top and the view whose
    /// last line is the document's last.
    pub(crate) fn scroll_detail(&mut self, step: Step) {
        let (Some(at), Some(area)) =
            (self.cursor_place(), self.panes(self.dims()).detail)
        else {
            return;
        };
        let (cells, rows) = detail_size(area);
        let last = detail_last(self.detail_doc(cells, rows).lines.len(), rows);
        let top = step.text_top(last, self.detail_top());
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

    /// Moves the selected link, stopping at the ends; a task without
    /// links scrolls the panel by lines.
    pub(crate) fn step_link(&mut self, step: Step) {
        match self.cursor_links().len() {
            0 => self.scroll_detail(step),
            n => {
                let at = Some(self.detail_link());
                self.show_link(step.to((n, n), at, 0).0);
            }
        }
    }

    /// Does a motion in the detail panel.
    pub(crate) fn detail_motion(&mut self, action: Action, count: usize) {
        let Some(area) = self.panes(self.dims()).detail else {
            return;
        };
        if let Some(step) = Step::of(action, count, detail_size(area).1) {
            self.step_link(step);
        }
    }

    /// Whether `<Tab>` or `<S-Tab>` leaves the detail panel: no link is next.
    fn tab_leaves(&self, back: bool) -> bool {
        let count = self.cursor_links().len();
        let at = self.detail_link();
        count == 0 || (back && at == 0) || (!back && at + 1 == count)
    }

    /// Does `<Tab>` or `<S-Tab>` in the detail panel.
    pub(crate) fn tab_link(&mut self, action: Action) {
        let back = action == Action::PrevLink;
        if self.tab_leaves(back) {
            self.ring_step(back);
        } else {
            self.step_link(Step::By(if back { -1 } else { 1 }));
        }
    }

    /// What the hint of `action` says where `<Tab>` or `<S-Tab>` leaves the
    /// panel.
    pub(crate) fn tab_word(&self, action: Action) -> Option<&'static str> {
        let (back, to) = if action == Action::NextLink {
            (false, Action::FocusProjects)
        } else if action == Action::PrevLink {
            (true, Action::FocusList)
        } else {
            return None;
        };
        self.tab_leaves(back).then(|| to.hint())
    }
}
