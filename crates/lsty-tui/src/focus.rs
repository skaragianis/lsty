//! Where the keyboard is, and what shows the sidebar.

use lsty_core::keys::{Action, Mode};

use crate::{model::Model, sidebar::FILTERS};

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

impl Model {
    /// The key table that applies now.
    pub(crate) fn key_mode(&self) -> Mode {
        if self.opening.is_some() {
            Mode::Picker
        } else if self.typing_theme() {
            Mode::Command
        } else if self.choosing.is_some() {
            Mode::Picker
        } else if self.typing_help() {
            Mode::Command
        } else if self.output.is_some() {
            Mode::Help
        } else if self.line.is_some() {
            Mode::Command
        } else if let Some(draft) = &self.edit {
            if draft.comp.popup.is_some() {
                Mode::Completion
            } else {
                Mode::Insert
            }
        } else {
            match self.focus {
                Focus::List => self.mode(),
                Focus::Detail => Mode::Detail,
                Focus::Projects | Focus::Contexts | Focus::Filters => {
                    Mode::Panel
                }
            }
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
            Action::FocusLeft if self.focus == Focus::List => {
                self.set_focus(self.last);
            }
            Action::FocusLeft if self.focus == Focus::Detail => {
                self.set_focus(Focus::List);
            }
            Action::FocusRight if self.focus == Focus::List => {
                self.set_focus(Focus::Detail);
            }
            Action::FocusList | Action::FocusRight => {
                self.set_focus(Focus::List);
            }
            Action::FocusDetail => self.set_focus(Focus::Detail),
            Action::FocusDown => self.step_focus(true),
            Action::FocusUp => self.step_focus(false),
            Action::FocusProjects => self.set_focus(Focus::Projects),
            Action::FocusContexts => self.set_focus(Focus::Contexts),
            Action::FocusFilters => self.set_focus(Focus::Filters),
            _ => {}
        }
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

    /// Moves to the next or previous panel of the sidebar, within it.
    fn step_focus(&mut self, down: bool) {
        let Some(panel) = self.focus.panel() else {
            return;
        };
        let to = if down {
            (panel + 1).min(FILTERS)
        } else {
            panel.saturating_sub(1)
        };
        self.set_focus(Focus::of_panel(to));
    }

    pub(crate) fn set_focus(&mut self, to: Focus) {
        if to == self.focus || (to == Focus::Detail && !self.detail_shown()) {
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
        if to == Focus::Detail {
            self.show_link(self.detail_link());
        }
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

    /// `gi`.
    pub(crate) fn toggle_detail(&mut self) {
        self.detail_on = !self.detail_on;
        self.relayout();
    }

    /// Takes the new size of the terminal. An overlay moves beside the
    /// list once there is room.
    pub(crate) fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        if self.overlay && self.roomy() {
            self.sidebar_on = true;
            self.overlay = false;
        }
        self.relayout();
    }

    /// Brings the views in line with the size and with what is shown.
    pub(crate) fn relayout(&mut self) {
        self.settle_focus();
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
