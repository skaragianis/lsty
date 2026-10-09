//! The mouse: clicks, drags and the wheel.

use jiff::Timestamp;
use lsty_core::keys::Action;

use crate::focus::Focus;
use crate::hit::{Kind, Pane, Target};
use crate::model::Model;
use crate::msg::{Button, Cmd, Gesture, Mouse};
use crate::peek::Scrolled;
use crate::sidebar::FILTERS;
use crate::track::{Place, Remap};

/// Two clicks on one item within this many nanoseconds make a
/// double-click.
const DOUBLE_CLICK: i128 = 300_000_000;
/// How many rows one notch of the wheel moves.
const WHEEL_STEP: isize = 3;
/// The narrowing filter that the progress part of the status line
/// selects.
const DOING: &str = "Doing";

/// The left button held down, and where it went down.
#[derive(Clone, Debug)]
struct Press {
    at: Target,
    anchor: Option<Place>,
    shift: bool,
    dragging: bool,
}

/// The last click on a row, for telling a double-click.
#[derive(Clone, Copy, Debug)]
struct Click {
    at: Place,
    when: Timestamp,
}

/// What the mouse has done so far that later events need.
#[derive(Debug, Default)]
pub(crate) struct Pointer {
    held: Option<Press>,
    last: Option<Click>,
    /// How far the wheel scrolled the detail panel.
    pub(crate) peek: Option<Scrolled>,
}

impl Model {
    /// Does what a mouse event asks for. Only the left button acts.
    pub(crate) fn mouse(&mut self, event: Mouse) -> Vec<Cmd> {
        if self.line.is_some()
            || self.opening.is_some()
            || self.choosing.is_some()
        {
            return Vec::new();
        }
        match event.gesture {
            Gesture::Wheel { up } => self.mouse_wheel(event, up),
            Gesture::Down(Button::Left) => self.mouse_press(event),
            Gesture::Drag(Button::Left) => self.mouse_drag(event),
            Gesture::Up(_) => return self.mouse_release(event),
            Gesture::Down(Button::Other) | Gesture::Drag(Button::Other) => {}
        }
        Vec::new()
    }

    /// Runs a change that a gesture asks for, with the message line
    /// cleared as a key press clears it.
    fn acting<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> T {
        self.clear_message();
        let out = change(self);
        if self.message.is_none() {
            self.next_warning();
        }
        out
    }

    fn mouse_press(&mut self, event: Mouse) {
        if self.output.is_some() {
            return;
        }
        let t = self.hit(usize::from(event.x), usize::from(event.y));
        if self.edit.is_some() {
            if t.kind == Kind::Field {
                self.place_text(t.at);
            }
            return;
        }
        let anchor = self
            .rows
            .get(t.at)
            .filter(|_| t.on_row())
            .map(|row| (row.source, row.nr));
        self.pointer.held = Some(Press {
            at: t,
            anchor,
            shift: event.shift,
            dragging: false,
        });
    }

    /// Puts the text cursor of the field at position `at`.
    fn place_text(&mut self, at: usize) {
        if let Some(draft) = &mut self.edit {
            draft.field.cursor = at;
            draft.field.col = None;
        }
        self.follow_word();
        self.scroll();
    }

    /// Follows the places a held press and the last click name through
    /// a reading of the files.
    pub(crate) fn remap_pointer(&mut self, moved: &Remap) {
        if let Some(held) = &mut self.pointer.held
            && held.at.on_row()
        {
            match held.anchor.and_then(|at| moved.track(at)) {
                Some(at) => held.anchor = Some(at),
                None => self.pointer.held = None,
            }
        }
        if let Some(last) = &mut self.pointer.last {
            match moved.track(last.at) {
                Some(at) => last.at = at,
                None => self.pointer.last = None,
            }
        }
    }

    fn mouse_release(&mut self, event: Mouse) -> Vec<Cmd> {
        let Some(press) = self.pointer.held.take() else {
            return Vec::new();
        };
        let at = self.hit(usize::from(event.x), usize::from(event.y));
        if press.dragging || at != press.at {
            return Vec::new();
        }
        self.acting(|m| m.click(&press))
    }

    fn click(&mut self, press: &Press) -> Vec<Cmd> {
        let t = &press.at;
        if t.kind != Kind::Row {
            self.pointer.last = None;
        }
        match t.kind {
            Kind::Row => self.click_row(press),
            Kind::Tag => self.click_tag(t),
            Kind::Entry | Kind::Toggle => self.click_entry(t.panel, t.at),
            Kind::Fold => self.click_fold(t.panel, t.at),
            Kind::Chip => self.clear_chip(t.at),
            Kind::Progress => {
                self.pick(FILTERS, Some(DOING.as_bytes().to_vec()));
            }
            Kind::Link => return self.click_link(t),
            Kind::None => self.click_panel(t),
            Kind::Field | Kind::Popup => {}
        }
        Vec::new()
    }

    /// A click that hits no action gives the keyboard to its panel.
    fn click_panel(&mut self, t: &Target) {
        match t.pane {
            Pane::List => self.set_focus(Focus::List),
            Pane::Sidebar => {
                self.exit_visual();
                self.set_focus(Focus::of_panel(t.panel));
            }
            Pane::None | Pane::Detail | Pane::Popup => {}
        }
    }

