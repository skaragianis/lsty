//! What the screen shows at a cell: a row of the list or a word in it,
//! an entry of a panel, a chip, a part of the detail panel.

use lsty_core::theme::Slot;
use lsty_core::todotxt::{Kind as Word, Line};
use ratatui::layout::Rect;

use crate::detail::{PADDING as DETAIL_PADDING, detail_rows, detail_size};
use crate::draw::{MIN_HEIGHT, MIN_WIDTH};
use crate::layout::BORDER;
use crate::model::Model;
use crate::sidebar::{CONTEXTS, PROJECTS};
use crate::status::PROGRESS_MARK;
use crate::text::{Span, cut, visible};

/// What is drawn at a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Nothing that does anything.
    #[default]
    None,
    /// A row of the list.
    Row,
    /// A project or a context, in a row or in the detail panel.
    Tag,
    /// A `file:` link.
    Link,
    /// An entry of a panel.
    Entry,
    /// The fold mark of a node.
    Fold,
    /// A show switch.
    Toggle,
    /// A chip of the top border.
    Chip,
    /// The `in progress` part of the status line.
    Progress,
    /// A place in the field of INSERT mode.
    Field,
    /// A popup.
    Popup,
}

/// The part of the screen a cell is in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pane {
    /// None of them.
    #[default]
    None,
    /// The list.
    List,
    /// The sidebar.
    Sidebar,
    /// The detail panel.
    Detail,
    /// A popup.
    Popup,
}

/// What is drawn at a cell. `at` is a row, an entry, a chip or a place
/// in the field; `panel` and `name` tell which project, context or
/// panel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) kind: Kind,
    pub(crate) pane: Pane,
    pub(crate) panel: usize,
    pub(crate) at: usize,
    /// Which `file:` line of the detail panel, counted from its first.
    pub(crate) link: usize,
    pub(crate) name: Vec<u8>,
}

impl Target {
    fn in_pane(pane: Pane) -> Self {
        Self {
            pane,
            ..Self::default()
        }
    }

    /// Whether the target is a row of the list or a part of one.
    pub(crate) fn on_row(&self) -> bool {
        self.pane == Pane::List
            && matches!(self.kind, Kind::Row | Kind::Tag | Kind::Link)
    }
}

/// A project, a context or a link inside a row or the detail panel.
struct Token {
    kind: Kind,
    panel: usize,
    name: Vec<u8>,
}

impl Token {
    fn on(self, at: &Target) -> Target {
        Target {
            kind: self.kind,
            pane: at.pane,
            panel: self.panel,
            at: at.at,
            link: at.link,
            name: self.name,
        }
    }
}

fn has(r: Rect, x: usize, y: usize) -> bool {
    let (rx, ry) = (usize::from(r.x), usize::from(r.y));
    x >= rx
        && x < rx + usize::from(r.width)
        && y >= ry
        && y < ry + usize::from(r.height)
}

impl Model {
    /// What the screen shows at column `x` of row `y`.
    pub(crate) fn hit(&self, x: usize, y: usize) -> Target {
        if self.width < MIN_WIDTH || self.height < MIN_HEIGHT {
            return Target::default();
        }
        let panes = self.panes(self.dims());
        let popup = self.output_rect().or_else(|| self.popup_rect());
        if popup.is_some_and(|r| has(r, x, y)) {
            return Target {
                kind: Kind::Popup,
                pane: Pane::Popup,
                ..Target::default()
            };
        }
        if y + 1 == usize::from(self.height) {
            return self.status_hit(x);
        }
        if let Some(side) = panes.sidebar.filter(|s| has(s.rect, x, y)) {
            return self.sidebar_hit(side.rect, x, y);
        }
        if has(panes.list, x, y) {
            return self.list_hit(panes.list, x, y);
        }
        match panes.detail.filter(|d| has(*d, x, y)) {
            Some(area) => self.detail_hit(area, x, y),
            None => Target::default(),
        }
    }

    fn status_hit(&self, x: usize) -> Target {
        let line = cut(self.status_base(), usize::from(self.width));
        let mut end = 0;
        let found = line.iter().find(|span| {
            end += span.cells();
            x < end
        });
        let progress = found.is_some_and(|span| {
            span.slot == Some(Slot::Doing)
                && span.text.starts_with(PROGRESS_MARK)
        });
        let kind = if progress { Kind::Progress } else { Kind::None };
        Target {
            kind,
            ..Target::default()
        }
    }

    fn sidebar_hit(&self, side: Rect, x: usize, y: usize) -> Target {
        let spots = self.panel_spots(side);
        let last = spots.len() - 1;
        for (i, (top, rows)) in spots.into_iter().enumerate() {
            let height = 1 + rows + usize::from(i == last);
            if y >= top && y < top + height {
                return self.entry_hit((i, top, rows), side, (x, y));
            }
        }
        Target::default()
    }

    /// The entry of panel `i`, whose title line is at `top` and which
    /// shows `rows` entries, at the cell.
    fn entry_hit(
        &self,
        (i, top, rows): (usize, usize, usize),
        side: Rect,
        (x, y): (usize, usize),
    ) -> Target {
        let none = Target {
            pane: Pane::Sidebar,
            panel: i,
            ..Target::default()
        };
        let line = y.checked_sub(top + 1);
        let col = x.checked_sub(usize::from(side.x) + 1);
        let inner = usize::from(side.width).saturating_sub(BORDER);
        let (Some(line), Some(col)) = (line, col) else {
            return none;
        };
        let panel = &self.panels[i];
        let at = panel.top + line;
        let Some(entry) = panel.entries.get(at).filter(|_| line < rows) else {
            return none;
        };
        if entry.is_rule() {
            return Target { at, ..none };
        }
        if col >= inner {
            return none;
        }
        let kind = if entry.toggles().is_some() {
            Kind::Toggle
        } else if entry.chevron_col() == Some(col) {
            Kind::Fold
        } else {
            Kind::Entry
        };
        Target { kind, at, ..none }
    }

