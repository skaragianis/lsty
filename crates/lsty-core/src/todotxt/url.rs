//! The URL in a word: `https://x.org` in `(see:https://x.org/a).`

/// The marks that end a sentence or a quotation, not a URL.
const TRAILING: &[u8] = b".,;:!?>\"'";

/// The bracket pairs that a URL may hold in balance.
const PAIRS: [(u8, u8); 3] = [(b'(', b')'), (b'[', b']'), (b'{', b'}')];

/// The URL in `word`: from the first letter of the run of scheme bytes
/// (letters, digits, `+`, `-`, `.`) before the first `://` up to the end
/// of the word, less the marks and the unmatched closing brackets that
/// end it. Nothing before that letter belongs to it. `None` when there is
/// no letter, nothing after the slashes, or a control byte in the URL.
pub(crate) fn url_in(word: &[u8]) -> Option<&[u8]> {
    let at = word.windows(3).position(|w| w == b"://")?;
    let from = word
        .get(..at)?
        .iter()
        .rposition(|b| !(b.is_ascii_alphanumeric() || b"+-.".contains(b)))
        .unwrap_or(0);
    let start = from
        + word
            .get(from..at)?
            .iter()
            .position(u8::is_ascii_alphabetic)?;
    let mut end = word.len();
    loop {
        let url = word.get(start..end)?;
        let last = *url.last()?;
        let unmatched = PAIRS.iter().any(|&(open, close)| {
            last == close && count(url, close) > count(url, open)
        });
        if !(TRAILING.contains(&last) || unmatched) {
            break;
        }
        end -= 1;
    }
    let url = word.get(start..end).filter(|_| end > at + 3)?;
    (!url.iter().any(u8::is_ascii_control)).then_some(url)
}

fn count(s: &[u8], b: u8) -> usize {
    s.iter().map(|&c| usize::from(c == b)).sum()
}
