use std::fmt;

use super::parse;

/// What a token is: a plain word, a project, a context or a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Anything that is none of the others.
    Word,
    /// `+name`.
    Project,
    /// `@name`.
    Context,
    /// `key:value`.
    Tag,
}

impl Kind {
    /// The kind's name: `Word`, `Project`, `Context` or `Tag`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Word => "Word",
            Self::Project => "Project",
            Self::Context => "Context",
            Self::Tag => "Tag",
        }
    }
}

/// The line ending of a line, as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Eol {
    /// The last line of a file that does not end with a newline.
    None,
    /// `\n`.
    Lf,
    /// `\r\n`.
    CrLf,
}

impl Eol {
    /// The ending's bytes.
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::None => b"",
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
        }
    }
}

/// A priority: one letter from `A` to `Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(u8);

impl Priority {
    /// `A`, the highest priority.
    pub const A: Self = Self(b'A');
    /// `D`, the last letter of the built-in priority range.
    pub const D: Self = Self(b'D');

    /// The priority for `letter`, or `None` outside `A..=Z`.
    pub const fn new(letter: u8) -> Option<Self> {
        if letter.is_ascii_uppercase() {
            Some(Self(letter))
        } else {
            None
        }
    }

    /// The letter, `A..=Z`, as a byte.
    pub const fn letter(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Write::write_char(f, char::from(self.0))
    }
}

/// A calendar date as written, `YYYY-MM-DD`; always ten ASCII bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stamp(pub(super) [u8; 10]);

impl Stamp {
    pub(super) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

/// The position of a token in the line's raw bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) start: usize,
    pub(super) end: usize,
    /// Offset of the colon in the raw bytes; only set for tags.
    pub(super) colon: usize,
}

/// One word of a description, borrowed from its [`Line`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenView<'a> {
    /// What the word is.
    pub kind: Kind,
    /// The word as written.
    pub text: &'a [u8],
    /// A tag's key; empty for other kinds.
    pub key: &'a [u8],
    /// A tag's value, or a project's or context's name; empty for words.
    pub value: &'a [u8],
    /// Byte offset of the word in the line's raw bytes.
    pub start: usize,
}

/// The fields a change must keep: done mark, priority and both dates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamps {
    done: bool,
    priority: Option<Priority>,
    completed: Option<Stamp>,
    created: Option<Stamp>,
}

/// Why a change to a line was not made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// The line has no token at that index.
    NoToken,
    /// The new line would parse with another done mark, priority or date.
    /// It holds that new line.
    Refused(Line),
}

/// One parsed todo.txt line. It keeps its exact bytes; the fields are
/// views of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub(super) raw: Vec<u8>,
    pub(super) eol: Eol,
    pub(super) blank: bool,
    pub(super) done: bool,
    pub(super) priority: Option<Priority>,
    pub(super) completed: Option<Stamp>,
    pub(super) created: Option<Stamp>,
    pub(super) desc_start: usize,
    pub(super) tokens: Vec<Token>,
}

impl Line {
    /// Parses one line, given without its line ending. The ending is
    /// [`Eol::None`] until [`Line::set_eol`].
    pub fn parse(raw: impl Into<Vec<u8>>) -> Self {
        parse::parse_line(raw.into())
    }

    /// The line without its line ending, as stored.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// The line ending as stored.
    pub const fn eol(&self) -> Eol {
        self.eol
    }

    /// Replaces the line ending.
    pub const fn set_eol(&mut self, eol: Eol) {
        self.eol = eol;
    }

