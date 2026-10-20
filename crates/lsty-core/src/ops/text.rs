//! `append`, `prepend`, `replace` and `del NR TERM`: edits of the text of
//! a task.

use super::{
    add::{backdated, own_priority},
    claim::{ClaimMode, Guard},
    env::Env,
    lifecycle::ClaimOpts,
    newline::invalid_text,
    pri::reopen_first,
    refs::Target,
    result::Outcome,
    rules::{edit_text, repeat_warnings},
    run::run,
    tokens::first_of_key,
    tx::{Tx, gone, stamped},
};
use crate::{
    errs::{Code, Error, Result, quote},
    list::TaskId,
    todotxt::{
        Kind, Line, Parts, Priority, TokenView, build, inside_span,
        is_claim_key, is_gap, trim, trim_range,
    },
};

/// The marks a sentence goes on after: appended text starting with one
/// of them needs no space before it.
const SENTENCE_MARKS: [u8; 4] = *b",.:;";

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

/// Puts `text` at the beginning of each target.
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
    id: TaskId,
    build: Builder,
    typed: &[u8],
) -> Result<()> {
    let old = tx.line_of(id)?.clone();
    let handle = tx.handle(id);
    let raw = tx.set_edited(&env.actor, id, build(&old, typed))?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    for warning in repeat_warnings(&handle, &old, typed) {
        tx.say(warning);
    }
    Ok(())
}

/// The text goes after the line, or before the `id:` that ends it, which
/// then stays last.
fn appended(line: &Line, text: &[u8]) -> Vec<u8> {
    let (head, id) = line.raw().split_at(line.text_end());
    let mut out = head.to_vec();
    let glued = text.first().is_some_and(|c| SENTENCE_MARKS.contains(c));
    if !glued && !out.is_empty() {
        out.push(b' ');
    }
    out.extend_from_slice(text);
    if !id.is_empty() {
        out.push(b' ');
        out.extend_from_slice(id.trim_ascii_start());
    }
    out
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
            replace_task(tx, env, id, &text)
        })
    })
}

fn replace_task(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    text: &str,
) -> Result<()> {
    let old = tx.line_of(id)?.clone();
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
    id: TaskId,
    old: &Line,
    text: &str,
) -> Result<Vec<u8>> {
    let (own, body) = own_priority(text);
    if backdated(body).is_some() {
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
    description.extend(kept_tokens(old, &Line::parse(body)));
    Ok(build(&Parts {
        done: old.is_done(),
        priority,
        completed: old.completed(),
        created: old.created(),
        description: &description,
    }))
}

/// The priority field of the new line: the text's own on an open task,
/// the old one on a done task, which refuses another.
fn kept_priority(
    tx: &Tx,
    id: TaskId,
    old: &Line,
    own: Option<Priority>,
) -> Result<Option<Priority>> {
    let Some(own) = own else {
        return Ok(old.priority());
    };
    if !old.is_done() {
        return Ok(Some(own));
    }
    if Some(own) != old.effective_priority() {
        let task = tx.task(id).ok_or_else(gone)?;
        return Err(reopen_first(tx, id, task));
    }
    Ok(old.priority())
}

/// The old structural tokens the new text lacks, then the old id, each
/// after a space.
fn kept_tokens(old: &Line, next: &Line) -> Vec<u8> {
    let done = old.is_done();
    let keeps = |tok: &TokenView<'_>| {
        tok.kind == Kind::Tag
            && (matches!(tok.key, b"dep" | b"rec")
                || is_claim_key(tok.key)
                || (done && tok.key == b"pri"))
            && first_of_key(next, tok.key).is_none()
    };
    let mut kept: Vec<Vec<u8>> = old
        .tokens()
        .filter(keeps)
        .map(|t| t.text.to_vec())
        .collect();
    if let Some(id) = old.tags().id
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
    id: TaskId,
    term: &str,
) -> Result<()> {
    let line = tx.line_of(id)?.clone();
    let handle = tx.handle(id);
    let desc = line.description();
    let head = line.raw().len() - desc.len();
    // white space at the start of the description stays, so the word
    // after it cannot become a priority or a date
    let lead = desc.get(..trim_range(desc).start).unwrap_or_default();
    let Some(kept) = without_term(desc, lead.len(), term.as_bytes()) else {
        tx.say(format!("{handle} has no {}", quote(term)));
        return Ok(());
    };
    let raw =
        [line.raw().get(..head).unwrap_or_default(), lead, &kept].concat();
    if !line.same_stamps(&Line::parse(&raw)) {
        return Err(tx.stamp_word_refusal(id));
    }
    let raw = tx.set_edited(&env.actor, id, raw)?;
    tx.say([handle.as_bytes(), b" ", &raw].concat());
    Ok(())
}

/// The offsets where `term` stands in `hay` between the ends or blanks.
/// `term` is not empty. Matches do not overlap.
fn whole_words<'a>(
    hay: &'a [u8],
    term: &'a [u8],
) -> impl Iterator<Item = usize> + 'a {
    let mut from = 0;
    std::iter::from_fn(move || {
        let at = (from..=hay.len().checked_sub(term.len())?).find(|&at| {
            hay.get(at..at + term.len()) == Some(term)
                && at
                    .checked_sub(1)
                    .is_none_or(|b| hay.get(b).copied().is_some_and(is_gap))
                && hay.get(at + term.len()).copied().is_none_or(is_gap)
        })?;
        from = at + term.len();
        Some(at)
    })
}

/// What stays of `desc` after its first `from` bytes (white space that is
/// kept in front) once `term` is gone from every place it is a whole word:
/// the blanks around the removals become one space. `None` when there is
/// no such place. A place that cuts a quoted span stays.
fn without_term(desc: &[u8], from: usize, term: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(desc.len());
    let mut pos = from;
    let mut found = false;
    let cuts = |at: &usize| {
        inside_span(desc, *at) || inside_span(desc, at + term.len())
    };
    for at in whole_words(desc, term).filter(|at| !cuts(at)) {
        found = true;
        out.extend_from_slice(desc.get(pos..at).unwrap_or_default());
        while out.last().copied().is_some_and(is_gap) {
            out.pop();
        }
        out.push(b' ');
        pos = skip_blanks(desc, at + term.len());
    }
    out.extend_from_slice(desc.get(pos..).unwrap_or_default());
    found.then(|| trim(&out).to_vec())
}

fn skip_blanks(bytes: &[u8], from: usize) -> usize {
    let tail = bytes.get(from..).unwrap_or_default();
    from + tail.iter().take_while(|b| is_gap(**b)).count()
}
