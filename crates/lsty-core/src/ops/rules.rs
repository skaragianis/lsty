//! The rule layer of text edits: dates, `rec:`, claims and the empty
//! item. Every line staged on the text path passes through it.

use std::io::Write as _;

use super::{
    env::{Actor, Env},
    lifecycle::emptied,
    staged::{LineRef, Path, Row},
    tokens::{ENDED_HINT, claim_tokens},
    tx::{Tx, gone, wrong_state},
};
use crate::{
    dates::{self, Date},
    errs::{Code, Error, Field, Result, Source, lossy},
    list::{Item, ItemId, Snapshot},
    todotxt::{Kind, Line, Rec, TokenView, trim},
};

/// The keys a text may set once; a repeat of one of them is warned about.
pub(super) const SETTABLE_KEYS: [&[u8]; 4] = [b"due", b"t", b"rec", b"pri"];

/// The text of a typed edit: trimmed, one line, with every fuzzy `due:`
/// and `t:` written as a date and the first `rec:` checked.
pub(super) fn edit_text(
    raw: &str,
    action: &str,
    today: Date,
) -> Result<String> {
    let text = edit_line(raw.as_bytes(), action, today)?;
    Ok(lossy(&text))
}

/// [`edit_text`] for bytes, which keeps bytes that are not UTF-8.
pub(super) fn edit_line(
    raw: &[u8],
    action: &str,
    today: Date,
) -> Result<Vec<u8>> {
    let text = trim(raw);
    if text.is_empty() {
        return Err(Error::new(Code::Usage, format!("{action} needs text")));
    }
    if text.iter().any(|b| matches!(b, b'\r' | b'\n')) {
        return Err(super::newline::invalid_text("a task is one line", ""));
    }
    resolve_line(text, today)
}

/// `text` with every `due:` and `t:` written as an ISO date and its first
/// `rec:` checked.
pub(super) fn resolve_line(text: &[u8], today: Date) -> Result<Vec<u8>> {
    let expanded = expand(text, today, true)?;
    check_rec(&Line::parse(&expanded))?;
    Ok(expanded)
}

/// `body` with the first `due:` and the first `t:` written as ISO dates.
pub(super) fn expand_first(body: &str, today: Date) -> Result<Vec<u8>> {
    expand(body.as_bytes(), today, false)
}

fn expand(body: &[u8], today: Date, all: bool) -> Result<Vec<u8>> {
    let line = Line::parse(body);
    let mut seen = [false; 2];
    let mut out = Vec::with_capacity(body.len());
    let mut last = 0;
    for tok in line.tokens().filter(|t| t.kind == Kind::Tag) {
        let Some(slot) = date_slot(tok.key) else {
            continue;
        };
        if std::mem::replace(&mut seen[slot], true) && !all {
            continue;
        }
        let Some(date) = resolve_value(&tok, today)? else {
            continue;
        };
        out.extend_from_slice(body.get(last..tok.start).unwrap_or_default());
        out.extend_from_slice(tok.key);
        write!(out, ":{date}").ok();
        last = tok.start + tok.text.len();
    }
    out.extend_from_slice(body.get(last..).unwrap_or_default());
    Ok(out)
}

const fn date_slot(key: &[u8]) -> Option<usize> {
    match key {
        b"due" => Some(0),
        b"t" => Some(1),
        _ => None,
    }
}

fn resolve_value(tok: &TokenView<'_>, today: Date) -> Result<Option<Date>> {
    let value = lossy(tok.value);
    dates::resolve(&value, today).map_err(|e| {
        let key = lossy(tok.key);
        e.with_field("field", Field::Str(key))
    })
}

/// Refuses a first `rec:` token that is not a recurrence.
pub(super) fn check_rec(line: &Line) -> Result<()> {
    let Some(tok) = line.tokens().find(|t| t.is_tag(b"rec")) else {
        return Ok(());
    };
    if Rec::parse(tok.value).is_some() {
        return Ok(());
    }
    let value = lossy(tok.value);
    let message = format!("not a recurrence: {value}");
    Err(
        Error::invalid_value(message, &value, &["3d, 2w, 1m, 1y, 5b, +1m"])
            .with_field("field", Field::Str("rec".to_owned())),
    )
}

/// The warnings for a typed text that repeats a key `old` already has.
pub(super) fn repeat_warnings(
    handle: &str,
    old: &Line,
    typed: &[u8],
) -> Vec<Vec<u8>> {
    let typed = Line::parse(typed);
    SETTABLE_KEYS
        .iter()
        .filter(|key| typed.tokens().any(|t| t.is_tag(key)))
        .filter_map(|key| old.tokens().find(|t| t.is_tag(key)))
        .map(|held| {
            [
                handle.as_bytes(),
                b" already has ",
                held.text,
                b"; the first one counts",
            ]
            .concat()
        })
        .collect()
}

