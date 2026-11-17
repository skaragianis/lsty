//! Where the keyboard is, and what shows the sidebar.

use lsty_core::{
    config::{
        DEFAULT_LIST_ROWS,
        DetailPosition::{Bottom, Right},
    },
    keys::{Action, Mode},
};

use crate::{model::Model, sidebar::FILTERS, text::scrolled};

/// Where the keyboard is: the list, a panel of the sidebar or the detail
/// panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    List,
    Projects,
    Contexts,
    Filters,
    Detail,
}

/// The sidebar and the list by position.
const ORDER: [Focus; 4] = [
    Focus::List,
    Focus::Projects,
    Focus::Contexts,
    Focus::Filters,
];

/// The places in the order that Tab visits them.
const RING: [Focus; 5] = [
    Focus::List,
    Focus::Detail,
    Focus::Projects,
    Focus::Contexts,
    Focus::Filters,
];

impl Focus {
    fn position(self) -> usize {
        self as usize
    }

    fn at(position: usize) -> Self {
        ORDER[position % ORDER.len()]
    }

    /// The index of the sidebar panel, `None` for the list and the
    /// detail panel.
    pub(crate) fn panel(self) -> Option<usize> {
        match self {
            Self::List | Self::Detail => None,
            Self::Projects | Self::Contexts | Self::Filters => {
                self.position().checked_sub(1)
            }
        }
    }

    /// The focus on sidebar panel `panel`.
    pub(crate) fn of_panel(panel: usize) -> Self {
        Self::at(panel + 1)
    }
}

/// The part of the screen that has the keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keys {
    /// The popup of links to open.
    Picker,
    /// The theme chooser.
    Theme,
    /// The output view, or the help.
    Output,
    /// The prompt line.
    Line,
    /// The field of a task being written.
    Field,
    /// The list and its panels.
    List,
}

impl Model {
    /// The part that has the keys now.
    pub(crate) fn keys_to(&self) -> Keys {
        if self.opening.is_some() {
            Keys::Picker
        } else if self.choosing.is_some() {
            Keys::Theme
        } else if self.output.is_some() {
            Keys::Output
        } else if self.line.is_some() {
            Keys::Line
        } else if self.edit.is_some() {
            Keys::Field
        } else {
            Keys::List
        }
    }

    /// The key table that applies now.
    pub(crate) fn key_mode(&self) -> Mode {
        self.mode_of(self.keys_to())
    }

    /// The key table of the part that has the keys.
    pub(crate) fn mode_of(&self, keys: Keys) -> Mode {
        match keys {
            Keys::Theme if self.typing_theme() => Mode::Command,
            Keys::Picker | Keys::Theme => Mode::Picker,
            Keys::Output if self.typing_help() => Mode::Command,
            Keys::Output => Mode::Help,
            Keys::Line => Mode::Command,
            Keys::Field if self.popup().is_some() => Mode::Completion,
            Keys::Field => Mode::Insert,
            Keys::List => match self.focus {
                Focus::List => self.mode(),
                Focus::Detail => Mode::Detail,
                Focus::Projects | Focus::Contexts | Focus::Filters => {
                    Mode::Panel
                }
            },
        }
    }

