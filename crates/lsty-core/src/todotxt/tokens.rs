//! Where the tokens of parsed lines are kept. The lines parsed together
//! (a whole file, or one line) share one array of tokens, so parsing a
//! file allocates for its tokens in one growing array and cloning a
//! line copies none.
//! Offsets are kept in 32 bits: a file over 4 GiB is refused when it is
//! read or written, so a line that is part of a file has none larger. A
//! single line built in memory past that stops at the largest offset.

use std::{fmt, sync::Arc};

use super::{line::Kind, share};

/// The position of a token in its line's raw bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) start: usize,
    pub(super) end: usize,
    /// Offset of the colon in the raw bytes; only set for tags. On a plain
    /// word it is 1 when backticks quote the word, else 0.
    pub(super) colon: usize,
}

/// A [`Token`] with its offsets in 32 bits; half its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Narrow {
    kind: Kind,
    start: u32,
    end: u32,
    colon: u32,
}

impl Narrow {
    /// A token that fills a slot not yet used.
    const NONE: Self = Self {
        kind: Kind::Word,
        start: 0,
        end: 0,
        colon: 0,
    };

    /// `token` in 32 bits; an offset past them (a line over 4 GiB, which
    /// no file holds) stops at the largest.
    pub(super) fn of(token: Token) -> Self {
        let narrow = |at| u32::try_from(at).unwrap_or(u32::MAX);
        Self {
            kind: token.kind,
            start: narrow(token.start),
            end: narrow(token.end),
            colon: narrow(token.colon),
        }
    }

    const fn widen(self) -> Token {
        Token {
            kind: self.kind,
            start: self.start as usize,
            end: self.end as usize,
            colon: self.colon as usize,
        }
    }
}

/// The tokens of the lines parsed together.
#[derive(Clone)]
enum Shared {
    /// The tokens of one line parsed alone, exactly as many as it has.
    Line(Arc<[Narrow]>),
    /// The tokens of a file, in the vector they were collected in, so
    /// that they are not copied once more.
    File(Arc<Vec<Narrow>>),
}

impl Shared {
    /// The `len` tokens from `first` on.
    fn iter(&self, first: usize, len: usize) -> Iter<'_> {
        let range = first..first + len;
        Iter(
            match self {
                Self::Line(all) => all.get(range),
                Self::File(all) => all.get(range),
            }
            .unwrap_or_default()
            .iter(),
        )
    }
}

/// The tokens of a line, in order.
pub(super) struct Iter<'a>(std::slice::Iter<'a, Narrow>);

impl Iterator for Iter<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Token> {
        self.0.nth(n).copied().map(Narrow::widen)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for Iter<'_> {}

/// Collects the tokens of lines as they are parsed.
#[derive(Default)]
pub(super) struct Builder(Vec<Narrow>);

impl Builder {
    /// The number of tokens so far.
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn push(&mut self, token: Token) {
        self.0.push(Narrow::of(token));
    }

    /// The tokens, ready to be shared by the lines that hold them.
    pub(super) fn finish(mut self) -> Store {
        if self.0.is_empty() {
            return Store(Shared::Line(Arc::default()));
        }
        self.0.shrink_to_fit();
        Store(Shared::File(Arc::new(self.0)))
    }
}

/// The finished tokens of the lines parsed together.
pub(super) struct Store(Shared);

impl Store {
    /// The `len` tokens from `first` on.
    pub(super) fn tokens(&self, first: usize, len: usize) -> Tokens {
        Tokens {
            all: self.0.clone(),
            first,
            len,
        }
    }
}

/// How many tokens a line parsed alone keeps on the stack before they
/// are shared; most lines have fewer.
const FEW: usize = 32;

/// The tokens of one line: a range of the shared array.
#[derive(Clone)]
pub(super) struct Tokens {
    all: Shared,
    first: usize,
    len: usize,
}

impl Tokens {
    /// The tokens of a line parsed alone, `words` in order, in an array
    /// of their own: one allocation when there are fewer than `FEW`
    /// (none when there are none), a collected vector copied in otherwise.
    pub(super) fn alone<I>(words: I) -> Self
    where
        I: Iterator<Item = Token> + Clone,
    {
        let mut few = [Narrow::NONE; FEW];
        let mut len = 0;
        for (token, slot) in words.clone().zip(few.iter_mut()) {
            *slot = Narrow::of(token);
            len += 1;
        }
        let all = if len < FEW {
            share(few.get(..len).unwrap_or_default())
        } else {
            words.map(Narrow::of).collect()
        };
        Self {
            len: all.len(),
            all: Shared::Line(all),
            first: 0,
        }
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn get(&self, i: usize) -> Option<Token> {
        self.iter().nth(i)
    }

    /// The tokens in order.
    pub(super) fn iter(&self) -> Iter<'_> {
        self.all.iter(self.first, self.len)
    }
}

impl PartialEq for Tokens {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl Eq for Tokens {}

impl fmt::Debug for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tokens_tests;
