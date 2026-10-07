//! `append`, `prepend`, `replace` and `del NR TERM`: edits of the text of
//! an item.

use super::add::{backdated, own_priority};
use super::claim::{ClaimMode, Guard};
use super::env::Env;
use super::lifecycle::ClaimOpts;
use super::newline::invalid_text;
use super::pri::reopen_first;
use super::refs::Target;
use super::result::Outcome;
use super::rules::{edit_text, repeat_warnings};
use super::run::run;
use super::tokens::first_of_key;
use super::tx::{Tx, stamped};
use crate::errs::{Code, Error, Result};
use crate::list::ItemId;
use crate::todotxt::{Kind, Line, Parts, Priority, build, trim, trim_range};

/// The marks a sentence goes on after: appended text starting with one
/// of them needs no space before it.
const SENTENCE_MARKS: [u8; 4] = *b",.:;";

/// The keys whose old tokens a replacement keeps unless the new text has
/// the key.
const STRUCTURAL: [&[u8]; 4] = [b"dep", b"started", b"owner", b"rec"];

/// Makes the new raw line from the old line and the typed text.
type Builder = fn(&Line, &[u8]) -> Vec<u8>;

/// Adds `text` at the end of each target.
///
/// # Errors
///
/// A usage error for empty text, `invalid_value` for a bad text, and
/// whatever the rules refuse. Nothing is written then.
pub fn append(
    env: &Env<'_>,
    targets: &[Target],
    text: &str,
) -> Result<Outcome> {
    typed(env, "append", targets, text, appended)
}

/// Puts `text` in front of the description of each target.
///
/// # Errors
///
/// As [`append`].
pub fn prepend(
    env: &Env<'_>,
    targets: &[Target],
    text: &str,
) -> Result<Outcome> {
    typed(env, "prepend", targets, text, prepended)
}

fn typed(
    env: &Env<'_>,
    action: &str,
    targets: &[Target],
    text: &str,
    build: Builder,
) -> Result<Outcome> {
    let text = edit_text(text, action, env.clock.today())?;
    run(env, action, false, |tx| {
        tx.each_edit(targets, &env.actor, |tx, id| {
            type_into(tx, env, id, build, text.as_bytes())
        })
    })
}

fn type_into(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    build: Builder,
    typed: &[u8],
) -> Result<()> {
    let Some(old) = tx.line(id).cloned() else {
        return Ok(());
    };
    let handle = tx.handle(id);
    let raw = tx.set_edited(&env.actor, id, build(&old, typed))?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    for warning in repeat_warnings(&handle, &old, typed) {
        tx.say(warning);
    }
    Ok(())
}

fn appended(line: &Line, text: &[u8]) -> Vec<u8> {
    let mut raw = line.raw().to_vec();
    if !text.first().is_some_and(|c| SENTENCE_MARKS.contains(c)) {
        raw.push(b' ');
    }
    raw.extend_from_slice(text);
    raw
}

fn prepended(line: &Line, text: &[u8]) -> Vec<u8> {
    let raw = line.raw();
    let head = raw.len() - line.description().len();
    let joined = [text, b" ", line.description()].concat();
    [raw.get(..head).unwrap_or_default(), trim(&joined)].concat()
}

/// Gives each target the new text and keeps its structure.
///
/// # Errors
///
/// As [`append`]; also `wrong_state` for a text that changes a stamp, and
/// `claimed` for an agent on another actor's claim without `takeover`.
pub fn replace(
    env: &Env<'_>,
    targets: &[Target],
    text: &str,
    opts: ClaimOpts,
) -> Result<Outcome> {
    let text = edit_text(text, "replace", env.clock.today())?;
    run(env, "replace", false, |tx| {
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Guarded,
            opts,
        };
        tx.each_guarded(targets, guard, |tx, id| {
            replace_item(tx, env, id, &text)
        })
    })
}

fn replace_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    text: &str,
) -> Result<()> {
    let Some(old) = tx.line(id).cloned() else {
        return Ok(());
    };
    let handle = tx.handle(id);
    let raw = replacement(tx, id, &old, text)?;
    if raw == old.raw() {
        tx.say(format!("{handle} unchanged"));
        return Ok(());
    }
    let raw = tx.set_edited(&env.actor, id, raw)?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    Ok(())
}

fn replacement(
    tx: &Tx,
    id: ItemId,
    old: &Line,
    text: &str,
) -> Result<Vec<u8>> {
    let (own, body) = own_priority(text);
    if backdated(body) {
        let handle = tx.handle(id);
        return Err(stamped(
            tx.snapshot(),
            id,
            &handle,
            "the creation date is stamped",
        ));
    }
    let priority = kept_priority(tx, id, old, own)?;
    let mut description = body.as_bytes().to_vec();
    description.extend(kept_tokens(old, &Line::parse(body.as_bytes())));
    Ok(build(&Parts {
        done: old.is_done(),
        priority,
        completed: old.completed(),
        created: old.created(),
        description: &description,
    }))
}

/// The priority field of the new line: the text's own on an open item,
/// the old one on a done item, which refuses another.
fn kept_priority(
    tx: &Tx,
    id: ItemId,
    old: &Line,
    own: Option<Priority>,
) -> Result<Option<Priority>> {
    let Some(own) = own else {
        return Ok(old.priority());
    };
    if !old.is_done() {
        return Ok(Some(own));
    }
    if Some(own) != old.effective_priority()
        && let Some(item) = tx.item(id)
    {
        return Err(reopen_first(tx, id, item));
    }
    Ok(old.priority())
}

