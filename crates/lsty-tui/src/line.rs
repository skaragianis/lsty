//! The text line of the status line, whichever kind is open.

use crate::{
    field::Field,
    prompt::{Completion, Prompt},
};

/// The line being typed in the status line: a search, the `:` line or a
/// prompt on tasks.
#[derive(Debug)]
pub(crate) struct Line {
    /// The text being typed.
    pub(crate) field: Field,
    /// The candidates offered now; a search never has any.
    pub(crate) comp: Option<Completion>,
    pub(crate) kind: LineKind,
}

/// What the line is for.
#[derive(Debug)]
pub(crate) enum LineKind {
    /// The search; the list follows its text, or the last text that
    /// parsed, which it holds.
    Search(String),
    /// The `:` line, whose commands write through themselves.
    Command,
    /// A prompt that writes to tasks fixed when it opened.
    Tasks(Prompt),
}

impl Line {
    /// A line of `kind` with `text` typed.
    pub(crate) fn new(kind: LineKind, text: &str) -> Self {
        Self {
            field: Field::new(text),
            comp: None,
            kind,
        }
    }
}
