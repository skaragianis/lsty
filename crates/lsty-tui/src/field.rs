//! A line of text with a cursor, for the prompt and for INSERT mode.
//!
//! Characters in a read-only range cannot be changed, and the cursor
//! never rests inside one or before the floor. A field without ranges is
//! a plain line of text.

use lsty_core::keys::{Action, Key, Mods, Named, Stroke};
use unicode_width::UnicodeWidthStr;

use crate::text::visible;

/// The characters from `0` up to, not including, `1`.
pub(crate) type Range = (usize, usize);

/// The text of a field and where the cursor is in it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Field {
    pub(crate) text: Vec<char>,
    pub(crate) cursor: usize,
    /// The ranges that cannot be changed.
    ro: Vec<Range>,
    /// The cursor never goes before this position.
    floor: usize,
    /// The column that vertical moves aim at, once they began.
    pub(crate) col: Option<usize>,
}

impl Field {
    /// A field that holds `text`, with the cursor after it.
    pub(crate) fn new(text: &str) -> Self {
        let text: Vec<char> = text.chars().collect();
        let cursor = text.len();
        Self {
            text,
            cursor,
            ..Self::default()
        }
    }

    /// A field that holds `text` with the read-only `ro` and the
    /// cursor at `cursor`.
    pub(crate) fn locked(
        text: &str,
        (ro, floor): (Vec<Range>, usize),
        cursor: usize,
    ) -> Self {
        Self {
            cursor,
            ro,
            floor,
            ..Self::new(text)
        }
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

    /// Whether the character at `at` is a space.
    fn space(&self, at: usize) -> bool {
        self.text.get(at).is_some_and(|c| c.is_whitespace())
    }

    /// Whether the character before `at` is a space.
    fn space_before(&self, at: usize) -> bool {
        at.checked_sub(1).is_some_and(|i| self.space(i))
    }

    /// Whether the character at `at` can be neither changed nor split.
    pub(crate) fn is_locked(&self, at: usize) -> bool {
        self.ro.iter().any(|r| r.0 <= at && at < r.1)
    }

    /// The read-only range that `at` lies strictly inside.
    fn within(&self, at: usize) -> Option<Range> {
        self.ro.iter().copied().find(|r| r.0 < at && at < r.1)
    }

    /// Whether the cursor may rest at `at`.
    pub(crate) fn valid(&self, at: usize) -> bool {
        at >= self.floor && self.within(at).is_none() && !self.shared(at)
    }

    /// Where the editable text around the cursor begins.
    fn edit_start(&self) -> usize {
        self.ro
            .iter()
            .filter(|r| r.1 <= self.cursor)
            .map(|r| r.1)
            .max()
            .unwrap_or(0)
    }

    /// Moves the ranges that start at `from` or later by `by`.
    fn shift(&mut self, from: usize, by: isize) {
        for r in &mut self.ro {
            if r.0 >= from {
                r.0 = r.0.saturating_add_signed(by);
                r.1 = r.1.saturating_add_signed(by);
            }
        }
    }

    /// Whether `at` is where one stamp ends and the next begins.
    fn shared(&self, at: usize) -> bool {
        at > self.floor
            && self.stamp_from(at).is_some()
            && self.stamp_to(at).is_some()
    }

    /// The range that starts at `at` and ends in a space.
    fn stamp_from(&self, at: usize) -> Option<Range> {
        self.ro
            .iter()
            .copied()
            .find(|r| r.0 == at && self.space_before(r.1))
    }

    /// The range that ends at `at`, in a space.
    fn stamp_to(&self, at: usize) -> Option<Range> {
        self.ro
            .iter()
            .copied()
            .find(|r| r.1 == at && self.space_before(r.1))
    }

    fn put(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.shift(self.cursor, 1);
        self.cursor += 1;
    }

    /// Whether a read-only token starts at `at` and ends in text.
    fn joins(&self, at: usize) -> bool {
        self.ro.iter().any(|r| r.0 == at && !self.space_before(r.1))
    }

    /// Puts `c` at the cursor, with a space between it and a read-only
    /// token that it would otherwise join.
    pub(crate) fn insert(&mut self, c: char) {
        if c.is_whitespace() {
            self.put(c);
            return;
        }
        let after_token = self.cursor > 0
            && self.is_locked(self.cursor - 1)
            && !self.space_before(self.cursor);
        if after_token {
            self.put(' ');
        }
        let glued = self.cursor < self.text.len() && self.joins(self.cursor);
        self.put(c);
        if glued {
            self.put(' ');
            self.cursor -= 1;
        }
    }

    /// Whether the text next to `at` has met the edge of a read-only
    /// token without a space between them.
    fn glued_at(&self, at: usize) -> bool {
        if at > 0 && !self.space_before(at) && self.joins(at) {
            return true;
        }
        at < self.text.len()
            && !self.space(at)
            && self.ro.iter().any(|r| r.1 == at && !self.space_before(at))
    }

    /// Deletes the characters from `from` up to `to`, keeping one space
    /// where the removal would join text to a read-only token.
    pub(crate) fn cut(&mut self, from: usize, to: usize) {
        let to = to.min(self.text.len());
        let from = from.min(to);
        self.text.drain(from..to);
        let removed = isize::try_from(to - from).unwrap_or(isize::MAX);
        self.shift(to, -removed);
        self.cursor = from;
        if self.glued_at(from) {
            self.text.insert(from, ' ');
            self.shift(from, 1);
        }
    }

    fn backspace(&mut self) {
        let Some(from) = self.cursor.checked_sub(1) else {
            return;
        };
        if self.is_locked(from) {
            return;
        }
        let before = self.text.len();
        self.cut(from, self.cursor);
        if self.text.len() == before && self.joins(from + 1) {
            self.cursor = from + 1;
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.text.len() && !self.is_locked(self.cursor) {
            self.cut(self.cursor, self.cursor + 1);
        }
    }

    /// The cursor one place to the left, over a read-only token as over
    /// one character.
    fn left(&mut self) {
        let Some(mut to) = self.cursor.checked_sub(1) else {
            return;
        };
        if let Some(r) = self.within(to) {
            to = r.0;
        }
        while self.shared(to) {
            let Some(r) = self.stamp_to(to) else {
                break;
            };
            to = r.0;
        }
        if to >= self.floor {
            self.cursor = to;
        }
    }

    /// The cursor one place to the right, likewise.
    fn right(&mut self) {
        let mut to = self.cursor + 1;
        if to > self.text.len() {
            return;
        }
        if let Some(r) = self.within(to) {
            to = r.1;
        }
        while self.shared(to) {
            let Some(r) = self.stamp_from(to) else {
                break;
            };
            to = r.1;
        }
        self.cursor = to;
    }

    fn delete_word_back(&mut self) {
        let limit = self.edit_start();
        let mut start = self.cursor;
        while start > limit && self.space_before(start) {
            start -= 1;
        }
        while start > limit && !self.space_before(start) {
            start -= 1;
        }
        self.cut(start, self.cursor);
    }

    /// Applies a key that no binding took: a printable character,
    /// `<BS>` or `<Del>`. Other keys change nothing.
    pub(crate) fn typed(&mut self, stroke: Stroke) {
        self.col = None;
        let plain = stroke.mods.without(Mods::SHIFT) == Mods::NONE;
        match stroke.key {
            Key::Char(c) if plain && !c.is_control() => self.insert(c),
            Key::Named(Named::Bs) => self.backspace(),
            Key::Named(Named::Del) => self.delete(),
            Key::Char(_) | Key::Named(_) => {}
        }
    }

    /// Puts pasted text in, with every line break and tab as a space.
    pub(crate) fn paste(&mut self, text: &str) {
        self.col = None;
        let text = text.replace("\r\n", "\n");
        for c in text.chars() {
            if matches!(c, '\n' | '\r' | '\t') {
                self.insert(' ');
            } else if !c.is_control() {
                self.insert(c);
            }
        }
    }

    /// Does an editing action of the command table. Others change
    /// nothing.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the editing actions reach the field"
    )]
    pub(crate) fn act(&mut self, action: Action) {
        self.col = None;
        match action {
            Action::CursorLeft => self.left(),
            Action::CursorRight => self.right(),
            Action::LineStart => self.cursor = self.floor,
            Action::LineEnd => self.cursor = self.text.len(),
            Action::DeleteWordBack => self.delete_word_back(),
            Action::DeleteToStart => self.cut(self.edit_start(), self.cursor),
            _ => {}
        }
    }

