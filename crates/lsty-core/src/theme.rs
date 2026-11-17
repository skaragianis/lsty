//! Colour themes: named slots, the built-in palettes, user theme files,
//! and what a terminal can show of a colour.
//!
//! A theme is plain data: a [`Style`] for each [`Slot`]. Nothing here
//! knows a terminal library; the CLI writes the escape sequences and the
//! TUI maps a style onto its own types. A problem in a theme file is
//! never an error: it becomes a [`Warning`](crate::config::Warning) and
//! the slot keeps its fallback.

mod builtin;
mod load;
mod part;
#[cfg(test)]
mod part_tests;
mod profile;
mod slot;
mod style;
mod value_text;

#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod slot_tests;

pub use load::{COLOUR_KEYS, DEFAULT_NAME, FALLBACK, FLAG_KEYS, load, names};
pub use part::{Parts, Today, WEEK, claim_slot, due_slot, state_slot};
pub use profile::Profile;
pub use slot::Slot;
pub use style::{Colour, Style};

/// A named set of styles, one for each slot, and the few that differ on
/// the highlighted row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    styles: [Style; Slot::ALL.len()],
    cursor: [Option<Style>; Slot::ALL.len()],
    from_file: bool,
}

impl Theme {
    /// Whether a file in the themes folder sets it.
    pub fn from_file(&self) -> bool {
        self.from_file
    }

    /// How `slot` looks.
    pub fn style(&self, slot: Slot) -> Style {
        self.styles[slot.index()]
    }

    /// How `slot` looks on the highlighted row: the style the theme gives
    /// it there, else the one it has elsewhere.
    pub fn cursor_style(&self, slot: Slot) -> Style {
        self.cursor[slot.index()].unwrap_or_else(|| self.style(slot))
    }
}
