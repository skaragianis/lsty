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

/// A [`Token`] whose offsets fit in 32 bits; half its size.
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
    /// The tokens of one line parsed alone, exactly as many as it has.
    Line(Arc<[Narrow]>),
    /// The tokens of a file, in the vector they were collected in, so
    /// that they are not copied once more.
    File(Arc<Vec<Narrow>>),
    /// Tokens of which one did not fit in 32 bits.
    Wide(Arc<[Token]>),
}

impl Shared {
    /// The `len` tokens from `first` on.
    fn iter(&self, first: usize, len: usize) -> Iter<'_> {
        let range = first..first.saturating_add(len);
        match self {
            Self::Line(all) => {
                Iter::Narrow(all.get(range).unwrap_or_default().iter())
            }
            Self::File(all) => {
                Iter::Narrow(all.get(range).unwrap_or_default().iter())
            }
            Self::Wide(all) => {
                Iter::Wide(all.get(range).unwrap_or_default().iter())
            }
        }
    }
}

/// The tokens of a line, in order.
pub(super) enum Iter<'a> {
    Narrow(std::slice::Iter<'a, Narrow>),
    Wide(std::slice::Iter<'a, Token>),
}

impl Iterator for Iter<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        match self {
            Self::Narrow(all) => all.next().copied().map(Narrow::widen),
            Self::Wide(all) => all.next().copied(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Narrow(all) => all.size_hint(),
            Self::Wide(all) => all.size_hint(),
        }
    }
}

impl ExactSizeIterator for Iter<'_> {}

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
            Growing::Narrow(all) if all.is_empty() => {
                Shared::Line(Arc::default())
            }
            Growing::Narrow(mut all) => {
                all.shrink_to_fit();
                Shared::File(Arc::new(all))
            }
            Growing::Wide(all) => Shared::Wide(all.into()),
        })
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
    /// of their own made in one allocation (none when there are none).
    pub(super) fn alone<I>(words: I, limit: usize) -> Self
    where
        I: Iterator<Item = Token> + Clone,
    {
        let mut few = [Narrow::NONE; FEW];
        let mut more = Vec::new();
        let mut len = 0;
        for token in words.clone() {
            let Some(narrow) = Narrow::of(token, limit) else {
                let all: Arc<[Token]> = words.collect();
                return Self {
                    len: all.len(),
                    all: Shared::Wide(all),
                    first: 0,
                };
            };
            match few.get_mut(len) {
                Some(slot) => *slot = narrow,
                None if more.is_empty() => {
                    more.extend_from_slice(&few);
                    more.push(narrow);
                }
                None => more.push(narrow),
            }
            len += 1;
        }
        let all: Arc<[Narrow]> = match (len, more.is_empty()) {
            (0, _) => Arc::default(),
            (_, true) => Arc::from(few.get(..len).unwrap_or_default()),
            (_, false) => more.into(),
        };
        Self {
            all: Shared::Line(all),
            first: 0,
            len,
        }
    }

    pub(super) const fn len(&self) -> usize {
        self.len
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
