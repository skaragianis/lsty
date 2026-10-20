//! `add`: create tasks at the end, at the top, or next to another task.

use std::sync::Arc;

use super::{
    dep::wait_for,
    env::{Env, Toggle},
    grant::{Grant, grant_ids},
    ids::Taken,
    ifabsent::{How, Placed},
    newline::{NEEDS_TEXT, clean_text},
    pin::{Spot, Tracked},
    refs::{Target, invalid_id},
    result::Outcome,
    rules::{blocked_error, check_rec, expand_first},
    run::run,
    staged::Path,
    tokens::{ENDED_HINT, is_claim_tag},
    tx::{Tx, gone},
};
use crate::{
    errs::{Code, Error, Field, Result, Source, lossy},
    list::{Snapshot, Task, TaskId},
    todotxt::{Id, Line, Parts, Priority, build},
};

/// Where new lines go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Place {
    /// After the last line of the file.
    #[default]
    End,
    /// Before line 1.
    Top,
    /// Directly below the task.
    After(Target),
    /// Directly above the task.
    Before(Target),
    /// Next to a line followed to where it stands now; once the line is
    /// gone, directly after the nearest earlier line that still is.
    Beside {
        /// The line.
        anchor: Tracked,
        /// Below the line, not above it.
        below: bool,
    },
}

/// Which id the new tasks get.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum IdChoice {
    /// Every new task gets a random id; a task found by `if_absent` gets
    /// one only for an agent.
    #[default]
    Implied,
    /// Every task gets its own random id.
    Random,
    /// The one task gets this id.
    Chosen(String),
}

/// The task a new task is a prerequisite of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocks {
    /// The task a reference names.
    Task(Target),
    /// A line followed to where it stands now.
    Line(Tracked),
}

/// The tasks to add.
#[derive(Clone, Debug, Default)]
pub struct AddParams {
    /// One text per new task, in order.
    pub texts: Vec<String>,
    /// Where the lines go.
    pub place: Place,
    /// The priority given with `--pri`.
    pub pri: Option<Priority>,
    /// Whether to write the creation date.
    pub date: Toggle,
    /// Which ids to give.
    pub id: IdChoice,
    /// Skip a text that an open task of the todo file already matches.
    pub if_absent: bool,
    /// Ids to give to tasks the texts wait for, in the same write.
    pub grants: Vec<Grant>,
    /// The task that waits for the new task, in the same write. The new
    /// task gets an id; with this, only one text and not `if_absent`.
    pub blocks: Option<Blocks>,
}

/// What a new line needs after its text is composed.
#[derive(Debug)]
pub(super) struct NewLine {
    pub(super) line: Line,
    /// The id the text carries.
    typed: Option<Vec<u8>>,
    /// The id to append.
    pub(super) wanted: Wanted,
}

impl NewLine {
    /// The id that names the task: the one in the text, or the one
    /// chosen.
    pub(super) fn key_id(&self) -> Option<&[u8]> {
        match (&self.typed, &self.wanted) {
            (Some(typed), _) => Some(typed),
            (None, Wanted::Chosen(id)) => Some(id.as_bytes()),
            (None, Wanted::Nothing | Wanted::Fresh | Wanted::Random) => None,
        }
    }
}

#[derive(Debug)]
pub(super) enum Wanted {
    Nothing,
    Chosen(String),
    /// A random id for a new line, and none for a line found.
    Fresh,
    /// A random id for a new line, and for a line found without one.
    Random,
}

/// Writes one new task per text, as one block, in one write.
///
/// # Errors
///
/// A usage or `invalid_value` error for a text or option that is not
/// accepted, `no_task` or `wrong_state` for the anchor, and whatever the
/// rules of the written lines refuse. Nothing is written then.
pub fn add(env: &Env<'_>, params: &AddParams) -> Result<Outcome> {
    let lines = compose_all(env, params)?;
    run(env, "add", false, |tx| stage(tx, env, params, &lines))
}

fn compose_all(env: &Env<'_>, p: &AddParams) -> Result<Vec<NewLine>> {
    p.check()?;
    p.texts.iter().map(|t| compose(env, p, t)).collect()
}

impl AddParams {
    /// Refuses what no text can fix: no text at all, or a chosen id that
    /// does not fit the texts. `add` checks this first.
    ///
    /// # Errors
    ///
    /// A usage error for no text or an id given to several texts, and
    /// `invalid_value` for an id that is not one.
    pub fn check(&self) -> Result<()> {
        if self.texts.is_empty() {
            return Err(Error::new(Code::Usage, NEEDS_TEXT));
        }
        if self.blocks.is_some() && (self.texts.len() > 1 || self.if_absent) {
            return Err(Error::new(
                Code::Usage,
                "a task that blocks another is added alone",
            ));
        }
        check_chosen(self)
    }
}