    /// Does one of the actions that move the focus.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the focus actions get here"
    )]
    pub(crate) fn move_focus(&mut self, action: Action) {
        match action {
            Action::FocusNext => self.ring_step(false),
            Action::FocusPrev => self.ring_step(true),
            other => {
                if let Some(to) = self.focus_after(other) {
                    self.set_focus(to);
                }
            }
        }
    }

    /// Where a focus action other than Tab's moves the focus, when it does.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the focus actions get here"
    )]
    pub(crate) fn focus_after(&self, action: Action) -> Option<Focus> {
        let to = match action {
            Action::FocusLeft
                if self.focus == Focus::Detail && !self.detail_below() =>
            {
                Focus::List
            }
            Action::FocusLeft => self.last,
            Action::FocusRight if self.focus == Focus::List => {
                (!self.detail_below()).then_some(Focus::Detail)?
            }
            Action::FocusList | Action::FocusRight => Focus::List,
            Action::FocusDetail => Focus::Detail,
            Action::FocusDown => self.step_focus(true)?,
            Action::FocusUp => self.step_focus(false)?,
            Action::FocusProjects => Focus::Projects,
            Action::FocusContexts => Focus::Contexts,
            Action::FocusFilters => Focus::Filters,
            _ => return None,
        };
        self.can_focus(to).then_some(to)
    }

    /// Moves to the next or previous place of the ring that Tab walks,
    /// skipping the detail panel when it is not shown. Entering the
    /// detail panel selects its first link, or its last going back.
    pub(crate) fn ring_step(&mut self, back: bool) {
        let mut at = RING.iter().position(|&f| f == self.focus).unwrap_or(0);
        let to = loop {
            at = (at + if back { RING.len() - 1 } else { 1 }) % RING.len();
            if RING[at] != Focus::Detail || self.detail_shown() {
                break RING[at];
            }
        };
        self.set_focus(to);
        if to == Focus::Detail {
            let last = self.cursor_links().len().saturating_sub(1);
            self.show_link(if back { last } else { 0 });
        }
    }

    /// Whether the detail panel is under the list, not beside it.
    fn detail_below(&self) -> bool {
        self.geometry(self.dims()).below
    }

    /// The next or previous panel of the sidebar, within it; between the
    /// list and a detail panel under it.
    fn step_focus(&self, down: bool) -> Option<Focus> {
        let Some(panel) = self.focus.panel() else {
            let to = if down { Focus::Detail } else { Focus::List };
            return self.detail_below().then_some(to);
        };
        let to = if down {
            (panel + 1).min(FILTERS)
        } else {
            panel.saturating_sub(1)
        };
        Some(Focus::of_panel(to))
    }

    /// Whether the focus can go to `to`: elsewhere, and where it shows.
    fn can_focus(&self, to: Focus) -> bool {
        to != self.focus && (to != Focus::Detail || self.detail_shown())
    }

    pub(crate) fn set_focus(&mut self, to: Focus) {
        if !self.can_focus(to) {
            return;
        }
        match to {
            Focus::List | Focus::Detail => self.overlay = false,
            Focus::Projects | Focus::Contexts | Focus::Filters => {
                self.last = to;
                self.overlay = self.overlay || !self.docked();
            }
        }
        self.focus = to;
        self.relayout();
    }

    /// `gs`: hides the sidebar, or shows it beside the list, or over it
    /// where it does not fit.
    pub(crate) fn toggle_sidebar(&mut self) {
        if self.sidebar_shown() {
            self.overlay = false;
            self.sidebar_on = self.sidebar_on && !self.roomy();
        } else if self.roomy() {
            self.sidebar_on = true;
        } else {
            self.overlay = true;
        }
        self.relayout();
    }

    /// `gi`: moves the detail panel from the place it is drawn at to the
    /// next one that has room, starting at `detail_position` and ending
    /// hidden.
    pub(crate) fn toggle_detail(&mut self) {
        let first = self.deps.config.detail_position;
        let order = [first, if first == Right { Bottom } else { Right }];
        let at = self
            .detail_place()
            .map_or(0, |now| 1 + usize::from(now != first));
        self.detail = order[at..].iter().copied().find(|&place| {
            self.detail = Some(place);
            self.detail_place() == Some(place)
        });
        self.relayout();
    }

    /// The rows a task may take in the list: `set`, or fewer when the
    /// list shows fewer.
    fn rows_up_to(&self, set: u64) -> usize {
        let set = usize::try_from(set).unwrap_or(usize::MAX);
        set.min(self.pane_rows().max(1))
    }

    /// The rows a task may take in the list.
    pub(crate) fn list_rows(&self) -> usize {
        self.rows_up_to(self.deps.config.list_rows)
    }

    /// The rows the longest task takes: `list_rows`, or fewer when no
    /// task needs that many.
    pub(crate) fn rows_shown(&self) -> usize {
        self.list_rows().min(self.spread().most())
    }

    /// Whether a task needs a row more than `list_rows` and the list has
    /// room for it.
    pub(crate) fn can_add_row(&self) -> bool {
        self.list_rows() < self.spread().most().min(self.pane_rows())
    }

    /// Whether `0` changes the rows a task may take: it does when they
    /// are not the default, even where the list is too short to show it.
    pub(crate) fn can_default_rows(&self) -> bool {
        self.deps.config.list_rows != DEFAULT_LIST_ROWS
    }

    /// A task may take `rows` rows. The cursor row stays on its line of
    /// the list when the list had it in view, the task at the top cut if
    /// need be, until the next scroll.
    fn set_list_rows(&mut self, rows: u64) {
        let seen = self.edit.is_none()
            && !self.rows.is_empty()
            && !self.cursor_hidden();
        let line = self.sized(|size| {
            let above: usize =
                (self.top..self.cursor).map(|i| size.height(i)).sum();
            above.saturating_sub(size.cut.1)
        });
        self.deps.config.list_rows = rows;
        if seen {
            let at = self.cursor;
            let (top, skip) = self.sized(|size| size.anchored(at, line));
            self.top = top;
            self.cut = (top, skip);
        } else {
            self.scroll();
        }
    }

    /// `+`: a task may take one more row, while one needs it.
    pub(crate) fn more_rows(&mut self) {
        if self.can_add_row() {
            self.set_list_rows(self.deps.config.list_rows + 1);
        }
    }

    /// `0`: a task may take the default rows again.
    pub(crate) fn default_rows(&mut self) {
        if self.can_default_rows() {
            self.set_list_rows(DEFAULT_LIST_ROWS);
        }
    }

    /// `-`: a task may take one row fewer than the longest takes.
    pub(crate) fn fewer_rows(&mut self) {
        let shown = self.rows_shown();
        if shown > 1 {
            self.set_list_rows(shown as u64 - 1);
        }
    }

    /// Takes the new size of the terminal. An overlay moves beside the
    /// list once there is room.
    pub(crate) fn resize(&mut self, width: u16, height: u16) {
        let away = self.cursor_hidden().then_some(self.top);
        let rows = self.panel_rows();
        let tops: Vec<_> = (self.panels.iter().zip(rows))
            .map(|(p, rows)| (!p.seen(rows)).then_some(p.top))
            .collect();
        self.width = width;
        self.height = height;
        if self.overlay && self.roomy() {
            self.sidebar_on = true;
            self.overlay = false;
        }
        self.relayout();
        if let Some(top) = away {
            self.keep_top(top);
        }
        let rows = self.panel_rows();
        for ((panel, top), rows) in self.panels.iter_mut().zip(tops).zip(rows)
        {
            if let Some(top) = top {
                panel.top = scrolled(top, 0, panel.entries.len(), rows);
            }
        }
    }

    /// Brings the views in line with the size and with what is shown.
    pub(crate) fn relayout(&mut self) {
        self.settle_focus();
        if self.focus == Focus::Detail {
            self.show_link(self.detail_link());
        }
        self.scroll();
        self.scroll_panels();
    }

    /// Gives the keyboard back to the list when its panel is not shown.
    pub(crate) fn settle_focus(&mut self) {
        let gone = match self.focus {
            Focus::List => false,
            Focus::Detail => !self.detail_shown(),
            Focus::Projects | Focus::Contexts | Focus::Filters => {
                !self.sidebar_shown()
            }
        };
        if gone {
            self.focus = Focus::List;
        }
    }
}