    /// The characters of the word under the cursor, as a range of
    /// positions: from where the word starts to where it ends.
    pub(crate) fn word_span(&self) -> (usize, usize) {
        let blank =
            |at: usize| self.text.get(at).is_none_or(|c| c.is_whitespace());
        let mut from = self.cursor;
        while from > 0 && !blank(from - 1) {
            from -= 1;
        }
        let mut to = self.cursor;
        while !blank(to) {
            to += 1;
        }
        (from, to)
    }

    /// The word that ends at the cursor and where it starts; `None`
    /// when the cursor is inside a word or after a space.
    pub(crate) fn word_before(&self) -> Option<(usize, String)> {
        let at = self.cursor;
        let before = at.checked_sub(1).and_then(|i| self.text.get(i))?;
        if before.is_whitespace()
            || self.text.get(at).is_some_and(|c| !c.is_whitespace())
        {
            return None;
        }
        let from =
            self.text.get(..at)?.iter().rposition(|c| c.is_whitespace());
        let from = from.map_or(0, |i| i + 1);
        Some((from, self.slice(from, at)))
    }

    /// Puts `to` in place of every word that reads `from`, keeping the
    /// cursor on what it was on.
    pub(crate) fn swap_token(&mut self, from: &str, to: &str) {
        let from: Vec<char> = from.chars().collect();
        let mut cursor = self.cursor;
        let mut end = self.text.len();
        loop {
            let start = self.word_start(end);
            cursor = self.swap_word((start, end), (&from, to), cursor);
            if start == 0 {
                break;
            }
            end = start - 1;
        }
        self.cursor = cursor.min(self.text.len());
    }

