//! A line of text with a cursor, for the prompt.

use lsty_core::keys::{Action, Key, Mods, Named, Stroke};
use unicode_width::UnicodeWidthStr;

use crate::text::visible;

/// The text of a prompt and where the cursor is in it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Field {
    text: Vec<char>,
    cursor: usize,
}

impl Field {
    /// A field that holds `text`, with the cursor after it.
    pub(crate) fn new(text: &str) -> Self {
        let text: Vec<char> = text.chars().collect();
        let cursor = text.len();
        Self { text, cursor }
    }

    /// The text.
    pub(crate) fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// The width in cells of the text before the cursor, as drawn.
    pub(crate) fn cells(&self) -> usize {
        let before: String = self.text.iter().take(self.cursor).collect();
        visible(before.into()).width()
    }

    /// Applies a key that no binding took: a printable character,
    /// `<BS>` or `<Del>`. Other keys change nothing.
    pub(crate) fn typed(&mut self, stroke: Stroke) {
        let plain = stroke.mods.without(Mods::SHIFT) == Mods::NONE;
        match stroke.key {
            Key::Char(c) if plain && !c.is_control() => self.insert(c),
            Key::Named(Named::Bs) => self.backspace(),
            Key::Named(Named::Del) => self.delete(),
            Key::Char(_) | Key::Named(_) => {}
        }
    }

    /// Does an editing action of the command table. Others change
    /// nothing.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the editing actions reach the field"
    )]
    pub(crate) fn act(&mut self, action: Action) {
        match action {
            Action::CursorLeft => self.cursor = self.cursor.saturating_sub(1),
            Action::CursorRight => {
                self.cursor = (self.cursor + 1).min(self.text.len());
            }
            Action::LineStart => self.cursor = 0,
            Action::LineEnd => self.cursor = self.text.len(),
            Action::DeleteWordBack => self.delete_word_back(),
            Action::DeleteToStart => self.cut(0, self.cursor),
            _ => {}
        }
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if let Some(from) = self.cursor.checked_sub(1) {
            self.cut(from, self.cursor);
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.cut(self.cursor, self.cursor + 1);
        }
    }

    fn delete_word_back(&mut self) {
        let is_space = |at: usize| {
            at.checked_sub(1)
                .and_then(|i| self.text.get(i))
                .is_some_and(|c| c.is_whitespace())
        };
        let mut start = self.cursor;
        while start > 0 && is_space(start) {
            start -= 1;
        }
        while start > 0 && !is_space(start) {
            start -= 1;
        }
        self.cut(start, self.cursor);
    }

    fn cut(&mut self, from: usize, to: usize) {
        self.text.drain(from..to);
        self.cursor = from;
    }
}
