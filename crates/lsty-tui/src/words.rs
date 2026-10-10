//! The words of the `:` line: cut at spaces and tabs, with quotes that group.

use crate::field::is_sep;

/// Where the cutting stands between two characters.
#[derive(Default)]
struct Scanner {
    words: Vec<String>,
    word: String,
    /// A word has begun, though it may be empty (`""`).
    began: bool,
    quote: Option<char>,
    /// The last character was a backslash inside double quotes.
    slash: bool,
}

impl Scanner {
    fn feed(&mut self, c: char) {
        if self.slash {
            self.escaped(c);
        } else if self.quote.is_some() {
            self.quoted(c);
        } else {
            self.bare(c);
        }
    }

    /// After a backslash: `"` and `\` stand for themselves, anything
    /// else keeps its backslash.
    fn escaped(&mut self, c: char) {
        if c != '"' && c != '\\' {
            self.add('\\');
        }
        self.add(c);
        self.slash = false;
    }

    fn quoted(&mut self, c: char) {
        if self.quote == Some('"') && c == '\\' {
            self.slash = true;
        } else if self.quote == Some(c) {
            self.quote = None;
        } else {
            self.add(c);
        }
    }

    fn bare(&mut self, c: char) {
        match c {
            '"' | '\'' => {
                self.quote = Some(c);
                self.began = true;
            }
            c if is_sep(c) => self.flush(),
            _ => self.add(c),
        }
    }

    fn add(&mut self, c: char) {
        self.word.push(c);
        self.began = true;
    }

    fn flush(&mut self) {
        if self.began {
            self.words.push(std::mem::take(&mut self.word));
            self.began = false;
        }
    }
}

/// The words of `line`, or `None` when a quote is not closed. Quotes
/// group a word; inside double quotes a backslash escapes a quote and
/// a backslash.
pub(crate) fn split_words(line: &str) -> Option<Vec<String>> {
    let mut scan = Scanner::default();
    for c in line.chars() {
        scan.feed(c);
    }
    if scan.quote.is_some() {
        return None;
    }
    scan.flush();
    Some(scan.words)
}

#[cfg(test)]
mod tests {
    use super::split_words;

    #[test]
    fn words_end_at_a_space_or_a_tab_only() {
        let got = split_words("a\tb\u{a0}c  d");
        assert_eq!(got, Some(vec!["a".into(), "b\u{a0}c".into(), "d".into()]));
    }
}