fn check_chosen(p: &AddParams) -> Result<()> {
    let IdChoice::Chosen(chosen) = &p.id else {
        return Ok(());
    };
    if p.texts.len() > 1 {
        return Err(Error::new(
            Code::Usage,
            "several tasks can't share an id",
        )
        .with_hint("write id:ID in each line"));
    }
    if !Id::is_valid(chosen) {
        return Err(invalid_id(chosen));
    }
    Ok(())
}

fn compose(env: &Env<'_>, p: &AddParams, raw: &str) -> Result<NewLine> {
    let text = clean_text(raw)?;
    let (own, body) = own_priority(text);
    if let (Some(own), Some(given)) = (own, p.pri)
        && own != given
    {
        return Err(Error::new(
            Code::Usage,
            format!("the text has priority ({own}) already"),
        ));
    }
    let today = env.clock.today();
    let stamp = p.date.resolve(env.config.include_creation);
    // A date typed in front is the creation date, so none is added.
    let typed = backdated(body);
    let body = typed.unwrap_or(body);
    let created = (stamp && typed.is_none()).then(|| today.to_string());
    let body = expand_first(body, today)?;
    let raw = build(&Parts {
        priority: own.or(p.pri).or(env.config.default_priority),
        created: created.as_deref(),
        description: &body,
        ..Parts::default()
    });
    finish(env, p, raw)
}

/// The priority a text starts with, and the rest of it.
pub(super) fn own_priority(text: &str) -> (Option<Priority>, &str) {
    if let [b'(', letter, b')', b' ', ..] = text.as_bytes()
        && let Some(own) = Priority::new(letter.to_ascii_uppercase())
        && let Some(rest) = text.get(4..)
    {
        return (Some(own), rest);
    }
    (None, text)
}

/// The body without the blanks before it, when its first word is a date,
/// which is then the creation date.
pub(super) fn backdated(body: &str) -> Option<&str> {
    let trimmed = body.trim_start_matches([' ', '\t']);
    let line = Line::parse(trimmed);
    trimmed.starts_with(line.created()?).then_some(trimmed)
}

/// Checks the tags of the composed line and decides which id it gets.
fn finish(env: &Env<'_>, p: &AddParams, raw: Vec<u8>) -> Result<NewLine> {
    let line = Line::parse(raw);
    check_rec(&line)?;
    let agent = !env.actor.is_human();
    if agent {
        refuse_claim_tags(&line)?;
    }
    let typed = line.tags().id.map(<[u8]>::to_vec);
    if let (Some(typed), IdChoice::Chosen(chosen)) = (&typed, &p.id)
        && typed.as_slice() != chosen.as_bytes()
    {
        return Err(mismatch(typed, chosen));
    }
    let wanted =
        wanted_id(typed.is_some(), &p.id, agent || p.blocks.is_some());
    Ok(NewLine {
        line,
        typed,
        wanted,
    })
}

/// Which id to append to a line.
fn wanted_id(typed: bool, choice: &IdChoice, agent: bool) -> Wanted {
    match choice {
        _ if typed => Wanted::Nothing,
        IdChoice::Chosen(chosen) => Wanted::Chosen(chosen.clone()),
        IdChoice::Random => Wanted::Random,
        IdChoice::Implied if agent => Wanted::Random,
        IdChoice::Implied => Wanted::Fresh,
    }
}

/// An id typed in the text that is not the one chosen.
fn mismatch(typed: &[u8], chosen: &str) -> Error {
    let typed = lossy(typed);
    Error::new(
        Code::Usage,
        format!("id:{typed} in the text and --id={chosen}"),
    )
}

fn refuse_claim_tags(line: &Line) -> Result<()> {
    let Some(tok) = line.tokens().find(is_claim_tag) else {
        return Ok(());
    };
    let key = lossy(tok.key);
    Err(Error::new(
        Code::WrongState,
        format!("an agent can't write {key}: in a new task"),
    )
    .with_hint(if tok.is_tag(b"ended") {
        ENDED_HINT
    } else {
        "use lsty start"
    })
    .with_field("state", Field::Str("todo".to_owned())))
}

fn stage(
    tx: &mut Tx,
    env: &Env<'_>,
    p: &AddParams,
    lines: &[NewLine],
) -> Result<()> {
    grant_ids(tx, env, &p.grants)?;
    let waiting = p.blocks.as_ref().map(|b| b.task(tx)).transpose()?;
    let after = insert_point(tx, &p.place)?;
    let typed = lines.iter().filter_map(|l| l.typed.as_deref());
    let held = tx.edited_ids().into_iter().chain(typed.map(<[u8]>::to_vec));
    let mut taken = Taken::new(held);
    let mut placed = Vec::with_capacity(lines.len());
    for line in lines {
        let found = if p.if_absent {
            tx.settle(env, line, &mut taken)?
        } else {
            None
        };
        let one = if let Some(one) = found {
            one
        } else {
            let raw = with_id(tx, env, line, &mut taken)?;
            if let Some(waiting) = waiting {
                let new = Line::parse(raw.as_ref());
                let id = new.tags().id.unwrap_or_default();
                wait_for(tx, env, waiting, id)?;
            }
            Placed {
                line: tx.insert(Source::Todo, after, raw, Path::Action),
                how: How::Added,
            }
        };
        tx.primary.push(one.line);
        placed.push(one);
    }
    announce(tx, env, &placed)?;
    Ok(())
}

