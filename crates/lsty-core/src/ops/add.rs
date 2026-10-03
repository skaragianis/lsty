//! `add`: create items at the end, at the top, or next to another item.

use super::env::Env;
use super::ids::Taken;
use super::ifabsent::{Found, How, Placed};
use super::newline::{NEEDS_TEXT, clean_text, invalid_text};
use super::pri::parse_letter;
use super::refs::{Target, invalid_id, no_item};
use super::result::Outcome;
use super::rules::{blocked_error, check_rec, expand_first};
use super::run::run;
use super::staged::{LineRef, Path};
use super::tx::{Picked, Tx};
use crate::errs::{Code, Error, Field, Result, Source};
use crate::list::{Item, Snapshot};
use crate::todotxt::{Id, Kind, Line, Parts, Priority, build};

/// Where new lines go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Place {
    /// After the last line of the file.
    #[default]
    End,
    /// Before line 1.
    Top,
    /// Directly below the item.
    After(Target),
    /// Directly above the item.
    Before(Target),
}

/// Which id the new items get.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum IdChoice {
    /// An agent's item gets a random id, the human's gets none.
    #[default]
    Implied,
    /// Every item gets its own random id.
    Random,
    /// The one item gets this id.
    Chosen(String),
}

/// The items to add.
#[derive(Clone, Debug, Default)]
pub struct AddParams {
    /// One text per new item, in order.
    pub texts: Vec<String>,
    /// Where the lines go.
    pub place: Place,
    /// The priority letter given with `--pri`.
    pub pri: Option<String>,
    /// Whether to write the creation date; `None` follows the
    /// configuration.
    pub date: Option<bool>,
    /// Which ids to give.
    pub id: IdChoice,
    /// Skip a text that an open item of the todo file already matches.
    pub if_absent: bool,
}

/// What a new line needs after its text is composed.
#[derive(Debug)]
struct NewLine {
    raw: Vec<u8>,
    /// The id the text carries.
    typed: Option<Vec<u8>>,
    /// The id to append.
    wanted: Wanted,
}

impl NewLine {
    /// The id that names the item: the one in the text, or the one
    /// chosen.
    fn key_id(&self) -> Option<Vec<u8>> {
        match (&self.typed, &self.wanted) {
            (Some(typed), _) => Some(typed.clone()),
            (None, Wanted::Chosen(id)) => Some(id.as_bytes().to_vec()),
            (None, Wanted::Nothing | Wanted::Random) => None,
        }
    }
}

#[derive(Debug)]
pub(super) enum Wanted {
    Nothing,
    Chosen(String),
    Random,
}

/// The priorities that can apply to a text.
struct Priorities {
    given: Option<Priority>,
    fallback: Option<Priority>,
}

/// Writes one new item per text, as one block, in one write.
///
/// # Errors
///
/// A usage or `invalid_value` error for a text or option that is not
/// accepted, `no_item` or `wrong_state` for the anchor, and whatever the
/// rules of the written lines refuse. Nothing is written then.
pub fn add(env: &Env<'_>, params: &AddParams) -> Result<Outcome> {
    let lines = compose_all(env, params)?;
    run(env, "add", false, |tx| stage(tx, env, params, &lines))
}

fn compose_all(env: &Env<'_>, p: &AddParams) -> Result<Vec<NewLine>> {
    if p.texts.is_empty() {
        return Err(Error::new(Code::Usage, NEEDS_TEXT));
    }
    check_chosen(p)?;
    let pri = Priorities {
        given: p.pri.as_deref().map(parse_letter).transpose()?,
        fallback: env.config.default_priority,
    };
    p.texts.iter().map(|t| compose(env, p, &pri, t)).collect()
}

fn check_chosen(p: &AddParams) -> Result<()> {
    let IdChoice::Chosen(chosen) = &p.id else {
        return Ok(());
    };
    if p.texts.len() > 1 {
        return Err(Error::new(
            Code::Usage,
            "several items can't share an id",
        )
        .with_hint("write id:ID in each line"));
    }
    if !Id::is_valid(chosen) {
        return Err(invalid_id(chosen));
    }
    Ok(())
}

