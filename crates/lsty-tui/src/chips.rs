//! The chip line: the active filters in the top border of the list.

use lsty_core::theme::Slot;

use crate::{
    layout::BORDER,
    model::Model,
    sidebar::{CONTEXTS, FILTERS, PROJECTS},
    text::{Span, fit},
};

const GAP: &str = "  ";
/// The number of the list, before its title, as the key that focuses it.
const NUMBER: &str = "[1]─";

/// One active filter in the top border of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chip {
    /// The panel whose active entry it shows; none for the search.
    pub(crate) panel: Option<usize>,
    pub(crate) text: String,
}

impl Model {
    /// The active filters, in the order of the chip line: project,
    /// context, narrowing filter, search.
    pub(crate) fn chips(&self) -> Vec<Chip> {
        let mut out = Vec::new();
        for i in [PROJECTS, CONTEXTS, FILTERS] {
            let panel = &self.panels[i];
            if let Some(name) = &panel.active {
                let name = String::from_utf8_lossy(name);
                out.push(Chip {
                    panel: Some(i),
                    text: format!("{}{name}", panel.sigil()),
                });
            }
        }
        let text = self.search_text();
        if !text.is_empty() {
            out.push(Chip {
                panel: None,
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
        fit(self.title(None), w.saturating_sub(2 * BORDER))
            .iter()
            .filter_map(|s| {
                let from = at;
                at += s.cells();
                (s.slot == Some(Slot::Chip)).then_some((from, s.cells()))
            })
            .collect()
    }

    /// The number of the list, then the chips, or the name of the file
    /// when no filter is active. `slot` styles the number, the name and
    /// the gaps.
    pub(crate) fn title(&self, slot: Option<Slot>) -> Vec<Span<'static>> {
        let mut out = vec![Span::new(NUMBER, slot)];
        let chips = self.chips();
        if chips.is_empty() {
            out.push(Span::new(self.base_name(), slot));
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