    /// The line with its ending, as it is written to a file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.raw.clone();
        out.extend_from_slice(self.eol.as_bytes());
        out
    }

    /// Whether `raw` is empty or holds only spaces and tabs.
    pub fn is_blank_text(raw: &[u8]) -> bool {
        raw.iter().all(|b| matches!(b, b' ' | b'\t'))
    }

    /// Whether the line is empty or holds only spaces and tabs.
    pub const fn is_blank(&self) -> bool {
        self.blank
    }

    /// Whether the line starts with the completion mark.
    pub const fn is_done(&self) -> bool {
        self.done
    }

    /// The priority, if the line has one. A done line may have one.
    pub const fn priority(&self) -> Option<Priority> {
        self.priority
    }

    /// The completion date as written, `YYYY-MM-DD`.
    pub fn completed(&self) -> Option<&str> {
        self.completed.as_ref().map(Stamp::as_str)
    }

    /// The creation date as written, `YYYY-MM-DD`.
    pub fn created(&self) -> Option<&str> {
        self.created.as_ref().map(Stamp::as_str)
    }

    /// Everything after the priority and dates, as stored.
    pub fn description(&self) -> &[u8] {
        self.raw.get(self.desc_start..).unwrap_or_default()
    }

    /// The number of tokens in the description.
    pub const fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// The `i`-th token of the description.
    pub fn token(&self, i: usize) -> Option<TokenView<'_>> {
        self.tokens.get(i).map(|t| self.view(t))
    }

    /// The tokens of the description, in order.
    pub fn tokens(&self) -> impl ExactSizeIterator<Item = TokenView<'_>> {
        self.tokens.iter().map(|t| self.view(t))
    }

    fn view(&self, t: &Token) -> TokenView<'_> {
        let slice = |from: usize, to: usize| -> &[u8] {
            self.raw.get(from..to).unwrap_or_default()
        };
        let (key, value) = match t.kind {
            Kind::Word => (slice(0, 0), slice(0, 0)),
            Kind::Project | Kind::Context => {
                (slice(0, 0), slice(t.start + 1, t.end))
            }
            Kind::Tag => (slice(t.start, t.colon), slice(t.colon + 1, t.end)),
        };
        TokenView {
            kind: t.kind,
            text: slice(t.start, t.end),
            key,
            value,
            start: t.start,
        }
    }

    /// The description's plain words, without projects, contexts or tags,
    /// joined by single spaces.
    pub fn text(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for tok in self.tokens().filter(|t| t.kind == Kind::Word) {
            if !out.is_empty() {
                out.push(b' ');
            }
            out.extend_from_slice(tok.text);
        }
        out
    }

    fn stamps(&self) -> Stamps {
        Stamps {
            done: self.done,
            priority: self.priority,
            completed: self.completed,
            created: self.created,
        }
    }

    /// Parses `raw` as this line's replacement, with this line's ending.
    fn rebuilt(&self, raw: Vec<u8>) -> Self {
        let mut line = Self::parse(raw);
        line.eol = self.eol;
        line
    }

    fn checked(&self, line: Self) -> Result<Self, EditError> {
        if line.stamps() == self.stamps() {
            Ok(line)
        } else {
            Err(EditError::Refused(line))
        }
    }

    /// Drops the `i`-th token and one adjoining space (the one before it,
    /// or the one after it for the first word), keeping other spacing.
    ///
    /// # Errors
    ///
    /// [`EditError::Refused`] when a word that follows the token would
    /// become a done mark, priority or date.
    pub fn remove_token(&self, i: usize) -> Result<Self, EditError> {
        let tok = self.tokens.get(i).ok_or(EditError::NoToken)?;
        let (mut start, mut end) = (tok.start, tok.end);
        if i > 0 {
            start -= 1;
        } else if end < self.raw.len() {
            end += 1;
        }
        let mut raw = self.raw.get(..start).unwrap_or_default().to_vec();
        raw.extend_from_slice(self.raw.get(end..).unwrap_or_default());
        self.checked(self.rebuilt(raw))
    }

    /// Appends `token` after one space, or without the space when the
    /// description has no tokens. [`Line::remove_token`] of the new last
    /// token restores the line. An empty `token` changes nothing.
    ///
    /// # Errors
    ///
    /// [`EditError::Refused`] when `token` would become a done mark,
    /// priority or date.
    pub fn append_token(&self, token: &[u8]) -> Result<Self, EditError> {
        if token.is_empty() {
            return Ok(self.clone());
        }
        let mut raw = self.raw.clone();
        if !self.tokens.is_empty() {
            raw.push(b' ');
        }
        raw.extend_from_slice(token);
        self.checked(self.rebuilt(raw))
    }

    /// Sets the text of the `i`-th token to `text`; `None` when there is
    /// no such token.
    pub fn replace_token(&self, i: usize, text: &[u8]) -> Option<Self> {
        let tok = self.tokens.get(i)?;
        let mut raw = self.raw.get(..tok.start)?.to_vec();
        raw.extend_from_slice(text);
        raw.extend_from_slice(self.raw.get(tok.end..)?);
        Some(self.rebuilt(raw))
    }
}

impl AsRef<[u8]> for Line {
    /// The line without its line ending, as [`Line::raw`].
    fn as_ref(&self) -> &[u8] {
        self.raw()
    }
}
