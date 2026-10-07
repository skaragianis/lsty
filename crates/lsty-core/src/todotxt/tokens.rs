//! Where the tokens of parsed lines are kept. The lines parsed together
//! (a whole file, or one line) share one array of tokens, so parsing a
//! file allocates for its tokens once and cloning a line copies none.
//! Offsets are kept in 32 bits while every one fits.

use std::fmt;
use std::sync::Arc;

use super::line::Kind;

/// The largest offset a narrow token holds.
pub(super) const LIMIT: usize = u32::MAX as usize;

/// The position of a token in its line's raw bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) start: usize,
    pub(super) end: usize,
    /// Offset of the colon in the raw bytes; only set for tags.
    pub(super) colon: usize,
}

impl Token {
    const NONE: Self = Self {
        kind: Kind::Word,
        start: 0,
        end: 0,
        colon: 0,
    };
}

/// A [`Token`] whose offsets fit in 32 bits; half its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Narrow {
    kind: Kind,
    start: u32,
    end: u32,
    colon: u32,
}

impl Narrow {
    /// `token` in 32 bits, when its offsets are at most `limit` and fit.
    pub(super) fn of(token: Token, limit: usize) -> Option<Self> {
        if token.end > limit {
            return None;
        }
        Some(Self {
            kind: token.kind,
            start: u32::try_from(token.start).ok()?,
            end: u32::try_from(token.end).ok()?,
            colon: u32::try_from(token.colon).ok()?,
        })
    }

    pub(super) const fn widen(self) -> Token {
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
    Narrow(Arc<[Narrow]>),
    Wide(Arc<[Token]>),
}

impl Shared {
    fn get(&self, i: usize) -> Option<Token> {
        match self {
            Self::Narrow(all) => all.get(i).copied().map(Narrow::widen),
            Self::Wide(all) => all.get(i).copied(),
        }
    }
}

/// Collects the tokens of lines as they are parsed. It keeps them narrow
/// until one does not fit, then widens all of them.
pub(super) struct Builder {
    tokens: Growing,
    limit: usize,
}

enum Growing {
    Narrow(Vec<Narrow>),
    Wide(Vec<Token>),
}

impl Builder {
    /// A builder that keeps offsets up to `limit` narrow; room for
    /// `capacity` tokens.
    pub(super) fn new(limit: usize, capacity: usize) -> Self {
        Self {
            tokens: Growing::Narrow(Vec::with_capacity(capacity)),
            limit,
        }
    }

    /// The number of tokens so far.
    pub(super) fn len(&self) -> usize {
        match &self.tokens {
            Growing::Narrow(all) => all.len(),
            Growing::Wide(all) => all.len(),
        }
    }

    pub(super) fn push(&mut self, token: Token) {
        match &mut self.tokens {
            Growing::Narrow(all) => {
                if let Some(narrow) = Narrow::of(token, self.limit) {
                    all.push(narrow);
                } else {
                    let mut wide = Vec::with_capacity(all.capacity());
                    wide.extend(all.iter().copied().map(Narrow::widen));
                    wide.push(token);
                    self.tokens = Growing::Wide(wide);
                }
            }
            Growing::Wide(all) => all.push(token),
        }
    }

    /// The tokens, ready to be shared by the lines that hold them.
    pub(super) fn finish(self) -> Store {
        Store(match self.tokens {
            Growing::Narrow(all) => Shared::Narrow(all.into()),
            Growing::Wide(all) => Shared::Wide(all.into()),
        })
    }
}

/// The finished tokens of the lines parsed together.
pub(super) struct Store(Shared);

/// The tokens of one line: a range of the shared array.
#[derive(Clone)]
pub(super) struct Tokens {
    /// `None` until the tokens are placed.
    all: Option<Shared>,
    first: usize,
    len: usize,
}

impl Tokens {
    /// The `len` tokens from `first` on; they are found on the store
    /// given to [`Tokens::place`].
    pub(super) fn pending(first: usize, len: usize) -> Self {
        Self {
            all: None,
            first,
            len,
        }
    }

    /// Points these tokens at `store`.
    pub(super) fn place(&mut self, store: &Store) {
        self.all = Some(store.0.clone());
    }

    pub(super) const fn len(&self) -> usize {
        self.len
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn get(&self, i: usize) -> Option<Token> {
        if i < self.len {
            self.all.as_ref()?.get(self.first + i)
        } else {
            None
        }
    }

    /// The tokens in order.
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = Token> {
        (0..self.len).map(|i| self.get(i).unwrap_or(Token::NONE))
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