/// The old structural tokens the new text lacks, then the old id, each
/// after a space.
fn kept_tokens(old: &Line, next: &Line) -> Vec<u8> {
    let done = old.is_done();
    let keeps = |tok: &crate::todotxt::TokenView<'_>| {
        tok.kind == Kind::Tag
            && (STRUCTURAL.contains(&tok.key) || (done && tok.key == b"pri"))
            && first_of_key(next, tok.key).is_none()
    };
    let mut kept: Vec<Vec<u8>> = old
        .tokens()
        .filter(keeps)
        .map(|t| t.text.to_vec())
        .collect();
    if let Some(id) = old.id()
        && first_of_key(next, b"id").is_none()
    {
        kept.push([b"id:".as_slice(), id].concat());
    }
    kept.iter()
        .flat_map(|t| [b" ", t.as_slice()].concat())
        .collect()
}

/// Removes `term` from the description of each target where it stands as
/// a whole word (or a run of whole words): between the start or end of
/// the description or spaces or tabs.
///
/// # Errors
///
/// A usage error for an empty term, `invalid_value` for a term with a
/// line break, and whatever the rules refuse.
pub fn del_term(
    env: &Env<'_>,
    targets: &[Target],
    term: &str,
) -> Result<Outcome> {
    let term = term_text(term)?;
    run(env, "del", false, |tx| {
        tx.each_edit(targets, &env.actor, |tx, id| {
            delete_term(tx, env, id, term)
        })
    })
}

fn term_text(raw: &str) -> Result<&str> {
    let term = raw.trim();
    if term.is_empty() {
        return Err(Error::new(Code::Usage, "del needs a term"));
    }
    if term.contains(['\r', '\n']) {
        return Err(invalid_text("a term is one line", ""));
    }
    Ok(term)
}

fn delete_term(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    term: &str,
) -> Result<()> {
    let Some(line) = tx.line(id).cloned() else {
        return Ok(());
    };
    let handle = tx.handle(id);
    let desc = line.description();
    if whole_words(desc, term.as_bytes()).next().is_none() {
        tx.say(format!("{handle} has no \"{term}\""));
        return Ok(());
    }
    let head = line.raw().len() - desc.len();
    // white space at the start of the description stays, so the word
    // after it cannot become a priority or a date
    let lead = desc.get(..trim_range(desc).start).unwrap_or_default();
    let rest = desc.get(lead.len()..).unwrap_or_default();
    let kept = without_term(rest, term.as_bytes());
    let lead = if kept.is_empty() { &[] } else { lead };
    let raw =
        [line.raw().get(..head).unwrap_or_default(), lead, &kept].concat();
    keeps_priority(tx, id, &line, &raw)?;
    let raw = tx.set_edited(&env.actor, id, raw)?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    Ok(())
}

/// Refuses taking words out of an item when the words left would make a
/// priority, as it refuses a done mark or a date. (The priority in front
/// is never edited, so a change of it is only a safety net.) Typed text
/// may start with a priority on purpose, so `prepend` and `replace` do
/// not use this.
fn keeps_priority(tx: &Tx, id: ItemId, old: &Line, raw: &[u8]) -> Result<()> {
    if Line::parse(raw.to_vec()).priority() == old.priority() {
        return Ok(());
    }
    Err(tx.stamp_word_refusal(id))
}

/// The offsets where `term` stands in `hay` between the ends or blanks.
/// Matches do not overlap; an empty term has none.
fn whole_words<'a>(
    hay: &'a [u8],
    term: &'a [u8],
) -> impl Iterator<Item = usize> + 'a {
    let mut from = 0;
    std::iter::from_fn(move || {
        if term.is_empty() {
            return None;
        }
        let at = (from..=hay.len().checked_sub(term.len())?).find(|&at| {
            hay.get(at..at + term.len()) == Some(term)
                && at
                    .checked_sub(1)
                    .is_none_or(|b| hay.get(b).is_some_and(|c| is_blank(*c)))
                && hay.get(at + term.len()).is_none_or(|c| is_blank(*c))
        })?;
        from = at + term.len();
        Some(at)
    })
}

/// `desc` without `term`; the blanks around a removal (or around a run of
/// removals with only blanks between them) become one space, or nothing
/// when there are none.
fn without_term(desc: &[u8], term: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(desc.len());
    let mut matches = whole_words(desc, term).peekable();
    let mut pos = 0;
    while let Some(first) = matches.next() {
        let mut start = first;
        while start > pos && desc.get(start - 1).is_some_and(|b| is_blank(*b))
        {
            start -= 1;
        }
        let mut end = first + term.len();
        let mut removed = term.len();
        loop {
            let next = skip_blanks(desc, end);
            if matches.peek() == Some(&next) {
                matches.next();
                end = next + term.len();
                removed += term.len();
            } else {
                end = next;
                break;
            }
        }
        out.extend_from_slice(desc.get(pos..start).unwrap_or_default());
        if end - start > removed {
            out.push(b' ');
        }
        pos = end;
    }
    out.extend_from_slice(desc.get(pos..).unwrap_or_default());
    trim(&out).to_vec()
}

const fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn skip_blanks(bytes: &[u8], from: usize) -> usize {
    let tail = bytes.get(from..).unwrap_or_default();
    from + tail.iter().take_while(|b| is_blank(**b)).count()
}
