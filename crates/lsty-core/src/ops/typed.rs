//! The line a person typed in place of a task's line.
//! Typing is not lost for a mistake that can be flagged instead, so a
//! bad date or recurrence, a second value for a key and a dependency
//! cycle are written and warned about.

use std::io::Write as _;

use super::{
    claim::{ClaimMode, Guard},
    env::Env,
    grant::{Grant, grant_ids},
    lifecycle::ClaimOpts,
    newline::invalid_text,
    refs::Target,
    result::Outcome,
    rules::SETTABLE_KEYS,
    run::{Rules, run_with},
    staged::Path,
    tx::{Tx, gone, wrong_state},
};
use crate::{
    dates::{self, Date},
    errs::{Error, Result, lossy},
    list::{Snapshot, Task, TaskId},
    todotxt::{Kind, Line, Rec, TokenView, is_gap},
};

/// The warning that goes with each text the rules let through.
type Warnings = Vec<Vec<u8>>;

/// Writes `draft` over the line `target` names. Each grant gives its id to
/// the task it names, in the same write.
///
/// # Errors
///
/// `expect_mismatch` for a line that is not the one aimed at,
/// `invalid_value` for a line break, `wrong_state` for an empty task and a
/// text that changes a stamp, and what the rules refuse: a second `id:`, a
/// taken id, a typed `started:` on a blocked task and a claim of another.
pub fn edit_typed(
    env: &Env<'_>,
    target: &Target,
    draft: &[u8],
    grants: &[Grant],
) -> Result<Outcome> {
    if draft.iter().any(|b| matches!(b, b'\r' | b'\n')) {
        return Err(invalid_text("a task is one line", ""));
    }
    run_with(env, "edit", false, Rules::WarnCycles, |tx| {
        let id = tx.anchor(target)?;
        tx.mark_primary(id)?;
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Guarded,
            opts: ClaimOpts::default(),
        };
        tx.guarded(guard, id, |tx| stage_typed(tx, env, id, draft))?;
        grant_ids(tx, env, grants)
    })
}

/// Stages the typed line over the task, and says what was done.
fn stage_typed(
    tx: &mut Tx,
    env: &Env<'_>,
    id: TaskId,
    draft: &[u8],
) -> Result<()> {
    let task = tx.task(id).ok_or_else(gone)?;
    let old = tx.snapshot().line(task);
    let handle = tx.handle(id);
    if !Line::parse(draft).has_text() {
        return Err(empty_task(tx.snapshot(), task));
    }
    let typed = trim_end(draft);
    let (text, mut warns) = soft_tags(typed, old, env.clock.today());
    let text = tx.without_claim(&env.actor, id, old, text)?;
    if !Line::parse(&text).has_text() {
        return Err(empty_task(tx.snapshot(), task));
    }
    warns.extend(repeated_keys(&handle, old, &text));
    if text == old.raw() {
        tx.say(format!("{handle} unchanged"));
        return Ok(());
    }
    tx.set(id, &text, Path::Typed);
    tx.say([handle.as_bytes(), b" ", &text].concat());
    for warning in warns {
        tx.say(warning);
    }
    Ok(())
}

fn empty_task(snap: &Snapshot, task: &Task) -> Error {
    wrong_state(
        snap,
        task,
        task.state(),
        "empty task".to_owned(),
        "use dd to delete".to_owned(),
    )
}

/// `text` without the spaces and tabs at its end.
fn trim_end(text: &[u8]) -> &[u8] {
    let keep = text
        .iter()
        .rposition(|b| !is_gap(*b))
        .map_or(0, |at| at + 1);
    text.get(..keep).unwrap_or_default()
}

/// The tags of `text` that `old` does not hold; a tag `old` holds twice
/// is held for two of the tags of `text`.
fn typed_tags<'a>(text: &'a Line, old: &Line) -> Vec<TokenView<'a>> {
    let mut held: Vec<&[u8]> = old
        .tokens()
        .filter(|t| t.kind == Kind::Tag)
        .map(|t| t.text)
        .collect();
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
            let value = lossy(tok.value);
            match dates::resolve(&value, today) {
                Ok(Some(date)) => {
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
fn repeated_keys(handle: &str, old: &Line, typed: &[u8]) -> Warnings {
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

#[cfg(test)]
mod typed_tests;
