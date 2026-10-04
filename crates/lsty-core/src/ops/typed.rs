//! `edit_typed`: the line a person typed in place of an item's line.
//! Typing is not lost for a mistake that can be flagged instead, so a
//! bad date or recurrence, a second value for a key and a dependency
//! cycle are written and warned about.

use super::claim::{ClaimMode, Guard};
use super::env::Env;
use super::grant::{Grant, grant_ids};
use super::lifecycle::ClaimOpts;
use super::newline::invalid_text;
use super::pin::Pinned;
use super::refs::{Target, no_item};
use super::result::Outcome;
use super::rules::SETTABLE_KEYS;
use super::run::run;
use super::tokens::is_key;
use super::tx::{Picked, Tx, wrong_state};
use crate::dates::{self, Date, Resolved};
use crate::errs::{Code, Error, Result};
use crate::list::ItemId;
use crate::todotxt::{Kind, Line, TokenView, trim};

/// The warning that goes with each text the rules let through.
type Warnings = Vec<Vec<u8>>;

/// Writes `draft` over the line of the todo file that `line` follows,
/// which must still read as `base`, as the human typed it. The item is guarded as for
/// any edit; a text that leaves no description is refused. Each grant
/// gives its id to the item it names, in the same write.
///
/// # Errors
///
/// `no_item` when the line is gone, `edit_conflict` when it is not
/// `base` any more, `invalid_value` for a line break, `wrong_state` for
/// an empty item and a text that changes a stamp, and what the rules
/// refuse: a second `id:`, a taken id, a typed `started:` on a blocked
/// item and a claim of another.
pub fn edit_typed(
    env: &Env<'_>,
    line: &Pinned,
    base: &[u8],
    draft: &[u8],
    grants: &[Grant],
) -> Result<Outcome> {
    if draft.iter().any(|b| matches!(b, b'\r' | b'\n')) {
        return Err(invalid_text("a task is one line", ""));
    }
    let soft = Env {
        actor: env.actor.clone(),
        cancel: env.cancel.clone(),
        cycle_warn_only: true,
        ..*env
    };
    run(&soft, "edit", false, |tx| {
        let asked = Target::todo(i64::from(line.nr().get()));
        let nr = line
            .now(&tx.snapshot().todo().lines)
            .ok_or_else(|| no_item(&asked))?;
        let target = Target::todo(i64::from(nr.get()));
        let Picked::Item(id) = tx.target(&target)? else {
            return Err(no_item(&target));
        };
        tx.mark_primary(id);
        let guard = Guard {
            actor: &soft.actor,
            mode: ClaimMode::Guarded,
            opts: ClaimOpts::default(),
        };
        tx.guarded(guard, id, |tx| {
            stage_typed(tx, &soft, (id, &target), (base, draft))
        })?;
        grant_ids(tx, &soft, grants)
    })
}

/// Stages the typed line over the item, and says what was done.
fn stage_typed(
    tx: &mut Tx,
    env: &Env<'_>,
    (id, target): (ItemId, &Target),
    (base, draft): (&[u8], &[u8]),
) -> Result<()> {
    let Some(old) = tx.line(id).cloned() else {
        return Err(no_item(target));
    };
    let handle = tx.handle(id);
    if old.raw() != base {
        return Err(Error::new(
            Code::EditConflict,
            format!("{handle} changed while it was being edited"),
        ));
    }
    if is_empty(draft) {
        return Err(empty_task(tx, id));
    }
    let typed = trim_end(draft);
    let (text, mut warns) = soft_tags(typed, &old, env.clock.today());
    let text = tx.without_claim(&env.actor, id, text)?;
    if is_empty(&text) {
        return Err(empty_task(tx, id));
    }
    warns.extend(repeated_keys(&handle, &old, &text));
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

/// Whether the text has no description.
fn is_empty(text: &[u8]) -> bool {
    trim(Line::parse(text).description()).is_empty()
}

fn empty_task(tx: &Tx, id: ItemId) -> Error {
    let snap = tx.snapshot();
    match tx.item(id) {
        Some(item) => wrong_state(
            snap,
            item,
            item.state(),
            "empty task".to_owned(),
            "use dd to delete".to_owned(),
        ),
        None => Error::new(Code::WrongState, "empty task")
            .with_hint("use dd to delete"),
    }
}

/// `text` without the spaces and tabs at its end.
fn trim_end(text: &[u8]) -> &[u8] {
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

/// Whether the token is a `due:` or `t:` tag.
fn is_date_tag(tok: &TokenView<'_>) -> bool {
    is_key(tok, b"due") || is_key(tok, b"t")
}

/// Why a typed tag is only warned about, if it is.
fn warning_of(tok: &TokenView<'_>, today: Date) -> Option<Vec<u8>> {
    let value = String::from_utf8_lossy(tok.value);
    let bad = if is_date_tag(tok) {
        dates::resolve(&value, today)
            .is_err()
            .then_some(" is not a date")
    } else if is_key(tok, b"rec") && Line::parse(tok.text).tags().rec.is_none()
    {
        Some(" is not valid")
    } else {
        None
    };
    bad.map(|why| [tok.text, why.as_bytes()].concat())
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
        if let Some(warning) = warning_of(&tok, today) {
            warns.push(warning);
            continue;
        }
        if !is_date_tag(&tok) {
            continue;
        }
        let value = String::from_utf8_lossy(tok.value);
        let Ok(Resolved::On(date)) = dates::resolve(&value, today) else {
            continue;
        };
        out.extend_from_slice(text.get(last..tok.start).unwrap_or_default());
        out.extend_from_slice(tok.key);
        out.extend_from_slice(format!(":{date}").as_bytes());
        last = tok.start + tok.text.len();
    }
    out.extend_from_slice(text.get(last..).unwrap_or_default());
    (out, warns)
}

/// How many tags with `key` the line has.
fn count_key(line: &Line, key: &[u8]) -> usize {
    line.tokens().filter(|t| is_key(t, key)).count()
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
            let first = typed.tokens().find(|t| is_key(t, key))?;
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
