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
mod profile;
mod slot;
mod style;
mod value_text;

#[cfg(test)]
mod load_tests;

pub use load::{DEFAULT_NAME, load, names};
pub use profile::Profile;
pub use slot::Slot;
pub use style::{Colour, Style};

/// A named set of styles, one for each slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    name: String,
    styles: [Style; Slot::ALL.len()],
}

impl Theme {
    /// The theme's name: its file name without `.toml`.
    #[cfg(test)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// How `slot` looks.
    pub fn style(&self, slot: Slot) -> Style {
        self.styles.get(slot.index()).copied().unwrap_or_default()
    }
}
