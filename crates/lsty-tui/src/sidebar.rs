//! The sidebar: the Projects, Contexts and Filters panels.

use lsty_core::{config::Hidden, keys::Action, theme::Slot, todotxt::Kind};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    connective,
    focus::Focus,
    layout::{BORDER, cells},
    model::Model,
    motion::Step,
    panel::{Entry, Panel, row_spans},
    text::{RULE, Span, put, scrolled, titled_spans},
};

pub(crate) const PROJECTS: usize = 0;
pub(crate) const CONTEXTS: usize = 1;
pub(crate) const FILTERS: usize = 2;
const PANELS: usize = 3;
/// The panel titles, numbered as the keys that focus them.
const TITLES: [&str; PANELS] = ["[3]─Projects", "[4]─Contexts", "[5]─Filters"];
const TOGGLE_ON: &str = "[x] ";
const TOGGLE_OFF: &str = "[ ] ";
/// The switches at the top of the Filters panel, in order.
const TOGGLES: [(Hidden, &str); 4] = [
    (Hidden::Done, "Done"),
    (Hidden::Later, "Later"),
    (Hidden::Blocked, "Blocked"),
    (Hidden::Archive, "Archive"),
];

/// The lines a panel with `entries` entries takes, borders included. The
/// last panel's bottom border is the sidebar's.
const fn lines_for(i: usize, entries: usize) -> usize {
    entries + if i == FILTERS { 2 } else { 1 }
}

/// The panels in the order they take lines.
const PRIORITY: [usize; PANELS] = [CONTEXTS, PROJECTS, FILTERS];

/// The least lines a panel has: its title and one entry.
const FLOORS: [usize; PANELS] = [
    lines_for(PROJECTS, 1),
    lines_for(CONTEXTS, 1),
    lines_for(FILTERS, 1),
];

/// Splits `h` lines between the panels, borders included. Each starts at
/// a third (the lines left over go to Contexts, then Projects, and no
/// panel starts below its floor). Then Contexts and Projects, in that
/// order, take what their entries need that their third lacks from the
/// panels after them, the last first, each down to its floor. A panel that
/// needs less than its third keeps the rest. A sidebar too short for the
/// floors is `crowded`; the lines always add up to `h`.
pub(crate) fn panel_heights(
    h: usize,
    entries: [usize; PANELS],
) -> [usize; PANELS] {
    if h < FLOORS.iter().sum() {
        return crowded(h);
    }
    let mut heights = FLOORS.map(|floor| floor.max(h / 3));
    let left = h - heights.iter().sum::<usize>();
    heights[CONTEXTS] += left.min(1);
    heights[PROJECTS] += left / 2;
    for (rank, &i) in PRIORITY[..2].iter().enumerate() {
        for &donor in PRIORITY[rank + 1..].iter().rev() {
            let lacks = lines_for(i, entries[i]).saturating_sub(heights[i]);
            let take = lacks.min(heights[donor] - FLOORS[donor]);
            heights[i] += take;
            heights[donor] -= take;
        }
    }
    heights
}

/// The heights of a sidebar too short for the floors: titles first, then
/// one entry each, in priority order. The floors sum to the lines it asks
/// for, so none are left.
fn crowded(h: usize) -> [usize; PANELS] {
    let mut heights = [0; PANELS];
    let mut left = h;
    for floor in [0, 1] {
        for i in PRIORITY {
            let give =
                lines_for(i, floor).saturating_sub(heights[i]).min(left);
            heights[i] += give;
            left -= give;
        }
    }
    heights
}

/// The entry rows of each panel in a sidebar `h` lines high.
fn panel_rows_in(h: usize, entries: [usize; PANELS]) -> [usize; PANELS] {
    let heights = panel_heights(h, entries);
    std::array::from_fn(|i| heights[i].saturating_sub(lines_for(i, 0)))
}

impl Model {
    pub(crate) fn new_panels() -> [Panel; PANELS] {
        [Panel::tree("+"), Panel::tree("@"), Panel::list()]
    }