    /// Puts `to` in place of the word from `start` to `end` when it
    /// reads `from`, and returns where `cursor` is then.
    fn swap_word(
        &mut self,
        (start, end): Range,
        (from, to): (&[char], &str),
        cursor: usize,
    ) -> usize {
        if self.text.get(start..end) != Some(from) {
            return cursor;
        }
        self.replace(start, end, to);
        if cursor >= end {
            return cursor + to.chars().count() - from.len();
        }
        cursor
    }

    /// Where the word that ends at `end` starts; words are separated by
    /// spaces and tabs.
    fn word_start(&self, end: usize) -> usize {
        let before = self.text.get(..end).unwrap_or_default();
        before
            .iter()
            .rposition(|c| matches!(c, ' ' | '\t'))
            .map_or(0, |i| i + 1)
    }

    /// The characters from position `from` up to `to`.
    pub(crate) fn slice(&self, from: usize, to: usize) -> String {
        self.text.get(from..to).unwrap_or_default().iter().collect()
    }

    /// Puts `word` in place of the characters from `from` up to `to`,
    /// with the cursor after it.
    pub(crate) fn replace(&mut self, from: usize, to: usize, word: &str) {
        let to = to.min(self.text.len());
        let from = from.min(to);
        let word: Vec<char> = word.chars().collect();
        let end = from + word.len();
        let by = isize::try_from(word.len()).unwrap_or(0)
            - isize::try_from(to - from).unwrap_or(0);
        self.text.splice(from..to, word);
        self.shift(to, by);
        self.cursor = end;
    }
}

#[cfg(test)]
impl Field {
    /// The read-only ranges.
    pub(crate) fn read_only(&self) -> &[Range] {
        &self.ro
    }
}
