//! The sidebar: the Projects, Contexts and Filters panels.

use lsty_core::{config::Hidden, keys::Action, theme::Slot, todotxt::Kind};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    focus::Focus,
    layout::{BORDER, cells},
    model::Model,
    motion::Step,
    panel::{Entry, Panel, empty_row, row_spans},
    text::{RULE, Span, put, titled},
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

/// Splits `h` lines between the panels, borders included: the Filters
/// panel gets what its entries need, at most half.
fn panel_heights(h: usize, filter_entries: usize) -> [usize; PANELS] {
    let low = (filter_entries + BORDER).min(h / 2);
    let rest = h - low;
    [rest - rest / 2, rest / 2, low]
}

/// The entry rows of each panel in a sidebar `h` lines high. The last
/// panel's bottom border is the sidebar's.
fn panel_rows_in(h: usize, filter_entries: usize) -> [usize; PANELS] {
    let heights = panel_heights(h, filter_entries);
    let mut rows = heights.map(|n| n.saturating_sub(1));
    rows[FILTERS] = heights[FILTERS].saturating_sub(2);
    rows
}

impl Model {
    pub(crate) fn new_panels() -> [Panel; PANELS] {
        [Panel::tree("+"), Panel::tree("@"), Panel::list()]
    }

    /// Lists the names again after the items changed.
    /// A panel scrolls to its cursor only if that was in view before.
    pub(crate) fn rebuild_panels(&mut self) {
        let rows = self.panel_rows();
        let before: Vec<_> = (self.panels.iter_mut().zip(rows))
            .map(|(p, rows)| (p.seen(rows), std::mem::take(&mut p.entries)))
            .collect();
        let projects = self.snap.rolled_counts(Kind::Project);
        let contexts = self.snap.rolled_counts(Kind::Context);
        let open_only = !self.shows.on(Hidden::Done);
        self.panels[PROJECTS].set_nodes(projects, open_only);
        self.panels[CONTEXTS].set_nodes(contexts, open_only);
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
        let active = self.panels[FILTERS].active.first();
        entries.extend(self.catalog.iter().map(|e| {
            Entry::filter(
                &e.name,
                active.is_some_and(|a| a == e.name.as_bytes()),
            )
        }));
        entries
    }

    /// The entry rows of each panel at the current height.
    pub(crate) fn panel_rows(&self) -> [usize; PANELS] {
        let h = usize::from(self.height).saturating_sub(1);
        panel_rows_in(h, self.panels[FILTERS].entries.len())
    }

    pub(crate) fn scroll_panels(&mut self) {
        let rows = self.panel_rows();
        for (panel, rows) in self.panels.iter_mut().zip(rows) {
            panel.scroll(rows);
        }
    }

    /// The tree that has the focus, when it has entries.
    fn focused_tree(&mut self) -> Option<(&mut Panel, usize)> {
        let i = self.focus.panel().filter(|&i| i != FILTERS)?;
        let panel = self.panels.get_mut(i)?;
        (!panel.entries.is_empty()).then_some((panel, i))
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
        let Some((panel, i)) = self.focused_tree() else {
            return;
        };
        if change(panel) {
            let rows = self.panel_rows()[i];
            self.panels[i].scroll(rows);
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

    /// The slot of a border shared by `owners`: active when the focus is
    /// on any of them.
    pub(crate) fn edge(&self, owners: &[Focus]) -> Slot {
        if owners.contains(&self.focus) {
            Slot::PanelBorderActive
        } else {
            Slot::PanelBorder
        }
    }

    /// The first screen row of each panel (its title line) and how
    /// many entry rows it shows, in a sidebar at `area`.
    pub(crate) fn panel_spots(&self, area: Rect) -> [(usize, usize); PANELS] {
        let rows = panel_rows_in(
            usize::from(area.height),
            self.panels[FILTERS].entries.len(),
        );
        let mut y = usize::from(area.y);
        rows.map(|n| {
            let spot = (y, n);
            y += 1 + n;
            spot
        })
    }

    /// Acts on entry `at` of panel `i`: the cursor goes there, then a
    /// switch changes, a name becomes the active one, or stops being it.
    pub(crate) fn click_entry(&mut self, i: usize, at: usize) {
        if let Some(panel) = self.panels.get_mut(i) {
            panel.cursor = at;
        }
        self.act_on_panel(i);
    }

    /// Folds or unfolds the node at entry `at` of panel `i`.
    pub(crate) fn click_fold(&mut self, i: usize, at: usize) {
        let Some(panel) = self.panels.get_mut(i) else {
            return;
        };
        if panel.flip_fold(at) {
            let rows = self.panel_rows()[i];
            self.panels[i].scroll(rows);
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
        let text = titled(left, title, right, w.into());
        put(buf, at, w.into(), &[Span::new(text, slot)], &self.styles);
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
        let spans = match panel.entries.get(index) {
            Some(entry) => row_spans(entry, inner),
            None => empty_row(inner),
        };
        let area = Rect::new(x + 1, y, cells(inner), 1);
        let focused = self.focus == Focus::of_panel(i);
        if focused && index == panel.cursor && index < panel.entries.len() {
            buf.set_style(area, self.styles.fill(Slot::Cursorline));
        }
        put(buf, (x + 1, y), inner, &spans, &self.styles);
    }
}
