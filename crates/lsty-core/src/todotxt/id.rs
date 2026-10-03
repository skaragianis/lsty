use std::fmt;

/// The characters of a generated id: lowercase Crockford base32, which has
/// no `i`, `l`, `o` or `u`.
pub const ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

const MAX_CHOSEN_LEN: usize = 64;
const MIN_GENERATED_LEN: usize = 4;

/// An id that lsty may write: 1 to 64 ASCII letters, digits, `-`, `_` and
/// `.`, starting with a letter or digit.
///
/// An id written by another tool may break the rule; it stays a plain
/// byte string in [`Tags`](super::Tags) and cannot become an `Id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(String);

impl Id {
    /// Whether lsty may write `s` as an id. Does not allocate.
    pub fn is_valid(s: &[u8]) -> bool {
        s.split_first().is_some_and(|(first, rest)| {
            s.len() <= MAX_CHOSEN_LEN
                && first.is_ascii_alphanumeric()
                && rest
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(b))
        })
    }

    /// The id for `s`, or `None` when lsty may not write it.
    pub fn new(s: impl AsRef<[u8]>) -> Option<Self> {
        let bytes = s.as_ref();
        if Self::is_valid(bytes) {
            String::from_utf8(bytes.to_vec()).ok().map(Self)
        } else {
            None
        }
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `s` looks like an id lsty generates: 4 or more characters of
/// [`ID_ALPHABET`].
pub fn is_generated_id(s: impl AsRef<[u8]>) -> bool {
    let s = s.as_ref();
    s.len() >= MIN_GENERATED_LEN && s.iter().all(|b| ID_ALPHABET.contains(b))
}