/// The `blocked` error for `item`, as it is in `after`.
pub(super) fn blocked_error(
    after: &Snapshot,
    item: &Item,
    message: &str,
    hint: &str,
) -> Error {
    let blockers = item
        .blockers()
        .iter()
        .filter_map(|id| after.item(*id))
        .map(|b| after.ref_of(b))
        .collect();
    Error::new(Code::Blocked, message)
        .with_hint(hint)
        .with_field("blockers", Field::Refs(blockers))
}

impl Tx {
    /// Stages `raw` as the new text of an item on the text path. A human
    /// who ends the work also drops the owners; an edit that empties the
    /// item is refused. Returns the line as staged.
    pub(super) fn set_edited(
        &mut self,
        actor: &Actor,
        id: ItemId,
        raw: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let old = self.line_of(id)?;
        let raw = self.without_claim(actor, id, old, raw)?;
        if raw != old.raw() && !Line::parse(&raw).has_text() {
            return Err(emptied(self, id));
        }
        self.set(id, &raw, Path::Text);
        Ok(raw)
    }

    /// `raw` without its owners, when a human ends the work on `id`.
    pub(super) fn without_claim(
        &self,
        actor: &Actor,
        id: ItemId,
        old: &Line,
        raw: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let cur = Line::parse(&raw);
        if !actor.is_human() || !old.tags().underway || cur.tags().underway {
            return Ok(raw);
        }
        let (out, _) = self.remove_where(id, &cur, |t| t.is_tag(b"owner"))?;
        Ok(out.raw().to_vec())
    }

    /// Checks the claim tokens of every edited line against the actor's
    /// rights.
    pub(super) fn check_claims(&mut self, env: &Env<'_>) -> Result<()> {
        let mut started = self.claim_file(&env.actor, Source::Todo)?;
        started.extend(self.claim_file(&env.actor, Source::Done)?);
        self.check_not_blocked(env, &started)
    }

    /// The items of one file that an edit starts.
    fn claim_file(
        &self,
        actor: &Actor,
        source: Source,
    ) -> Result<Vec<(LineRef, ItemId)>> {
        let file = self.file(source);
        let mut started = Vec::new();
        for (index, row) in file.rows().iter().enumerate() {
            if let Some(id) = self.claim_row(actor, row)? {
                started.push((file.line_ref(index), id));
            }
        }
        Ok(started)
    }

    /// Refuses an agent's change to a claim. For the human, gives the
    /// item when the edit starts it.
    fn claim_row(&self, actor: &Actor, row: &Row) -> Result<Option<ItemId>> {
        let Some((id, cur)) = edited(row) else {
            return Ok(None);
        };
        let item = self.snap.item(id).ok_or_else(gone)?;
        let old = self.snap.line(item);
        let (was, now) = (claim_tokens(old), claim_tokens(&cur));
        if was == now {
            return Ok(None);
        }
        if actor.is_human() {
            let starts = !old.tags().underway && cur.tags().underway;
            return Ok(starts.then_some(id));
        }
        let ended = |t: &&Vec<u8>| t.starts_with(b"ended:");
        let hint = if was.iter().filter(ended).ne(now.iter().filter(ended)) {
            ENDED_HINT
        } else if now.len() > was.len() {
            "use lsty start"
        } else {
            "use lsty stop"
        };
        let handle = self.handle(id);
        Err(wrong_state(
            &self.snap,
            item,
            item.state(),
            format!("{handle}: a text edit can't change a claim"),
            hint.to_owned(),
        ))
    }

    /// Refuses a start of an item that waits for another.
    fn check_not_blocked(
        &mut self,
        env: &Env<'_>,
        started: &[(LineRef, ItemId)],
    ) -> Result<()> {
        if started.is_empty() {
            return Ok(());
        }
        let after = self.snapshot_after(env);
        for &(line, id) in started {
            let found = self.items_after(&after, &[line]);
            let Some(item) = found.first().and_then(|i| after.item(*i)) else {
                continue;
            };
            if item.blocked() {
                return Err(blocked_error(
                    &after,
                    item,
                    &format!("{} is blocked", self.handle(id)),
                    "finish what it waits for first",
                ));
            }
        }
        Ok(())
    }

    /// Lets the staged line of `id` lose its `id:`.
    pub(super) fn allow_id_loss(&mut self, id: ItemId) {
        if let Some(row) = self.row_mut(id) {
            row.drops_id = true;
        }
    }
}

/// The edited line of a row and the item it was.
fn edited(row: &Row) -> Option<(ItemId, Line)> {
    if row.path == Path::Action
        || row.removed
        || row.raw == row.orig
        || Line::is_blank_text(&row.raw)
    {
        return None;
    }
    Some((row.item?, Line::parse_shared(&row.raw)))
}
