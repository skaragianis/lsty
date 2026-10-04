//! The chip line: the active filters in the top border of the list.

use lsty_core::theme::Slot;

use crate::model::Model;
use crate::sidebar::{CONTEXTS, FILTERS, PROJECTS};
use crate::text::Span;

const GAP: &str = "  ";

impl Model {
    /// The texts of the active filters, in the order of the chip line:
    /// project, context, narrowing filter, search.
    fn chips(&self) -> Vec<String> {
        let mut out = Vec::new();
        for i in [PROJECTS, CONTEXTS, FILTERS] {
            let panel = &self.panels[i];
            if let Some(name) = &panel.active {
                let name = String::from_utf8_lossy(name);
                out.push(format!("{}{name}", panel.sigil()));
            }
        }
        let text = self.search_text();
        if !text.is_empty() {
            out.push(format!("/{text}"));
        }
        out
    }

    /// The chips, or the name of the file when no filter is active.
    /// `slot` styles the name and the gaps.
    pub(crate) fn title(&self, slot: Option<Slot>) -> Vec<Span<'static>> {
        let chips = self.chips();
        if chips.is_empty() {
            return vec![Span::new(self.base_name(), slot)];
        }
        let mut out = Vec::new();
        for (i, chip) in chips.into_iter().enumerate() {
            if i > 0 {
                out.push(Span::new(GAP, slot));
            }
            out.push(Span::new(chip, Some(Slot::Chip)));
        }
        out
    }
}