    /// Lists the projects, contexts and filters again after the tasks
    /// changed.
    /// A panel scrolls to its cursor only if that was in view before.
    pub(crate) fn rebuild_panels(&mut self) {
        let rows = self.panel_rows();
        let before: Vec<_> = (self.panels.iter_mut().zip(rows))
            .map(|(p, rows)| (p.seen(rows), std::mem::take(&mut p.entries)))
            .collect();
        self.panels[PROJECTS].set_nodes(&self.snap, Kind::Project);
        self.panels[CONTEXTS].set_nodes(&self.snap, Kind::Context);
        let entries = self.filter_entries();
        self.panels[FILTERS].set_entries(entries);
        let rows = self.panel_rows();
        for ((panel, (seen, was)), rows) in
            self.panels.iter_mut().zip(&before).zip(rows)
        {
            panel.land(was, *seen, rows);
        }
    }

    fn filter_entries(&self) -> Vec<Entry> {
        let mut entries = Vec::new();
        for (what, label) in TOGGLES {
            let box_ = if self.shows.on(what) {
                TOGGLE_ON
            } else {
                TOGGLE_OFF
            };
            entries.push(Entry::toggle(what, format!("{box_}{label}")));
        }
        entries.push(Entry::rule());
        let active = &self.panels[FILTERS].active;
        entries.extend(self.catalog.iter().map(|e| {
            Entry::filter(&e.name, connective::mark(active, e.name.as_bytes()))
        }));
        entries
    }

    /// The entry rows of each panel at the current height.
    pub(crate) fn panel_rows(&self) -> [usize; PANELS] {
        let h = usize::from(self.height).saturating_sub(1);
        panel_rows_in(h, self.entry_counts())
    }

    fn entry_counts(&self) -> [usize; PANELS] {
        self.panels.each_ref().map(|p| p.entries.len())
    }

    /// Which panels show their cursor now.
    fn panels_seen(&self) -> [bool; PANELS] {
        let rows = self.panel_rows();
        std::array::from_fn(|i| self.panels[i].seen(rows[i]))
    }

    /// Fits the views to the rows after the entries of a panel changed: a
    /// panel follows its cursor if it was in view (`seen`), else it only
    /// stays inside its entries.
    fn refit_panels(&mut self, seen: [bool; PANELS]) {
        let rows = self.panel_rows();
        for ((panel, seen), rows) in self.panels.iter_mut().zip(seen).zip(rows)
        {
            if seen {
                panel.scroll(rows);
            } else {
                panel.top = scrolled(panel.top, 0, panel.entries.len(), rows);
            }
        }
    }

    pub(crate) fn scroll_panels(&mut self) {
        let rows = self.panel_rows();
        for (panel, rows) in self.panels.iter_mut().zip(rows) {
            panel.scroll(rows);
        }
    }

    /// The panel that has the focus.
    pub(crate) fn focused_panel(&self) -> Option<&Panel> {
        self.panels.get(self.focus.panel()?)
    }

    /// `h`: folds the node under the cursor, or the parent of a leaf.
    pub(crate) fn fold(&mut self) {
        self.fold_with(Panel::fold);
    }

    /// `l`: unfolds the node under the cursor.
    pub(crate) fn unfold(&mut self) {
        self.fold_with(Panel::unfold);
    }

    fn fold_with(&mut self, change: fn(&mut Panel) -> bool) {
        let Some(i) = self.focus.panel() else {
            return;
        };
        let mut seen = self.panels_seen();
        seen[i] = true;
        if change(&mut self.panels[i]) {
            self.refit_panels(seen);
            self.keep_folds();
        }
    }

    /// Moves the cursor of the panel that has the focus.
    pub(crate) fn panel_motion(
        &mut self,
        i: usize,
        action: Action,
        count: usize,
    ) {
        let rows = self.panel_rows()[i];
        let (Some(panel), Some(step)) =
            (self.panels.get_mut(i), Step::of(action, count, rows))
        else {
            return;
        };
        panel.go(step, rows);
    }