    fn list_hit(&self, r: Rect, x: usize, y: usize) -> Target {
        let none = Target::in_pane(Pane::List);
        if self.edit.is_some() {
            return self.field_hit(r, x, y);
        }
        if y == usize::from(r.y) {
            return self.chip_hit(r, x);
        }
        let line = y.checked_sub(usize::from(r.y) + 1);
        let col = x.checked_sub(usize::from(r.x) + 1);
        let inner = usize::from(r.width).saturating_sub(BORDER);
        let (Some(line), Some(col)) = (line, col) else {
            return none;
        };
        let i = self.top + line;
        if line >= self.pane_rows() || col >= inner || i >= self.rows.len() {
            return none;
        }
        let row = Target {
            kind: Kind::Row,
            at: i,
            ..none
        };
        let Some(item) = self.snap.item(self.rows[i].id) else {
            return row;
        };
        let layout = self.list_layout(usize::from(r.width));
        let spans = self.row_spans(&self.rows[i], i, layout);
        match token_at(self.snap.line(item), &spans, col) {
            Some(token) => token.on(&row),
            None => row,
        }
    }

    fn chip_hit(&self, r: Rect, x: usize) -> Target {
        let none = Target::in_pane(Pane::List);
        let cells = self.chip_cells(usize::from(r.width));
        let found = cells.into_iter().position(|(from, w)| {
            let from = usize::from(r.x) + from;
            x >= from && x < from + w
        });
        match found {
            Some(at) => Target {
                kind: Kind::Chip,
                at,
                ..none
            },
            None => none,
        }
    }

    /// The place in the field at the cell, nearest to one where the
    /// text cursor can rest.
    fn field_hit(&self, r: Rect, x: usize, y: usize) -> Target {
        let none = Target::in_pane(Pane::List);
        let Some(draft) = &self.edit else {
            return none;
        };
        let (left, w) = self.field_box();
        let rows = draft.field.layout(w).rows;
        let first = usize::from(r.y) + 1 + self.field_at();
        let row = (y + draft.window + self.top).checked_sub(first);
        let col = x.checked_sub(usize::from(r.x) + 1 + left);
        let inside = x + 1 < usize::from(r.x) + usize::from(r.width);
        let (Some(row), Some(col)) = (row, col) else {
            return none;
        };
        if row >= rows || !inside {
            return none;
        }
        Target {
            kind: Kind::Field,
            at: draft.field.place_at(w, row, col),
            ..none
        }
    }

    fn detail_hit(&self, r: Rect, x: usize, y: usize) -> Target {
        let none = Target::in_pane(Pane::Detail);
        let (cells, rows) = detail_size(r);
        let line = y.checked_sub(usize::from(r.y) + 1);
        let col = x.checked_sub(usize::from(r.x) + 1 + DETAIL_PADDING);
        let (Some(line), Some(col)) = (line, col) else {
            return none;
        };
        if line >= rows || col >= cells {
            return none;
        }
        let doc = self.detail_doc(cells, rows);
        let shown = detail_rows(doc.lines.len(), rows, self.detail_top());
        let Some(&Some(n)) = shown.get(line) else {
            return none;
        };
        if let Some(files) = doc.files.filter(|&files| n > files) {
            return Target {
                kind: Kind::Link,
                link: n - files - 1,
                ..none
            };
        }
        let Some(item) = self.rows.get(self.cursor) else {
            return none;
        };
        let (Some(item), true) = (self.snap.item(item.id), n < doc.desc)
        else {
            return none;
        };
        let spans = doc.lines.get(n).map_or(&[][..], Vec::as_slice);
        match token_at(self.snap.line(item), spans, col) {
            Some(token) => token.on(&none),
            None => none,
        }
    }
}

/// The token of `line` that the span at column `col` shows whole.
fn token_at(line: &Line, spans: &[Span<'_>], col: usize) -> Option<Token> {
    let mut at = 0;
    for span in spans {
        let cells = span.cells();
        if col >= at + cells {
            at += cells;
            continue;
        }
        let text = span.text.as_ref();
        let lead = text.len() - text.trim_start_matches(' ').len();
        if col - at < lead {
            return None;
        }
        return token_of(line, text.trim());
    }
    None
}

fn token_of(line: &Line, text: &str) -> Option<Token> {
    for tok in line.tokens() {
        if visible(String::from_utf8_lossy(tok.text)) != text {
            continue;
        }
        let name = tok.value.to_vec();
        match tok.kind {
            Word::Project => {
                return Some(Token {
                    kind: Kind::Tag,
                    panel: PROJECTS,
                    name,
                });
            }
            Word::Context => {
                return Some(Token {
                    kind: Kind::Tag,
                    panel: CONTEXTS,
                    name,
                });
            }
            Word::Tag if tok.key == b"file" => {
                return Some(Token {
                    kind: Kind::Link,
                    panel: 0,
                    name,
                });
            }
            Word::Tag | Word::Word => {}
        }
    }
    None
}
