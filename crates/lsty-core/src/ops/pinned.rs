//! `edit_pinned`: the line a person typed, written over the line it was
//! typed on wherever that line is now. A line that changed meanwhile is
//! merged with the draft, and a line that is gone comes back as a copy.

use super::{
    claim::{ClaimMode, Guard},
    env::Env,
    grant::{Grant, grant_ids},
    lifecycle::ClaimOpts,
    merge::{Clash, StampWord, merge},
    newline::{copy_error, invalid_text},
    pin::{Pin, Spot, Tracked},
    refs::Target,
    result::Outcome,
    run::{Rules, run_with},
    spawn::spawn,
    staged::{LineRef, Path},
    tx::Tx,
    typed::{soft_tags, stage_typed, trim_end},
};
use crate::{
    errs::{Code, Error, Nr, Result, Source},
    list::ItemId,
    todotxt::Line,
};

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
    let mut clash = None;
    let result = run_with(env, "edit", false, Rules::WarnCycles, |tx| {
        let pin = tracked.follow(&tx.snapshot().todo().lines);
        pinned_edit(tx, env, &pin, draft, &mut clash)?;
        grant_ids(tx, env, grants)
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
    let Spot::Live(at) = pin.spot() else {
        return copy_draft(tx, env, pin, draft);
    };
    let target = Target::todo_index(at);
    let id = tx.anchor(&target)?;
    tx.mark_primary(id);
    let guard = Guard {
        actor: &env.actor,
        mode: ClaimMode::Guarded,
        opts: ClaimOpts::default(),
    };
    tx.guarded(guard, id, |tx| {
        merge_typed(tx, env, id, (pin.base(), draft), clash)
    })
}

/// Writes `draft` over the item, merged with what the item reads now
/// when that is not the base.
fn merge_typed(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    (base, draft): (&[u8], &[u8]),
    clash: &mut Option<Conflict>,
) -> Result<()> {
    let disk = tx.line_of(id)?.clone();
    if disk.raw() == base {
        return stage_typed(tx, env, id, (base, draft));
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
    stage_typed(tx, env, id, (disk.raw(), &merged.line))?;
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
    .with_hint("<CR> writes your version")
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

/// Adds the draft as a new item where its line was.
fn copy_draft(
    tx: &mut Tx,
    env: &Env<'_>,
    pin: &Pin,
    draft: &[u8],
) -> Result<()> {
    let typed = Line::parse(trim_end(draft));
    let line = spawn(env, &typed).ok_or_else(|| copy_error(draft))?;
    if !line.has_text() {
        return Err(invalid_text("empty task", "use dd to delete"));
    }
    let held = Line::parse(pin.base());
    let (raw, warns) = soft_tags(line.raw(), &held, env.clock.today());
    let at = put_copy(tx, pin.spot(), raw.clone());
    tx.primary(&[at]);
    let nr = tx.row_nr(at).map_or(0, Nr::get);
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
fn put_copy(tx: &mut Tx, spot: Spot, raw: Vec<u8>) -> LineRef {
    match spot {
        Spot::Blank(at) => {
            if let Some(row) = tx.todo.row_mut(at) {
                row.raw = raw.into();
                row.path = row.path.max(Path::Text);
            }
            tx.todo.line_ref(at)
        }
        Spot::Live(at) | Spot::After(Some(at)) => {
            tx.insert(Source::Todo, at + 1, raw, Path::Text)
        }
        Spot::After(None) => tx.insert(Source::Todo, 0, raw, Path::Text),
    }
}