    fn click_row(&mut self, press: &Press) {
        let i = press.at.at;
        let Some(row) = self.rows.get(i) else {
            return;
        };
        let at = (row.source, row.nr);
        let again = self.doubled(at);
        self.set_focus(Focus::List);
        if press.shift {
            if self.visual.is_none() {
                self.enter_visual();
            }
            self.cursor_to_row(i);
            return;
        }
        self.exit_visual();
        self.cursor_to_row(i);
        self.pointer.last = Some(Click {
            at,
            when: (self.deps.now)(),
        });
        if again {
            self.pointer.last = None;
            self.open_edit(Action::EditAppend);
        }
    }

    /// Whether a click on the item at `at` now makes a double-click.
    fn doubled(&self, at: Place) -> bool {
        let Some(last) = self.pointer.last else {
            return false;
        };
        let since =
            (self.deps.now)().as_nanosecond() - last.when.as_nanosecond();
        last.at == at && since <= DOUBLE_CLICK
    }

    /// Moves the cursor to row `i`, as a motion would.
    pub(crate) fn cursor_to_row(&mut self, i: usize) {
        let was = self.cursor_place();
        self.cursor = i.min(self.rows.len().saturating_sub(1));
        self.scroll();
        self.release(was);
    }

    fn click_tag(&mut self, t: &Target) {
        if t.pane == Pane::Detail {
            self.pick(t.panel, Some(t.name.clone()));
            return;
        }
        let Some(row) = self.rows.get(t.at) else {
            return;
        };
        let at = (row.source, row.nr);
        self.set_focus(Focus::List);
        self.exit_visual();
        self.pick(t.panel, Some(t.name.clone()));
        self.cursor_to(Some(at));
    }

    /// Clears the filter that chip `i` shows.
    fn clear_chip(&mut self, i: usize) {
        let Some(chip) = self.chips().into_iter().nth(i) else {
            return;
        };
        if let Some(panel) = chip.panel {
            self.pick(panel, None);
            return;
        }
        self.search.clear();
        self.refresh();
    }

    fn mouse_drag(&mut self, event: Mouse) {
        if self.output.is_some() || self.edit.is_some() {
            return;
        }
        let Some(mut press) = self.pointer.held.take() else {
            return;
        };
        if !press.at.on_row() {
            self.pointer.held = Some(press);
            return;
        }
        let Some(anchor) = press.anchor.and_then(|at| self.row_at(at)) else {
            return;
        };
        let under = self.row_under(usize::from(event.y));
        if let Some(i) = under.filter(|&i| press.dragging || i != anchor) {
            self.acting(|m| m.drag_to(&mut press, i));
        }
        self.pointer.held = Some(press);
    }

    /// The row at screen row `y`. Beyond the top or the bottom of the
    /// pane it scrolls one row and gives the row at that edge.
    fn row_under(&mut self, y: usize) -> Option<usize> {
        let top = usize::from(self.panes(self.dims()).list.y) + 1;
        let rows = self.pane_rows();
        let last = self.rows.len().checked_sub(1)?;
        let line = if y < top {
            self.top = self.top.saturating_sub(1);
            0
        } else if y - top >= rows {
            let most = self.rows.len().saturating_sub(rows);
            self.top = (self.top + 1).min(most);
            rows.saturating_sub(1)
        } else {
            y - top
        };
        Some((self.top + line).min(last))
    }

    fn drag_to(&mut self, press: &mut Press, i: usize) {
        if !press.dragging {
            press.dragging = true;
            self.visual = press.anchor;
            self.set_focus(Focus::List);
            self.settle_mode();
        }
        self.cursor_to_row(i);
    }

    fn mouse_wheel(&mut self, event: Mouse, up: bool) {
        let by = if up { -WHEEL_STEP } else { WHEEL_STEP };
        if self.output.is_some() {
            self.wheel_output(by);
            return;
        }
        if self.edit.is_some() {
            return;
        }
        let t = self.hit(usize::from(event.x), usize::from(event.y));
        match t.pane {
            Pane::List => self.wheel_list(by),
            Pane::Sidebar => self.wheel_panel(t.panel, by),
            Pane::Detail => self.wheel_detail(by),
            Pane::None | Pane::Popup => {}
        }
    }

    /// Scrolls the view; the cursor stays on screen.
    fn wheel_list(&mut self, by: isize) {
        let rows = self.pane_rows();
        let most = self.rows.len().saturating_sub(rows);
        self.top = self.top.saturating_add_signed(by).min(most);
        let Some(last) = self.rows.len().checked_sub(1) else {
            return;
        };
        let was = self.cursor_place();
        let end = (self.top + rows.saturating_sub(1)).min(last);
        self.cursor = self.cursor.clamp(self.top.min(end), end);
        self.release(was);
    }

    fn wheel_panel(&mut self, i: usize, by: isize) {
        let rows = self.panel_rows()[i];
        let Some(panel) = self.panels.get_mut(i) else {
            return;
        };
        let most = panel.entries.len().saturating_sub(rows);
        panel.top = panel.top.saturating_add_signed(by).min(most);
    }
}
