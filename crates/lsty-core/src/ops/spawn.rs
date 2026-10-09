//! The next occurrence of a recurring item, and the copy of a line it is
//! built from.

use super::env::Env;
use super::lifecycle::is_pri_tag;
use super::reopen::restored_priority;
use super::staged::{LineRef, Path};
use super::tokens::{cut_where, first_of_key};
use super::tx::Tx;
use crate::dates::Date;
use crate::errs::{Code, Error, Field, Result, Source};
use crate::list::ItemId;
use crate::todotxt::{Line, Priority, Rec, TokenView};

/// A fresh open copy of `line`: today's creation date when the
/// configuration asks for one, the priority (restored from `pri:` for a
/// done line), and every token but `id:`, `started:` and `owner:` as it
/// was. Dates are left alone. `None` when removing a token would turn a
/// word into a stamp.
pub(crate) fn spawn(env: &Env<'_>, line: &Line) -> Option<Line> {
    let letter = if line.is_done() {
        restored_priority(line)
    } else {
        line.priority().map(Priority::letter)
    };
    let mut head = Vec::new();
    if let Some(letter) = letter {
        head.extend_from_slice(&[b'(', letter, b')', b' ']);
    }
    if env.config.include_creation {
        head.extend_from_slice(env.clock.today().to_string().as_bytes());
        head.push(b' ');
    }
    head.extend_from_slice(line.description());
    let mut out = Line::parse(head);
    if line.is_done() {
        out = cut_where(&out, is_pri_tag)?.0;
    }
    cut_where(&out, is_structure_tag).map(|(line, _)| line)
}

fn is_structure_tag(tok: &TokenView<'_>) -> bool {
    tok.is_tag(b"id") || tok.is_tag(b"started") || tok.is_tag(b"owner")
}

/// A spawn whose message waits for its final line number.
pub(super) struct Pending {
    at: usize,
    line: LineRef,
    next: Line,
}

/// Makes the next occurrence of the item `id`, which `do` is completing,
/// and stages it: directly below the item, or at the end of the todo file
/// for an item of the done file. An item without a valid `rec:` says why
/// it makes none.
pub(super) fn spawn_next(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    pending: &mut Vec<Pending>,
) -> Result<()> {
    let (Some(item), Some(line)) = (tx.item(id), tx.line(id)) else {
        return Ok(());
    };
    let Some(rec) = line.tags().rec else {
        if first_of_key(line, b"rec").is_some() {
            let handle = tx.handle(id);
            tx.say(format!(
                "{handle} has a bad rec: value; no next occurrence"
            ));
        }
        return Ok(());
    };
    let (source, after) = (item.source(), item.nr().index() + 1);
    let origin = tx.snapshot().ref_of(item);
    let copy = spawn(env, line).ok_or_else(|| tx.stamp_word_refusal(id))?;
    let next = advanced(&copy, rec, env.clock.today())
        .map_err(|e| e.with_item(origin))?;
    let raw = next.raw();
    let at = match source {
        Source::Done => tx.append(Source::Todo, raw, Path::Action),
        Source::Todo => tx.insert(source, after, raw, Path::Action),
    };
    tx.spawn(&[at]);
    pending.push(Pending {
        at: tx.messages.len(),
        line: at,
        next,
    });
    tx.say(Vec::new());
    Ok(())
}

/// Writes the messages of the spawns, now that their line numbers are
/// final.
pub(super) fn say_nexts(tx: &mut Tx, pending: Vec<Pending>) {
    for Pending { at, line, next } in pending {
        let Some(nr) = tx.row_nr(line) else {
            continue;
        };
        let tags = next.tags();
        let note = match (tags.due, tags.threshold) {
            (Some(due), _) => format!("due {due}"),
            (None, threshold) => {
                format!("from {}", threshold.unwrap_or_default())
            }
        };
        let message = [
            nr.to_string().as_bytes(),
            b" next: ",
            &next.text(),
            b" (",
            note.as_bytes(),
            b")",
        ]
        .concat();
        if let Some(slot) = tx.messages.get_mut(at) {
            *slot = message;
        }
    }
}

/// `line` with `due:` and `t:` moved on by `rec`. The base is today, or
/// for a strict recurrence the old date; the lead time of `t:` is kept
/// in days.
fn advanced(line: &Line, rec: Rec, today: Date) -> Result<Line> {
    let tags = line.tags();
    let due = tags.due.and_then(Date::parse);
    let thr = tags.threshold.and_then(Date::parse);
    let step = |old: Date| -> Result<Date> {
        let base = if rec.strict() { old } else { today };
        moved(base, rec)
    };
    if let Some(due) = due {
        let new = step(due)?;
        let line = with_date(line, b"due", new);
        let Some(thr) = thr else {
            return Ok(line);
        };
        let lead = thr.add(due.days_until(new), crate::todotxt::RecUnit::Day);
        return Ok(with_date(&line, b"t", lead.ok_or_else(|| range(rec))?));
    }
    if let Some(thr) = thr {
        return Ok(with_date(line, b"t", step(thr)?));
    }
    Ok(with_date(line, b"due", moved(today, rec)?))
}

fn moved(base: Date, rec: Rec) -> Result<Date> {
    base.add(i32::from(rec.n()), rec.unit())
        .ok_or_else(|| range(rec))
}

fn range(rec: Rec) -> Error {
    Error::new(
        Code::InvalidValue,
        "the next occurrence is outside the years 0000 to 9999",
    )
    .with_field("field", Field::Str("rec".to_owned()))
    .with_field("value", Field::Str(rec_text(rec)))
}

fn rec_text(rec: Rec) -> String {
    let plus = if rec.strict() { "+" } else { "" };
    format!("{plus}{}{}", rec.n(), char::from(rec.unit().letter()))
}

/// `line` with the first `key:` written as `date`, over a bad value if
/// need be, or else appended.
fn with_date(line: &Line, key: &[u8], date: Date) -> Line {
    let text = [key, b":", date.to_string().as_bytes()].concat();
    let replaced =
        first_of_key(line, key).and_then(|at| line.replace_token(at, &text));
    if let Some(done) = replaced {
        return done;
    }
    line.append_token(&text).unwrap_or_else(|_| line.clone())
}
