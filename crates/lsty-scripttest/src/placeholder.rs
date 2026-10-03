//! `<idN>` and `<any>` placeholders in expected output.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

/// The alphabet of generated ids.
const ID_ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
const MIN_ID_LEN: usize = 4;
const ANY: &str = "any";

/// Names bound to ids, such as `id1` → `k3f9`.
pub(crate) type Bindings = BTreeMap<String, String>;

#[expect(
    clippy::expect_used,
    reason = "the pattern is a constant and covered by the tests"
)]
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(id[0-9]+|any)>").expect("placeholder pattern")
});

enum Piece<'a> {
    Literal(&'a str),
    Name(&'a str),
}

fn pieces(pattern: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut last = 0;
    for caps in PLACEHOLDER.captures_iter(pattern) {
        let (Some(all), Some(name)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        if all.start() > last {
            pieces.push(Piece::Literal(&pattern[last..all.start()]));
        }
        pieces.push(Piece::Name(name.as_str()));
        last = all.end();
    }
    if last < pattern.len() {
        pieces.push(Piece::Literal(&pattern[last..]));
    }
    pieces
}

/// Matches `text` against `pattern`. On a match, returns the ids bound for
/// the first time; `bound` holds the bindings made earlier.
pub(crate) fn match_text(
    pattern: &str,
    text: &str,
    bound: &Bindings,
) -> Option<Bindings> {
    let mut found = Bindings::new();
    match_pieces(&pieces(pattern), text, bound, &mut found).then_some(found)
}

fn match_pieces(
    pieces: &[Piece<'_>],
    text: &str,
    bound: &Bindings,
    found: &mut Bindings,
) -> bool {
    let Some((head, rest)) = pieces.split_first() else {
        return text.is_empty();
    };
    match head {
        Piece::Literal(literal) => text
            .strip_prefix(literal)
            .is_some_and(|after| match_pieces(rest, after, bound, found)),
        Piece::Name(name) if *name == ANY => {
            match_any(rest, text, bound, found)
        }
        Piece::Name(name) => match_id(name, rest, text, bound, found),
    }
}

/// `<any>` takes as much of the line as it can and still match.
fn match_any(
    rest: &[Piece<'_>],
    text: &str,
    bound: &Bindings,
    found: &mut Bindings,
) -> bool {
    let end = text.find('\n').unwrap_or(text.len());
    for n in (0..=end).rev() {
        if !text.is_char_boundary(n) {
            continue;
        }
        let mut attempt = found.clone();
        if match_pieces(rest, &text[n..], bound, &mut attempt) {
            *found = attempt;
            return true;
        }
    }
    false
}

fn match_id(
    name: &str,
    rest: &[Piece<'_>],
    text: &str,
    bound: &Bindings,
    found: &mut Bindings,
) -> bool {
    if let Some(value) = bound.get(name).or_else(|| found.get(name)) {
        return text
            .strip_prefix(value.as_str())
            .is_some_and(|after| match_pieces(rest, after, bound, found));
    }
    let Some(token) = leading_id(text) else {
        return false;
    };
    found.insert(name.to_owned(), token.to_owned());
    match_pieces(rest, &text[token.len()..], bound, found)
}

/// The generated id at the start of `text`: at least four characters of
/// the alphabet, not followed by another word character.
fn leading_id(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let n = bytes.iter().take_while(|b| ID_ALPHABET.contains(b)).count();
    let word_follows = bytes
        .get(n)
        .is_some_and(|b| *b == b'_' || b.is_ascii_alphanumeric());
    (n >= MIN_ID_LEN && !word_follows).then(|| &text[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(pairs: &[(&str, &str)]) -> Bindings {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }
    #[test]
    fn binds_an_id() {
        let found = match_text("added <id1>\n", "added k3f9\n", &bound(&[]));
        assert_eq!(found, Some(bound(&[("id1", "k3f9")])));
    }
    #[test]
    fn bound_id_must_repeat() {
        let b = bound(&[("id1", "k3f9")]);
        assert!(match_text("<id1>", "a1b2", &b).is_none());
        assert_eq!(match_text("<id1>", "k3f9", &b), Some(bound(&[])));
        assert!(match_text("<id1> <id1>", "k3f9 a1b2", &bound(&[])).is_none());
    }
    #[test]
    fn rejects_text_that_is_not_an_id() {
        for text in ["K3F9", "k3i9", "k3f", "k3f9x_"] {
            assert!(
                match_text("<id1>", text, &bound(&[])).is_none(),
                "{text}"
            );
        }
    }

    #[test]
    fn any_stays_on_its_line() {
        let b = bound(&[]);
        assert!(match_text("a <any> z\n", "a b c z\n", &b).is_some());
        assert!(
            match_text("HOME=<any>/home\n", "HOME=/t/w/home\n", &b).is_some()
        );
        assert!(match_text("a <any>\nz\n", "a b\nc\nz\n", &b).is_none());
    }
}