    /// The first screen row of each panel (its title line) and how
    /// many entry rows it shows, in a sidebar at `area`.
    pub(crate) fn panel_spots(&self, area: Rect) -> [(usize, usize); PANELS] {
        let rows =
            panel_rows_in(usize::from(area.height), self.entry_counts());
        let mut y = usize::from(area.y);
        rows.map(|n| {
            let spot = (y, n);
            y += 1 + n;
            spot
        })
    }

    /// Acts on entry `at` of panel `i`: the cursor goes there, then a
    /// switch changes, a project or context becomes the active one, or
    /// stops being it.
    pub(crate) fn click_entry(&mut self, i: usize, at: usize) {
        if let Some(panel) = self.panels.get_mut(i) {
            panel.cursor = at;
        }
        self.act_on_panel(i);
    }

    /// Folds or unfolds the node at entry `at` of panel `i`.
    pub(crate) fn click_fold(&mut self, i: usize, at: usize) {
        let seen = self.panels_seen();
        let Some(panel) = self.panels.get_mut(i) else {
            return;
        };
        if panel.flip_fold(at) {
            self.refit_panels(seen);
            self.keep_folds();
        }
    }

    /// Draws the sidebar in `area`.
    pub(crate) fn draw_sidebar(&self, buf: &mut Buffer, area: Rect) {
        let spots = self.panel_spots(area);
        for (i, (y, rows)) in spots.into_iter().enumerate() {
            let y = cells(y);
            self.draw_panel_top(buf, (area.x, y), area.width, i);
            for line in 0..rows {
                let at = (area.x, y + 1 + cells(line));
                self.draw_panel_row(buf, at, area.width, (i, line));
            }
        }
        let (y, rows) = spots[FILTERS];
        let end = cells(y + 1 + rows);
        let slot = Some(self.edge(&[Focus::Filters]));
        let w = usize::from(area.width).saturating_sub(BORDER);
        let rule = format!("└{}┘", RULE.repeat(w));
        let span = Span::new(rule, slot);
        put(buf, (area.x, end), area.width.into(), &[span], &self.styles);
    }

    fn draw_panel_top(
        &self,
        buf: &mut Buffer,
        at: (u16, u16),
        w: u16,
        i: usize,
    ) {
        let (left, right) = if i == PROJECTS {
            ("┌", "┐")
        } else {
            ("├", "┤")
        };
        let own = Focus::of_panel(i);
        let slot = Some(if i == PROJECTS {
            self.edge(&[own])
        } else {
            self.edge(&[own, Focus::of_panel(i - 1)])
        });
        let title = TITLES[i];
        let slots = (slot, Some(self.title_slot(own)));
        let spans = titled_spans((left, title, right), w.into(), slots);
        put(buf, at, w.into(), &spans, &self.styles);
    }

    fn draw_panel_row(
        &self,
        buf: &mut Buffer,
        at: (u16, u16),
        w: u16,
        which: (usize, usize),
    ) {
        let (x, y) = at;
        let (i, line) = which;
        let panel = &self.panels[i];
        let inner = usize::from(w).saturating_sub(BORDER);
        let side = Span::new("│", Some(self.edge(&[Focus::of_panel(i)])));
        let right = x + w.saturating_sub(1);
        put(buf, (x, y), 1, std::slice::from_ref(&side), &self.styles);
        put(
            buf,
            (right, y),
            1,
            std::slice::from_ref(&side),
            &self.styles,
        );
        let index = panel.top + line;
        let entry = panel.entries.get(index);
        let spans = entry.map_or_default(|e| row_spans(e, inner));
        let area = Rect::new(x + 1, y, cells(inner), 1);
        let focused = self.focus == Focus::of_panel(i);
        if focused && index == panel.cursor && index < panel.entries.len() {
            buf.set_style(area, self.styles.fill(Slot::Cursorline));
        }
        put(buf, (x + 1, y), inner, &spans, &self.styles);
    }
}
