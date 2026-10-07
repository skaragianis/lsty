//! `edit_pinned`: the line a person typed, written over the line it was
//! typed on wherever that line is now. A line that changed meanwhile is
//! merged with the draft, and a line that is gone comes back as a copy.

use super::claim::{ClaimMode, Guard};
use super::env::Env;
use super::grant::{Grant, grant_ids};
use super::idrules::At;
use super::lifecycle::ClaimOpts;
use super::merge::{Clash, StampWord, merge};
use super::newline::{copy_error, invalid_text};
use super::pin::{Pin, Spot, Tracked};
use super::refs::{Target, no_item};
use super::result::Outcome;
use super::run::run;
use super::spawn::spawn;
use super::staged::{LineRef, Path};
use super::tx::{Picked, Tx};
use super::typed::{soft_tags, stage_typed, trim_end};
use crate::errs::{Code, Error, Nr, Result, Source};
use crate::list::ItemId;
use crate::todotxt::Line;

/// What writing a draft came to.
#[derive(Debug)]
pub enum Edited {
    /// The draft was written.
    Written(Box<Outcome>),
    /// Both sides changed a part: nothing was written.
    Conflict(Conflict),
}

/// A draft that clashes with the line on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    merged: Vec<u8>,
    disk: Vec<u8>,
    clashes: Vec<Clash>,
}

impl Conflict {
    /// The line with the draft's version of each clashing part.
    pub fn merged(&self) -> &[u8] {
        &self.merged
    }

    /// The line as it is on disk.
    pub fn disk(&self) -> &[u8] {
        &self.disk
    }

    /// The refusal that names what the other side changed.
    pub fn error(&self) -> Error {
        conflict_error(&self.clashes)
    }
}

/// Writes `draft` over the line `tracked` follows, which was `base` when
/// the draft was started. A line that reads otherwise now is merged with
/// the draft; a part both sides changed leaves everything as it is. A
/// line that is gone comes back as a copy of the draft. Each grant gives
/// its id to the item it names, in the same write.
///
/// # Errors
///
/// `invalid_value` for a line break, a draft that cannot be copied and
/// an empty copy, `wrong_state` for an empty item and a text that
/// changes a stamp, and what the rules refuse: a second `id:`, a taken
/// id, a typed `started:` on a blocked item and a claim of another.
pub fn edit_pinned(
    env: &Env<'_>,
    tracked: &Tracked,
    draft: &[u8],
    grants: &[Grant],
) -> Result<Edited> {
    if draft.iter().any(|b| matches!(b, b'\r' | b'\n')) {
        return Err(invalid_text("a task is one line", ""));
    }
    let soft = Env {
        actor: env.actor.clone(),
        cancel: env.cancel.clone(),
        cycle_warn_only: true,
        ..*env
    };
    let mut clash = None;
    let result = run(&soft, "edit", false, |tx| {
        let pin = tracked.follow(&tx.snapshot().todo().lines);
        pinned_edit(tx, &soft, &pin, draft, &mut clash)?;
        grant_ids(tx, &soft, grants)
    });
    match (result, clash) {
        (Err(_), Some(conflict)) => Ok(Edited::Conflict(conflict)),
        (result, _) => result.map(|o| Edited::Written(Box::new(o))),
    }
}

fn pinned_edit(
    tx: &mut Tx,
    env: &Env<'_>,
    pin: &Pin,
    draft: &[u8],
    clash: &mut Option<Conflict>,
) -> Result<()> {
    let at = match pin.spot() {
        Spot::Live(at) => at,
        Spot::Blank(at) => {
            return copy_draft(tx, env, pin, (Gone::Blank(at), draft));
        }
        Spot::After(after) => {
            return copy_draft(tx, env, pin, (Gone::After(after), draft));
        }
    };
    let target = Target::todo(i64::try_from(at + 1).unwrap_or(i64::MAX));
    let Picked::Item(id) = tx.target(&target)? else {
        return Err(no_item(&target));
    };
    tx.mark_primary(id);
    let guard = Guard {
        actor: &env.actor,
        mode: ClaimMode::Guarded,
        opts: ClaimOpts::default(),
    };
    tx.guarded(guard, id, |tx| {
        merge_typed(tx, env, (id, &target), (pin.base(), draft), clash)
    })
}

