//! The text line of the status line, whichever kind is open.

use crate::field::Field;
use crate::prompt::{Completion, Prompt};

/// The line being typed in the status line: a search, the `:` line or a
/// prompt on items.
#[derive(Debug)]
pub(crate) enum Line {
    /// The search; the list follows its text.
    Search(Field),
    /// The `:` line, whose commands write through themselves.
    Command {
        field: Field,
        comp: Option<Completion>,
    },
    /// A prompt that writes to items fixed when it opened.
    Items(Prompt),
}

impl Line {
    /// The `:` line with `text` typed.
    pub(crate) fn command(text: &str) -> Self {
        Self::Command {
            field: Field::new(text),
            comp: None,
        }
    }

    /// The text being typed.
    pub(crate) const fn field(&self) -> &Field {
        match self {
            Self::Search(field) | Self::Command { field, .. } => field,
            Self::Items(prompt) => &prompt.field,
        }
    }

    pub(crate) const fn field_mut(&mut self) -> &mut Field {
        match self {
            Self::Search(field) | Self::Command { field, .. } => field,
            Self::Items(prompt) => &mut prompt.field,
        }
    }

    /// Whether this is the `:` line.
    pub(crate) const fn is_command(&self) -> bool {
        matches!(self, Self::Command { .. })
    }

    /// The candidates offered now; a search has none.
    pub(crate) const fn completion(&self) -> Option<&Completion> {
        match self {
            Self::Search(_) => None,
            Self::Command { comp, .. } => comp.as_ref(),
            Self::Items(prompt) => prompt.comp.as_ref(),
        }
    }

    const fn slot(&mut self) -> Option<&mut Option<Completion>> {
        match self {
            Self::Search(_) => None,
            Self::Command { comp, .. } => Some(comp),
            Self::Items(prompt) => Some(&mut prompt.comp),
        }
    }

    /// Whether candidates may be started: the line takes them and has
    /// none yet.
    pub(crate) fn can_complete(&self) -> bool {
        match self {
            Self::Search(_) => false,
            Self::Command { comp, .. } => comp.is_none(),
            Self::Items(prompt) => prompt.comp.is_none(),
        }
    }

    pub(crate) fn set_completion(&mut self, comp: Option<Completion>) {
        if let Some(slot) = self.slot() {
            *slot = comp;
        }
    }

    /// Ends the list of candidates; the text stays as it is.
    pub(crate) fn drop_completion(&mut self) {
        self.set_completion(None);
    }

    /// Moves `by` places round the candidates, which puts that word in
    /// the text.
    pub(crate) fn step_completion(&mut self, by: isize) {
        let (field, comp) = match self {
            Self::Search(_) => return,
            Self::Command { field, comp } => (field, comp),
            Self::Items(prompt) => (&mut prompt.field, &mut prompt.comp),
        };
        if let Some(comp) = comp {
            comp.step(field, by);
        }
    }
}
