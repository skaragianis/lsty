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
use crate::todotxt::{Kind, Line, Parts, Priority, build, trim};

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

/// Removes every occurrence of `term` from the description of each
/// target.
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
    if find(desc, term.as_bytes()).is_none() {
        tx.say(format!("{handle} has no \"{term}\""));
        return Ok(());
    }
    let head = line.raw().len() - desc.len();
    let raw = [
        line.raw().get(..head).unwrap_or_default(),
        &without_term(desc, term.as_bytes()),
    ]
    .concat();
    let raw = tx.set_edited(&env.actor, id, raw)?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    Ok(())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// `desc` without `term`; the blanks around a removal become one space.
fn without_term(desc: &[u8], term: &[u8]) -> Vec<u8> {
    let mut marked = Vec::with_capacity(desc.len());
    let mut rest = desc;
    while let Some(at) = find(rest, term) {
        marked.extend_from_slice(rest.get(..at).unwrap_or_default());
        marked.push(MARK);
        rest = rest.get(at + term.len()..).unwrap_or_default();
    }
    marked.extend_from_slice(rest);
    trim(&collapse_gaps(&marked)).to_vec()
}

const MARK: u8 = 0;

const fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn skip_blanks(bytes: &[u8], from: usize) -> usize {
    let tail = bytes.get(from..).unwrap_or_default();
    from + tail.iter().take_while(|b| is_blank(**b)).count()
}

/// Replaces each run of marks with the blanks around and between them by
/// one space, or by nothing when there are no blanks.
fn collapse_gaps(marked: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(marked.len());
    let mut i = 0;
    while i < marked.len() {
        let j = skip_blanks(marked, i);
        if marked.get(j) == Some(&MARK) {
            let (end, blank) = gap_end(marked, i, j);
            if blank {
                out.push(b' ');
            }
            i = end;
            continue;
        }
        out.extend_from_slice(marked.get(i..j).unwrap_or_default());
        if let Some(&b) = marked.get(j) {
            out.push(b);
        }
        i = j + 1;
    }
    out
}

/// Where the run of marks that starts at `first` ends (after the blanks
/// that follow it), and whether it holds a blank. The run began at
/// `start`, `first - start` blanks earlier.
fn gap_end(marked: &[u8], start: usize, first: usize) -> (usize, bool) {
    let mut blank = first > start;
    let mut end = first + 1;
    loop {
        let next = skip_blanks(marked, end);
        blank |= next > end;
        if marked.get(next) == Some(&MARK) {
            end = next + 1;
        } else {
            return (next, blank);
        }
    }
}
