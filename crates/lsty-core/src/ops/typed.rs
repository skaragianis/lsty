//! The line a person typed in place of an item's line.
//! Typing is not lost for a mistake that can be flagged instead, so a
//! bad date or recurrence, a second value for a key and a dependency
//! cycle are written and warned about.

use std::io::Write as _;

use super::env::Env;
use super::refs::{Target, no_item};
use super::rules::SETTABLE_KEYS;
use super::tx::{Tx, wrong_state};
use crate::dates::{self, Date, Resolved};
use crate::errs::{Code, Error, Result};
use crate::list::{Item, ItemId, Snapshot};
use crate::todotxt::{Kind, Line, Rec, TokenView};

/// The warning that goes with each text the rules let through.
type Warnings = Vec<Vec<u8>>;

/// Stages the typed line over the item, and says what was done.
pub(super) fn stage_typed(
    tx: &mut Tx,
    env: &Env<'_>,
    (id, target): (ItemId, &Target),
    (base, draft): (&[u8], &[u8]),
) -> Result<()> {
    let Some(item) = tx.item(id) else {
        return Err(no_item(target));
    };
    let old = tx.snapshot().line(item);
    let handle = tx.handle(id);
    if old.raw() != base {
        return Err(Error::new(
            Code::EditConflict,
            format!("{handle} changed while it was being edited"),
        ));
    }
    if !Line::parse(draft).has_text() {
        return Err(empty_task(tx.snapshot(), item));
    }
    let typed = trim_end(draft);
    let (text, mut warns) = soft_tags(typed, old, env.clock.today());
    let text = tx.without_claim(&env.actor, id, text)?;
    if !Line::parse(&text).has_text() {
        return Err(empty_task(tx.snapshot(), item));
    }
    warns.extend(repeated_keys(&handle, old, &text));
    if text == old.raw() {
        tx.say(format!("{handle} unchanged"));
        return Ok(());
    }
    let text = tx.set_edited(&env.actor, id, text)?;
    tx.say([handle.as_bytes(), b" ", &text].concat());
    for warning in warns {
        tx.say(warning);
    }
    Ok(())
}

fn empty_task(snap: &Snapshot, item: &Item) -> Error {
    wrong_state(
        snap,
        item,
        item.state(),
        "empty task".to_owned(),
        "use dd to delete".to_owned(),
    )
}

/// `text` without the spaces and tabs at its end.
pub(super) fn trim_end(text: &[u8]) -> &[u8] {
    let keep = text
        .iter()
        .rposition(|b| !matches!(b, b' ' | b'\t'))
        .map_or(0, |at| at + 1);
    text.get(..keep).unwrap_or_default()
}

/// The tags of `text` that `old` does not hold; a tag `old` holds twice
/// is held for two of the tags of `text`.
fn typed_tags<'a>(text: &'a Line, old: &Line) -> Vec<TokenView<'a>> {
    let mut held: Vec<&[u8]> = old.tokens().map(|t| t.text).collect();
    let mut out = Vec::new();
    for tok in text.tokens().filter(|t| t.kind == Kind::Tag) {
        match held.iter().position(|h| *h == tok.text) {
            Some(at) => {
                held.swap_remove(at);
            }
            None => out.push(tok),
        }
    }
    out
}

/// `text` with the fuzzy dates of the tags typed in it written as ISO
/// dates, and a warning for each typed value that is not valid. Tags
/// that `old` already holds are left alone.
pub(super) fn soft_tags(
    text: &[u8],
    old: &Line,
    today: Date,
) -> (Vec<u8>, Warnings) {
    let line = Line::parse(text);
    let mut out = Vec::with_capacity(text.len());
    let mut warns = Vec::new();
    let mut last = 0;
    for tok in typed_tags(&line, old) {
        let bad = if tok.is_tag(b"rec") {
            if Rec::parse(tok.value).is_some() {
                continue;
            }
            " is not valid"
        } else if tok.is_tag(b"due") || tok.is_tag(b"t") {
            let value = String::from_utf8_lossy(tok.value);
            match dates::resolve(&value, today) {
                Ok(Resolved::On(date)) => {
                    out.extend_from_slice(
                        text.get(last..tok.start).unwrap_or_default(),
                    );
                    out.extend_from_slice(tok.key);
                    write!(out, ":{date}").ok();
                    last = tok.start + tok.text.len();
                    continue;
                }
                Ok(_) => continue,
                Err(_) => " is not a date",
            }
        } else {
            continue;
        };
        warns.push([tok.text, bad.as_bytes()].concat());
    }
    out.extend_from_slice(text.get(last..).unwrap_or_default());
    (out, warns)
}

/// How many tags with `key` the line has.
fn count_key(line: &Line, key: &[u8]) -> usize {
    line.tokens().filter(|t| t.is_tag(key)).count()
}

/// A warning for each single-valued key that `typed` has more often
/// than `old`, and more than once.
pub(super) fn repeated_keys(
    handle: &str,
    old: &Line,
    typed: &[u8],
) -> Warnings {
    let typed = Line::parse(typed);
    SETTABLE_KEYS
        .iter()
        .filter_map(|key| {
            let n = count_key(&typed, key);
            if n < 2 || n <= count_key(old, key) {
                return None;
            }
            let first = typed.tokens().find(|t| t.is_tag(key))?;
            Some(
                [
                    handle.as_bytes(),
                    b" already has ",
                    first.text,
                    b"; the first one counts",
                ]
                .concat(),
            )
        })
        .collect()
}