/// Writes `draft` over the item, merged with what the item reads now
/// when that is not the base.
fn merge_typed(
    tx: &mut Tx,
    env: &Env<'_>,
    (id, target): (ItemId, &Target),
    (base, draft): (&[u8], &[u8]),
    clash: &mut Option<Conflict>,
) -> Result<()> {
    let Some(disk) = tx.line(id).cloned() else {
        return Err(no_item(target));
    };
    if disk.raw() == base {
        return stage_typed(tx, env, (id, target), (base, draft));
    }
    let merged = merge(base, draft, disk.raw())
        .map_err(|StampWord| tx.stamp_word_refusal(id))?;
    if !merged.clashes.is_empty() {
        let error = conflict_error(&merged.clashes);
        *clash = Some(Conflict {
            merged: merged.line,
            disk: disk.raw().to_vec(),
            clashes: merged.clashes,
        });
        return Err(error);
    }
    stage_typed(tx, env, (id, target), (disk.raw(), &merged.line))?;
    say_life_change(tx, base, &disk);
    Ok(())
}

fn conflict_error(clashes: &[Clash]) -> Error {
    let parts: Vec<String> = clashes
        .iter()
        .map(|c| format!("{} ({})", c.part.name(), c.disk))
        .collect();
    Error::new(
        Code::EditConflict,
        format!("changed on disk meanwhile: {}", parts.join(", ")),
    )
    .with_hint("Enter writes your version")
}

/// Says when the item was completed or reopened since the base.
fn say_life_change(tx: &mut Tx, base: &[u8], disk: &Line) {
    let was = Line::parse(base).is_done();
    if disk.is_done() && !was {
        tx.say("the item was completed meanwhile");
    } else if !disk.is_done() && was {
        tx.say("the item was reopened meanwhile");
    }
}

/// Where the line of a draft was, when the item is gone.
#[derive(Clone, Copy)]
enum Gone {
    /// The blank line the item left.
    Blank(usize),
    /// The place just after this line, or the top.
    After(Option<usize>),
}

/// Adds the draft as a new item where its line was.
fn copy_draft(
    tx: &mut Tx,
    env: &Env<'_>,
    pin: &Pin,
    (gone, draft): (Gone, &[u8]),
) -> Result<()> {
    let typed = Line::parse(trim_end(draft));
    let line = spawn(env, &typed).ok_or_else(|| copy_error(draft))?;
    if line.description().is_empty() {
        return Err(invalid_text("empty task", "use dd to delete"));
    }
    let held = Line::parse(pin.base());
    let (raw, warns) = soft_tags(line.raw(), &held, env.clock.today());
    let at = put_copy(tx, gone, raw.clone());
    tx.primary(&[at]);
    let nr = tx
        .row_nr(At {
            source: at.source,
            index: at.index,
        })
        .map_or(0, Nr::get);
    tx.say([nr.to_string().as_bytes(), b" ", &raw].concat());
    for warning in warns {
        tx.say(warning);
    }
    let how = if pin.archived(&tx.snapshot().done().lines) {
        "archived"
    } else {
        "deleted"
    };
    tx.say(format!(
        "the item was {how} meanwhile — your edit was added as a new item"
    ));
    Ok(())
}

/// Puts `raw` into the blank line the item left, or after the line the
/// pin follows.
fn put_copy(tx: &mut Tx, gone: Gone, raw: Vec<u8>) -> LineRef {
    match gone {
        Gone::Blank(at) => {
            if let Some(row) = tx.todo.row_mut(at) {
                row.raw = raw.into();
                row.path = row.path.max(Path::Text);
            }
            tx.todo.line_ref(at)
        }
        Gone::After(Some(at)) => {
            tx.insert(Source::Todo, at + 1, raw, Path::Text)
        }
        Gone::After(None) => tx.insert(Source::Todo, 0, raw, Path::Text),
    }
}