fn compose(
    env: &Env<'_>,
    p: &AddParams,
    pri: &Priorities,
    raw: &str,
) -> Result<NewLine> {
    let text = clean_text(raw)?;
    let (own, body) = own_priority(text);
    if let (Some(own), Some(given)) = (own, pri.given)
        && own != given
    {
        return Err(Error::new(
            Code::Usage,
            format!("the text has priority ({own}) already"),
        ));
    }
    let today = env.clock.today();
    let created = writes_date(env, p).then(|| today.to_string());
    if created.is_none() && backdated(body) {
        return Err(invalid_text(
            "a new task can't set its own creation date",
            r#"write "Dentist due:2026-01-01""#,
        ));
    }
    let body = expand_first(body, today)?;
    let raw = build(&Parts {
        priority: own.or(pri.given).or(pri.fallback),
        created: created.as_deref(),
        description: &body,
        ..Parts::default()
    });
    finish(env, p, raw)
}

fn writes_date(env: &Env<'_>, p: &AddParams) -> bool {
    p.date.unwrap_or(env.config.include_creation)
}

/// The priority a text starts with, and the rest of it.
pub(super) fn own_priority(text: &str) -> (Option<Priority>, &str) {
    let b = text.as_bytes();
    if let [b'(', letter, b')', b' ', ..] = b
        && let Some(own) = Priority::new(letter.to_ascii_uppercase())
        && let Some(rest) = text.get(4..)
    {
        return (Some(own), rest);
    }
    (None, text)
}

/// Whether the first word is a date that would become the creation date.
pub(super) fn backdated(body: &str) -> bool {
    let trimmed = body.trim_start_matches([' ', '\t']);
    Line::parse(trimmed.as_bytes()).created().is_some()
}

