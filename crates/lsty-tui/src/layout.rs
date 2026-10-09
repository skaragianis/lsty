//! Where the sidebar, the list and the detail panel go.

use ratatui::layout::Rect;

use crate::model::Model;

/// The least and most the widest entry counts for: the least is the widest
/// pane title.
const MIN_SIDEBAR: usize = 12;
const MAX_SIDEBAR: usize = 28;
/// The cell the sidebar takes beyond its widest entry and its border.
const SIDEBAR_SPARE: usize = 1;
const MIN_DETAIL_SIDE: usize = 20;
const MAX_DETAIL_SIDE: usize = 50;
const MIN_DETAIL_LOW: usize = 6;
const MIN_LIST_LINES: usize = 3;
const MIN_LIST_WIDTH: usize = 20;
const THIRDS: usize = 3;
/// The cells that the border of a pane takes across.
pub(crate) const BORDER: usize = 2;

/// A size in cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) w: usize,
    pub(crate) h: usize,
}

/// The sizes of the parts of the screen above the status line.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Geometry {
    sidebar: usize,
    overlay: bool,
    pub(crate) list: Size,
    detail: Size,
    below: bool,
}

/// The sidebar's place, and whether it covers the list.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SidebarPane {
    pub(crate) rect: Rect,
    pub(crate) overlay: bool,
}

/// Where each part is drawn.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Panes {
    pub(crate) list: Rect,
    pub(crate) detail: Option<Rect>,
    pub(crate) sidebar: Option<SidebarPane>,
}

pub(crate) fn cells(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn rect(x: usize, y: usize, size: Size) -> Rect {
    Rect::new(cells(x), cells(y), cells(size.w), cells(size.h))
}

impl Model {
    /// The size of the terminal as the model was told.
    pub(crate) fn dims(&self) -> Size {
        Size {
            w: usize::from(self.width),
            h: usize::from(self.height),
        }
    }

    fn roomy_at(&self, width: usize) -> bool {
        let min = self.deps.config.sidebar_min_width;
        u64::try_from(width).is_ok_and(|w| w >= min)
    }

    /// Whether the terminal is wide enough for a sidebar beside the list.
    pub(crate) fn roomy(&self) -> bool {
        self.roomy_at(usize::from(self.width))
    }

    /// Whether the sidebar is shown beside the list.
    pub(crate) fn docked(&self) -> bool {
        self.sidebar_on && self.roomy()
    }

    /// Whether the sidebar is shown, beside the list or over it.
    pub(crate) fn sidebar_shown(&self) -> bool {
        self.docked() || self.overlay
    }

    /// As wide as the widest entry, within limits, plus the border and one
    /// spare cell.
    fn sidebar_width(&self) -> usize {
        let most = self.panels.iter().map(|p| p.natural).max().unwrap_or(0);
        most.clamp(MIN_SIDEBAR, MAX_SIDEBAR) + BORDER + SIDEBAR_SPARE
    }

    /// The sizes of the parts of a screen of `dims`.
    pub(crate) fn geometry(&self, dims: Size) -> Geometry {
        let room = dims.h.saturating_sub(1);
        let mut g = Geometry {
            list: Size { w: dims.w, h: room },
            ..Geometry::default()
        };
        let docked = self.sidebar_on && self.roomy_at(dims.w);
        if docked || self.overlay {
            g.sidebar = self.sidebar_width().min(dims.w);
            g.overlay = !docked;
            if !g.overlay {
                g.list.w = g.list.w.saturating_sub(g.sidebar);
            }
        }
        if self.detail_on {
            self.place_detail(&mut g, dims);
        }
        g
    }

    fn place_detail(&self, g: &mut Geometry, dims: Size) {
        let w = (dims.w / THIRDS).clamp(MIN_DETAIL_SIDE, MAX_DETAIL_SIDE);
        let min = self.deps.config.detail_min_width;
        let wide = u64::try_from(dims.w).is_ok_and(|n| n >= min);
        if wide && g.list.w.saturating_sub(w) >= MIN_LIST_WIDTH {
            g.detail = Size { w, h: g.list.h };
            g.list.w -= w;
            return;
        }
        let most = g.list.h.saturating_sub(MIN_LIST_LINES);
        let h = (dims.h / THIRDS).max(MIN_DETAIL_LOW).min(most);
        if h < MIN_LIST_LINES {
            return;
        }
        g.detail = Size { w: g.list.w, h };
        g.list.h -= h;
        g.below = true;
    }

    /// Where the parts of a screen of `dims` are drawn.
    pub(crate) fn panes(&self, dims: Size) -> Panes {
        let g = self.geometry(dims);
        let mut list_x = 0;
        let mut sidebar = None;
        if g.sidebar > 0 {
            let size = Size {
                w: g.sidebar,
                h: dims.h.saturating_sub(1),
            };
            sidebar = Some(SidebarPane {
                rect: rect(0, 0, size),
                overlay: g.overlay,
            });
            if !g.overlay {
                list_x = g.sidebar;
            }
        }
        let detail = if g.detail.w == 0 {
            None
        } else if g.below {
            Some(rect(list_x, g.list.h, g.detail))
        } else {
            Some(rect(list_x + g.list.w, 0, g.detail))
        };
        Panes {
            list: rect(list_x, 0, g.list),
            detail,
            sidebar,
        }
    }

    /// The number of item rows the list pane shows.
    pub(crate) fn pane_rows(&self) -> usize {
        self.geometry(self.dims()).list.h.saturating_sub(BORDER)
    }
}
