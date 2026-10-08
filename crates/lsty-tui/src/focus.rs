//! Where the keyboard is, and what shows the sidebar.

use lsty_core::keys::{Action, Mode};

use crate::model::Model;

/// Where the keyboard is: the list or a panel of the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    List,
    Projects,
    Contexts,
    Filters,
}

/// The actions that move the focus.
pub(crate) const MOVES: [Action; 10] = [
    Action::FocusNext,
    Action::FocusPrev,
    Action::FocusLeft,
    Action::FocusRight,
    Action::FocusDown,
    Action::FocusUp,
    Action::FocusList,
    Action::FocusProjects,
    Action::FocusContexts,
    Action::FocusFilters,
];

/// The places in the order that Tab visits them.
const ORDER: [Focus; 4] = [
    Focus::List,
    Focus::Projects,
    Focus::Contexts,
    Focus::Filters,
];

impl Focus {
    fn position(self) -> usize {
        ORDER.iter().position(|f| *f == self).unwrap_or(0)
    }

    fn at(position: usize) -> Self {
        ORDER
            .get(position % ORDER.len())
            .copied()
            .unwrap_or(Self::List)
    }

    /// The index of the sidebar panel, `None` for the list.
    pub(crate) fn panel(self) -> Option<usize> {
        self.position().checked_sub(1)
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
        } else if self.focus == Focus::List {
            self.mode()
        } else {
            Mode::Panel
        }
    }

    /// Does one of the actions that move the focus.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the focus actions get here"
    )]
    pub(crate) fn move_focus(&mut self, action: Action) {
        let here = self.focus.position();
        match action {
            Action::FocusNext => self.set_focus(Focus::at(here + 1)),
            Action::FocusPrev => {
                self.set_focus(Focus::at(here + ORDER.len() - 1));
            }
            Action::FocusLeft if self.focus == Focus::List => {
                self.set_focus(self.last);
            }
            Action::FocusRight | Action::FocusList => {
                self.set_focus(Focus::List);
            }
            Action::FocusDown => self.step_focus(1),
            Action::FocusUp => self.step_focus(-1),
            Action::FocusProjects => self.set_focus(Focus::Projects),
            Action::FocusContexts => self.set_focus(Focus::Contexts),
            Action::FocusFilters => self.set_focus(Focus::Filters),
            _ => {}
        }
    }

    /// Moves to the next or previous panel of the sidebar, within it.
    fn step_focus(&mut self, step: i32) {
        let Some(panel) = self.focus.panel() else {
            return;
        };
        let last = self.panels.len() - 1;
        let to = if step < 0 {
            panel.saturating_sub(1)
        } else {
            (panel + 1).min(last)
        };
        self.set_focus(Focus::of_panel(to));
    }

    pub(crate) fn set_focus(&mut self, to: Focus) {
        if to == self.focus {
            return;
        }
        if to == Focus::List {
            self.overlay = false;
        } else {
            self.last = to;
            self.overlay = self.overlay || !self.docked();
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
        if self.focus != Focus::List && !self.sidebar_shown() {
            self.focus = Focus::List;
        }
        self.scroll();
        self.scroll_panels();
    }
}