/// Checks the tags of the composed line and decides which id it gets.
fn finish(env: &Env<'_>, p: &AddParams, raw: Vec<u8>) -> Result<NewLine> {
    let line = Line::parse(raw);
    check_rec(&line)?;
    let agent = !env.actor.is_human();
    if agent {
        refuse_claim_tags(&line)?;
    }
    let typed = line
        .tags()
        .id
        .filter(|id| !id.is_empty())
        .map(<[u8]>::to_vec);
    if let (Some(typed), IdChoice::Chosen(chosen)) = (&typed, &p.id)
        && typed.as_slice() != chosen.as_bytes()
    {
        return Err(mismatch(typed, chosen));
    }
    let wanted = wanted_id(typed.is_some(), &p.id, agent);
    Ok(NewLine {
        raw: line.raw().to_vec(),
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
        IdChoice::Implied => Wanted::Nothing,
    }
}

/// An id typed in the text that is not the one chosen.
fn mismatch(typed: &[u8], chosen: &str) -> Error {
    let typed = String::from_utf8_lossy(typed);
    Error::new(
        Code::Usage,
        format!("id:{typed} in the text and --id={chosen}"),
    )
}

fn refuse_claim_tags(line: &Line) -> Result<()> {
    let claim = line.tokens().find(|t| {
        t.kind == Kind::Tag && (t.key == b"started" || t.key == b"owner")
    });
    let Some(tok) = claim else {
        return Ok(());
    };
    let key = String::from_utf8_lossy(tok.key);
    Err(Error::new(
        Code::WrongState,
        format!("an agent can't write {key}: in a new task"),
    )
    .with_hint("use lsty start")
    .with_field("state", Field::Str("todo".to_owned())))
}

fn stage(
    tx: &mut Tx,
    env: &Env<'_>,
    p: &AddParams,
    lines: &[NewLine],
) -> Result<()> {
    let after = insert_point(tx, p)?;
    let staged = tx.staged_ids();
    let typed = lines.iter().filter_map(|l| l.typed.as_deref());
    let held = staged.iter().map(Vec::as_slice).chain(typed);
    let mut taken = Taken::new(tx.snapshot(), held);
    let at = Stage { env, p, after };
    let mut placed = Vec::with_capacity(lines.len());
    for line in lines {
        placed.push(place(tx, &at, line, &mut taken)?);
    }
    let refs: Vec<LineRef> = placed.iter().map(|p| p.line).collect();
    tx.primary(&refs);
    announce(tx, env, &placed)
}

/// What staging a text needs to know.
struct Stage<'a, 'e> {
    env: &'a Env<'e>,
    p: &'a AddParams,
    /// The line the new lines go after; 0 is the top.
    after: usize,
}

/// Stages one text: as a new line, or, with `if_absent`, as a match of
/// an open item.
fn place(
    tx: &mut Tx,
    at: &Stage<'_, '_>,
    line: &NewLine,
    taken: &mut Taken,
) -> Result<Placed> {
    let Stage { env, p, after } = *at;
    if p.if_absent {
        let key = line.key_id();
        let new = Line::parse(line.raw.as_slice());
        if let Some(index) = tx.find_open(&new, key.as_deref()) {
            let found = Found {
                index,
                key: key.as_deref(),
                random: matches!(line.wanted, Wanted::Random),
            };
            return tx.settle(env, &found, taken);
        }
    }
    let raw = with_id(env, line, taken)?;
    let line = tx.insert(Source::Todo, after, raw, Path::Action);
    Ok(Placed {
        line,
        how: How::Added,
    })
}

/// The line, with the id it is to get appended.
fn with_id(
    env: &Env<'_>,
    line: &NewLine,
    taken: &mut Taken,
) -> Result<Vec<u8>> {
    let id = match &line.wanted {
        Wanted::Nothing => return Ok(line.raw.clone()),
        Wanted::Chosen(id) => id.clone(),
        Wanted::Random => taken.draw(env.ids)?,
    };
    append_id(&line.raw, id.as_bytes())
}

/// `raw` with the token `id:ID` appended.
pub(super) fn append_id(raw: &[u8], id: &[u8]) -> Result<Vec<u8>> {
    let tag = [b"id:".as_slice(), id].concat();
    match Line::parse(raw).append_token(&tag) {
        Ok(with) => Ok(with.raw().to_vec()),
        Err(_) => Err(word_id_error(raw, id)),
    }
}

fn word_id_error(raw: &[u8], id: &[u8]) -> Error {
    let raw = String::from_utf8_lossy(raw);
    let id = String::from_utf8_lossy(id);
    Error::new(Code::InvalidValue, format!("can't give {raw} the id:{id}"))
        .with_hint("the text is only a prefix-like word; reword it first")
        .with_field("field", Field::Str("id".to_owned()))
        .with_field("value", Field::Str(id.into_owned()))
}

/// The number of the line the new lines go after; 0 is the top.
fn insert_point(tx: &mut Tx, p: &AddParams) -> Result<usize> {
    match &p.place {
        Place::Top => Ok(0),
        Place::End => Ok(tx.snapshot().todo().lines.len()),
        Place::After(anchor) => anchor_index(tx, anchor).map(|i| i + 1),
        Place::Before(anchor) => anchor_index(tx, anchor),
    }
}

/// The index of the anchor's line.
fn anchor_index(tx: &mut Tx, anchor: &Target) -> Result<usize> {
    let anchor = anchor.clone().current_only();
    let Picked::Item(id) = tx.target(&anchor)? else {
        return Err(no_item(&anchor));
    };
    let item = tx.item(id).ok_or_else(|| no_item(&anchor))?;
    Ok(item.nr().index())
}

/// Says what was added or found, and refuses an item that starts while
/// it waits.
fn announce(tx: &mut Tx, env: &Env<'_>, placed: &[Placed]) -> Result<()> {
    let after = tx.snapshot_after(env);
    for one in placed {
        let ids = tx.items_after(&after, &[one.line]);
        let Some(item) = ids.first().and_then(|id| after.item(*id)) else {
            continue;
        };
        if one.how == How::Added && after.tags(item).underway && item.blocked()
        {
            return Err(blocked_start(&after, item));
        }
        tx.say(message(one.how, &after, item));
    }
    Ok(())
}

fn message(how: How, after: &Snapshot, item: &Item) -> Vec<u8> {
    let nr = item.nr();
    let text = after.line(item).text();
    let id = after.id_of(item).filter(|id| !id.is_empty());
    let head = match (how, id) {
        (How::Added, None) => format!("{nr} added"),
        (How::Added, Some(_)) => format!("{nr} added as id:"),
        (How::Found, None) => format!("{nr} already there"),
        (How::Found, Some(_)) => format!("{nr} already there as id:"),
        (How::Named, _) => format!("{nr} already there, now id:"),
    };
    [head.as_bytes(), id.unwrap_or_default(), b": ", &text].concat()
}

fn blocked_start(after: &Snapshot, item: &Item) -> Error {
    blocked_error(
        after,
        item,
        "a new task can't start while it waits for another",
        "write it without started:",
    )
}
