//! The chip line: the active filters in the top border of the list.

use std::path::Path;

use lsty_core::{errs::lossy_path, theme::Slot};

use crate::{
    connective,
    layout::{BORDER, FRAME},
    model::Model,
    sidebar::{CONTEXTS, FILTERS, PROJECTS},
    text::{ELLIPSIS, Span, ending, fit, width_of},
};

const GAP: &str = "  ";
/// The number of the list, before its title, as the key that focuses it.
const NUMBER: &str = "[1]─";

/// One active filter in the top border of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chip {
    /// The panel whose active entry it shows; none for a term of the
    /// search.
    pub(crate) panel: Option<usize>,
    /// Where its first project or context is among the active ones of the
    /// panel, or where its term is in the search.
    pub(crate) at: usize,
    pub(crate) text: String,
}

/// The path of `todo` as the title shows it: `~` for `home`, which is
/// resolved first because `todo` is.
pub(crate) fn title_path(todo: &Path, home: Option<&Path>) -> String {
    let home = home.map(|h| h.canonicalize().unwrap_or_else(|_| h.into()));
    match home.and_then(|h| todo.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", lossy_path(rest)),
        None => lossy_path(todo),
    }
}

/// `path` in `room` cells: when it is too long, the folders from the
/// left give way to an ellipsis, then the start of the file name.
pub(crate) fn left_cut(path: &str, room: usize) -> String {
    if width_of(path) <= room {
        return path.to_owned();
    }
    let folders = path.match_indices('/').map(|(at, _)| at);
    let cut =
        |at: usize| format!("{ELLIPSIS}{}", path.get(at..).unwrap_or(""));
    if let Some(shown) = folders.map(cut).find(|c| width_of(c) <= room) {
        return shown;
    }
    ending(path.rsplit('/').next().unwrap_or(path), room)
}

impl Model {
    /// The active filters, in the order of the chip line: project,
    /// context, narrowing filter, the terms of the search.
    pub(crate) fn chips(&self) -> Vec<Chip> {
        let split = self.searched();
        let mut out = Vec::new();
        for i in [PROJECTS, CONTEXTS, FILTERS] {
            let active = split.active(&self.panels, i);
            for (at, text) in connective::terms(self.panels[i].sigil, active) {
                out.push(Chip {
                    panel: Some(i),
                    at,
                    text,
                });
            }
        }
        for (at, (text, _)) in split.stays.iter().enumerate() {
            out.push(Chip {
                panel: None,
                at,
                text: format!("/{text}"),
            });
        }
        out
    }

    /// The first cell of each chip and its width, as the top border
    /// of a list pane `w` cells wide shows them; a chip that is cut
    /// has the cells that are left, with its ellipsis.
    pub(crate) fn chip_cells(&self, w: usize) -> Vec<(usize, usize)> {
        let mut at = BORDER;
        let room = w.saturating_sub(FRAME);
        fit(self.title(None, room), room)
            .iter()
            .filter_map(|s| {
                let from = at;
                at += s.cells();
                (s.slot == Some(Slot::Chip)).then_some((from, s.cells()))
            })
            .collect()
    }

    /// The number of the list, then the chips, or the path of the file
    /// when no filter is active, cut from the left to fit `room` cells.
    /// `slot` styles the number, the path and the gaps.
    pub(crate) fn title(
        &self,
        slot: Option<Slot>,
        room: usize,
    ) -> Vec<Span<'static>> {
        let mut out = vec![Span::new(NUMBER, slot)];
        let chips = self.chips();
        if chips.is_empty() {
            let left = room.saturating_sub(width_of(NUMBER));
            out.push(Span::new(left_cut(&self.path, left), slot));
            return out;
        }
        for (i, chip) in chips.into_iter().enumerate() {
            if i > 0 {
                out.push(Span::new(GAP, slot));
            }
            out.push(Span::new(chip.text, Some(Slot::Chip)));
        }
        out
    }
}