impl Blocks {
    /// The task that waits.
    fn task(&self, tx: &mut Tx) -> Result<TaskId> {
        let target = match self {
            Self::Task(target) => target.clone(),
            Self::Line(tracked) => {
                let pin = tracked.follow(&tx.snapshot().todo().lines);
                let Spot::Live(at) = pin.spot() else {
                    return Err(gone());
                };
                Target::todo_index(at)
            }
        };
        tx.anchor(&target)
    }
}

/// The line, with the id it is to get appended.
fn with_id(
    tx: &Tx,
    env: &Env<'_>,
    line: &NewLine,
    taken: &mut Taken,
) -> Result<Arc<[u8]>> {
    match &line.wanted {
        Wanted::Nothing => Ok(line.line.shared()),
        Wanted::Chosen(id) => append_id(&line.line, id.as_bytes()),
        Wanted::Fresh => drawn_id(tx, env, taken, &line.line),
        Wanted::Random => append_id(
            &line.line,
            taken.draw(tx.snapshot(), env.ids)?.as_str().as_bytes(),
        ),
    }
}

/// `line` with a new random id appended, for a line nobody asked an id for;
/// text that cannot take one (only a prefix-like word) stays as it is.
pub(super) fn drawn_id(
    tx: &Tx,
    env: &Env<'_>,
    taken: &mut Taken,
    line: &Line,
) -> Result<Arc<[u8]>> {
    let id = taken.draw(tx.snapshot(), env.ids)?;
    Ok(append_id(line, id.as_str().as_bytes())
        .unwrap_or_else(|_| line.shared()))
}

/// `line` with the token `id:ID` appended.
pub(super) fn append_id(line: &Line, id: &[u8]) -> Result<Arc<[u8]>> {
    let tag = [b"id:".as_slice(), id].concat();
    match line.append_token(&tag) {
        Ok(with) => Ok(with.shared()),
        Err(_) => Err(word_id_error(line.raw(), id)),
    }
}

fn word_id_error(raw: &[u8], id: &[u8]) -> Error {
    let raw = lossy(raw);
    let id = lossy(id);
    Error::new(Code::InvalidValue, format!("can't give {raw} the id:{id}"))
        .with_hint("the text is only a prefix-like word; reword it first")
        .with_field("field", Field::Str("id".to_owned()))
        .with_field("value", Field::Str(id))
}

/// The number of the line the new lines go after; 0 is the top.
pub(super) fn insert_point(tx: &mut Tx, place: &Place) -> Result<usize> {
    match place {
        Place::Top => Ok(0),
        Place::End => Ok(tx.snapshot().todo().lines.len()),
        Place::After(anchor) => anchor_index(tx, anchor).map(|i| i + 1),
        Place::Before(anchor) => anchor_index(tx, anchor),
        Place::Beside { anchor, below } => {
            let pin = anchor.follow(&tx.snapshot().todo().lines);
            Ok(match pin.spot() {
                Spot::Live(at) if !below => at,
                Spot::Live(at) | Spot::Blank(at) => at + 1,
                Spot::After(after) => after.map_or(0, |at| at + 1),
            })
        }
    }
}

/// The index of the anchor's line.
fn anchor_index(tx: &mut Tx, anchor: &Target) -> Result<usize> {
    let id = tx.anchor(anchor)?;
    let task = tx.task(id).ok_or_else(gone)?;
    Ok(task.nr().index())
}

/// Says what was added or found, and refuses a task that starts while
/// it waits.
fn announce(tx: &mut Tx, env: &Env<'_>, placed: &[Placed]) -> Result<()> {
    let after = tx.snapshot_after(env);
    for one in placed {
        let ids = tx.tasks_after(&after, &[one.line]);
        let Some(task) = ids.first().and_then(|id| after.task(*id)) else {
            continue;
        };
        if one.how == How::Added && after.tags(task).underway && task.blocked()
        {
            return Err(blocked_start(&after, task));
        }
        tx.say(message(one.how, &after, task));
    }
    Ok(())
}

fn message(how: How, after: &Snapshot, task: &Task) -> Vec<u8> {
    let nr = task.nr();
    let text = after.line(task).text();
    let id = after.id_of(task);
    let head = match (how, id) {
        (How::Added, None) => format!("{nr} added"),
        (How::Added, Some(_)) => format!("{nr} added as id:"),
        (How::Found, None) => format!("{nr} already there"),
        (How::Found, Some(_)) => format!("{nr} already there as id:"),
        (How::Named, _) => format!("{nr} already there, now id:"),
    };
    [head.as_bytes(), id.unwrap_or_default(), b": ", &text].concat()
}

fn blocked_start(after: &Snapshot, task: &Task) -> Error {
    blocked_error(
        after,
        task,
        "a new task can't start while it waits for another",
        "write it without started:",
    )
}
