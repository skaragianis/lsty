use super::model::Word;
use crate::case::{lower, simple_lower};

/// Characters that are upper case only by the `Other_Uppercase` property:
/// letters in circles and roman numerals. They do not make a word exact.
/// The table is exact for Unicode 17; a test fails when the standard
/// library moves to another version, and then the table is derived again.
const OTHER_UPPERCASE: [(char, char); 5] = [
    ('\u{2160}', '\u{216F}'),
    ('\u{24B6}', '\u{24CF}'),
    ('\u{1F130}', '\u{1F149}'),
    ('\u{1F150}', '\u{1F169}'),
    ('\u{1F170}', '\u{1F189}'),
];

/// Whether `c` is an upper-case letter (category `Lu`).
pub fn is_upper(c: char) -> bool {
    c.is_uppercase()
        && !OTHER_UPPERCASE.iter().any(|&(a, b)| (a..=b).contains(&c))
}

/// The first character of `bytes` and its width. A byte that does not
/// start a UTF-8 sequence is U+FFFD and one byte wide.
fn next_char(bytes: &[u8]) -> Option<(char, usize)> {
    let first = *bytes.first()?;
    if first.is_ascii() {
        return Some((char::from(first), 1));
    }
    let head = bytes.get(..bytes.len().min(4))?;
    let valid = match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(e) => std::str::from_utf8(head.get(..e.valid_up_to())?).ok()?,
    };
    match valid.chars().next() {
        Some(c) => Some((c, c.len_utf8())),
        None => Some((char::REPLACEMENT_CHARACTER, 1)),
    }
}

impl Word {
    /// A word that matches by smartcase: exact when it has a capital,
    /// otherwise without regard to case.
    pub(super) fn new(text: &str) -> Self {
        let exact = text.chars().any(is_upper);
        if exact {
            return Self {
                needle: text.as_bytes().to_vec(),
                folded: Vec::new(),
                exact,
            };
        }
        let needle = lower(text);
        let folded = needle.chars().collect();
        Self {
            needle: needle.into_bytes(),
            folded,
            exact,
        }
    }

    /// Whether the word is a substring of `description`. It does not
    /// allocate.
    pub(super) fn found_in(&self, description: &[u8]) -> bool {
        if self.exact {
            return contains(description, &self.needle, <[u8]>::eq);
        }
        if description.is_ascii() {
            return contains(
                description,
                &self.needle,
                <[u8]>::eq_ignore_ascii_case,
            );
        }
        self.found_folding(description)
    }

    /// The slow path: fold the description as it is read.
    fn found_folding(&self, description: &[u8]) -> bool {
        let mut rest = description;
        loop {
            if self.starts_here(rest) {
                return true;
            }
            let Some((_, width)) = next_char(rest) else {
                return false;
            };
            rest = rest.get(width..).unwrap_or_default();
        }
    }

    fn starts_here(&self, mut rest: &[u8]) -> bool {
        for &want in &self.folded {
            let Some((c, width)) = next_char(rest) else {
                return false;
            };
            if simple_lower(c) != want {
                return false;
            }
            rest = rest.get(width..).unwrap_or_default();
        }
        true
    }
}

/// Whether two equal-length byte strings are the same, in some sense.
type Same = fn(&[u8], &[u8]) -> bool;

pub(crate) fn contains(haystack: &[u8], needle: &[u8], same: Same) -> bool {
    needle.is_empty()
        || haystack.windows(needle.len()).any(|w| same(w, needle))
}

#[cfg(test)]
mod tests {
    use super::is_upper;

    /// `OTHER_UPPERCASE` and `errs::quote_table::NOT_PRINTABLE` are both
    /// derived for this version.
    #[test]
    fn the_tables_are_for_the_unicode_version_of_the_standard_library() {
        assert_eq!(char::UNICODE_VERSION, (17, 0, 0));
    }

    #[test]
    fn capitals_are_letters_of_category_lu() {
        assert!(is_upper('A'));
        assert!(is_upper('\u{c9}'));
        assert!(!is_upper('\u{1c5}'));
        assert!(!is_upper('\u{24b6}'));
        assert!(!is_upper('\u{2160}'));
        assert!(!is_upper('a'));
    }
}
