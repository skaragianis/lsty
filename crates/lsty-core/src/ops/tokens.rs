//! Changes to the tokens of a line, with a stamp-like word refused.

use super::tx::Tx;
use crate::errs::Result;
use crate::list::ItemId;
use crate::todotxt::{Kind, Line, TokenView};

/// The index of the first tag with `key`.
pub(super) fn first_of_key(line: &Line, key: &[u8]) -> Option<usize> {
    line.tokens().position(|t| is_key(&t, key))
}

/// Whether `tok` is a tag with `key`.
pub(super) fn is_key(tok: &TokenView<'_>, key: &[u8]) -> bool {
    tok.kind == Kind::Tag && tok.key == key
}

/// Whether `tok` is a `started:` or `owner:` tag.
pub(super) fn is_claim_tag(tok: &TokenView<'_>) -> bool {
    is_key(tok, b"started") || is_key(tok, b"owner")
}

/// The texts of the `started:` and `owner:` tokens, sorted.
pub(super) fn claim_tokens(line: &Line) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = line
        .tokens()
        .filter(is_claim_tag)
        .map(|t| t.text.to_vec())
        .collect();
    out.sort();
    out
}

impl Tx {
    /// `line` with `token` appended; a token that would turn into a
    /// stamp is refused for item `id`.
    pub(super) fn append_tok(
        &self,
        id: ItemId,
        line: &Line,
        token: &[u8],
    ) -> Result<Line> {
        line.append_token(token)
            .map_err(|_| self.stamp_word_refusal(id))
    }

    /// `line` without every token `matches` accepts, and how many that
    /// were.
    pub(super) fn remove_where(
        &self,
        id: ItemId,
        line: &Line,
        matches: impl Fn(&TokenView<'_>) -> bool,
    ) -> Result<(Line, usize)> {
        cut_where(line, matches).ok_or_else(|| self.stamp_word_refusal(id))
    }
}

/// `line` without every token `matches` accepts, and how many that were;
/// `None` when a removal would turn a word into a stamp.
pub(super) fn cut_where(
    line: &Line,
    matches: impl Fn(&TokenView<'_>) -> bool,
) -> Option<(Line, usize)> {
    let found: Vec<usize> = line
        .tokens()
        .enumerate()
        .filter(|(_, t)| matches(t))
        .map(|(i, _)| i)
        .collect();
    let mut out = line.clone();
    for &i in found.iter().rev() {
        out = out.remove_token(i).ok()?;
    }
    Some((out, found.len()))
}
