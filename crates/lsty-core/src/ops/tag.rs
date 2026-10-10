//! `tag` and `untag`: add and remove projects, contexts and tags.

use super::{
    env::Env,
    refs::Target,
    result::Outcome,
    rules::{SETTABLE_KEYS, check_rec},
    run::run,
    tokens::first_of_key,
    tx::Tx,
};
use crate::{
    dates::{self, Date, Resolved},
    errs::{Code, Error, Field, Result, quote},
    list::ItemId,
    todotxt::{Kind, Line, TokenView},
};

/// A tag as given, owned: the kind of token, its key and its text.
#[derive(Clone, Debug)]
pub(super) struct Tag {
    pub(super) kind: Kind,
    pub(super) key: Vec<u8>,
    pub(super) text: Vec<u8>,
}

/// What applying a tag did to a line.
#[derive(Debug)]
pub(super) enum Applied {
    /// The token was appended.
    Added(Line),
    /// The first token of the key now has the new value; this is the
    /// old token.
    Set(Line, Vec<u8>),
    /// The line has it already.
    Kept,
}

/// Adds each tag to each target. A single-valued key is set in place.
///
/// # Errors
///
/// A usage error for something that is not a tag, `invalid_value` for a
/// bad date or recurrence, and whatever the rules refuse.
pub fn tag(
    env: &Env<'_>,
    targets: &[Target],
    tags: &[String],
) -> Result<Outcome> {
    let today = env.clock.today();
    let tags = tags
        .iter()
        .map(|t| normalize(parse_tag(t)?, today))
        .collect::<Result<Vec<Tag>>>()?;
    run(env, "tag", false, |tx| {
        tx.each_edit(targets, &env.actor, |tx, id| {
            tag_item(tx, env, id, &tags)
        })
    })
}

/// The tag `arg` is, or a usage error.
pub(super) fn parse_tag(arg: &str) -> Result<Tag> {
    let line = Line::parse(arg);
    let mut toks = line.tokens();
    match (toks.next(), toks.next()) {
        (Some(tok), None)
            if tok.text == arg.as_bytes() && tok.kind != Kind::Word =>
        {
            Ok(Tag {
                kind: tok.kind,
                key: tok.key.to_vec(),
                text: tok.text.to_vec(),
            })
        }
        _ => Err(not_a_tag(arg)),
    }
}

fn not_a_tag(arg: &str) -> Error {
    Error::new(Code::Usage, format!("{} is not a tag", quote(arg)))
        .with_hint("use +name, @name or key:value")
}

/// The tag with a fuzzy date resolved and a recurrence checked.
fn normalize(tag: Tag, today: Date) -> Result<Tag> {
    if tag.kind != Kind::Tag {
        return Ok(tag);
    }
    match tag.key.as_slice() {
        b"rec" => {
            check_rec(&Line::parse(&tag.text))?;
            Ok(tag)
        }
        b"due" | b"t" => resolve_date(tag, today),
        _ => Ok(tag),
    }
}

fn resolve_date(tag: Tag, today: Date) -> Result<Tag> {
    let text = String::from_utf8_lossy(&tag.text).into_owned();
    let key = String::from_utf8_lossy(&tag.key).into_owned();
    let value = text.split_once(':').map_or("", |(_, v)| v);
    let resolved = dates::resolve(value, today)
        .map_err(|e| e.with_field("field", Field::Str(key.clone())))?;
    match resolved {
        Resolved::On(date) => Ok(Tag {
            text: format!("{key}:{date}").into_bytes(),
            ..tag
        }),
        Resolved::Clear => Ok(tag),
    }
}

fn tag_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    tags: &[Tag],
) -> Result<()> {
    let mut line = tx.line_of(id)?.clone();
    let handle = tx.handle(id);
    for tag in tags {
        let step = apply_one(tx, id, &line, tag, &handle)?;
        if let Some(next) = step.next {
            line = next;
        }
        tx.say(step.said);
    }
    tx.set_edited(&env.actor, id, line.raw().to_vec())?;
    Ok(())
}

/// The result of one tag: the new line when it changed, and what to say.
struct Said {
    next: Option<Line>,
    said: Vec<u8>,
}

/// Applies one tag.
fn apply_one(
    tx: &Tx,
    id: ItemId,
    line: &Line,
    tag: &Tag,
    handle: &str,
) -> Result<Said> {
    let head = handle.as_bytes();
    Ok(match apply_tag(tx, id, line, tag)? {
        Applied::Added(next) => Said {
            next: Some(next),
            said: [head, b" tagged ", &tag.text].concat(),
        },
        Applied::Set(next, old) => Said {
            next: Some(next),
            said: [head, b" ", &old, b" is now ", &tag.text].concat(),
        },
        Applied::Kept => Said {
            next: None,
            said: [head, b" already has ", &tag.text].concat(),
        },
    })
}

/// Sets or adds `tag` on `line`.
pub(super) fn apply_tag(
    tx: &Tx,
    id: ItemId,
    line: &Line,
    tag: &Tag,
) -> Result<Applied> {
    let at = first_of_key(line, &tag.key).filter(|_| {
        tag.kind == Kind::Tag && SETTABLE_KEYS.contains(&tag.key.as_slice())
    });
    if let Some(at) = at {
        return Ok(set_first(line, at, tag));
    }
    if line.tokens().any(|t| t.text == tag.text.as_slice()) {
        return Ok(Applied::Kept);
    }
    tx.append_tok(id, line, &tag.text).map(Applied::Added)
}

/// Writes the value of `tag` over token `at`, unless it is the same.
fn set_first(line: &Line, at: usize, tag: &Tag) -> Applied {
    let old = line.token(at).map(|t| t.text.to_vec()).unwrap_or_default();
    if old == tag.text {
        return Applied::Kept;
    }
    match line.replace_token(at, &tag.text) {
        Some(next) => Applied::Set(next, old),
        None => Applied::Kept,
    }
}

/// What `untag` removes: one exact token, or every token of a key.
#[derive(Clone, Debug)]
struct Untag {
    given: String,
    key: Option<Vec<u8>>,
}

impl Untag {
    fn matches(&self, tok: &TokenView<'_>) -> bool {
        match &self.key {
            Some(key) => tok.is_tag(key),
            None => tok.text == self.given.as_bytes(),
        }
    }

    fn is_id(&self) -> bool {
        self.given.starts_with("id:")
    }
}

/// Removes each tag from each target.
///
/// # Errors
///
/// A usage error for something that is not a tag, and whatever the rules
/// refuse.
pub fn untag(
    env: &Env<'_>,
    targets: &[Target],
    tags: &[String],
) -> Result<Outcome> {
    let removals = tags
        .iter()
        .map(|t| parse_untag(t))
        .collect::<Result<Vec<Untag>>>()?;
    run(env, "untag", false, |tx| {
        tx.each_edit(targets, &env.actor, |tx, id| {
            untag_item(tx, env, id, &removals)
        })
    })
}

fn parse_untag(arg: &str) -> Result<Untag> {
    let key = match arg.strip_suffix(':') {
        None => parse_tag(arg).map(|_| None)?,
        Some(key) => match parse_tag(&format!("{key}:x")) {
            Ok(tag) if tag.kind == Kind::Tag => Some(tag.key),
            Ok(_) | Err(_) => return Err(not_a_tag(arg)),
        },
    };
    Ok(Untag {
        given: arg.to_owned(),
        key,
    })
}

/// The id an `untag id:…` removes: the one written, or the item's own.
fn lost_id(tx: &Tx, id: ItemId, given: &str) -> Vec<u8> {
    match given.strip_prefix("id:") {
        Some(written) if !written.is_empty() => written.as_bytes().to_vec(),
        Some(_) | None => tx
            .item(id)
            .and_then(|item| tx.snapshot().id_of(item))
            .map(<[u8]>::to_vec)
            .unwrap_or_default(),
    }
}

fn untag_item(
    tx: &mut Tx,
    env: &Env<'_>,
    id: ItemId,
    removals: &[Untag],
) -> Result<()> {
    let mut line = tx.line_of(id)?.clone();
    let handle = tx.handle(id);
    for removal in removals {
        let (next, removed) =
            tx.remove_where(id, &line, |t| removal.matches(t))?;
        line = next;
        if removed == 0 {
            tx.say(format!("{handle} has no {}", removal.given));
            continue;
        }
        let said = format!("{handle} untagged {}", removal.given);
        if removal.is_id() {
            tx.allow_id_loss(id);
            let lost = lost_id(tx, id, &removal.given);
            tx.say_lost(&lost, said);
        } else {
            tx.say(said);
        }
    }
    tx.set_edited(&env.actor, id, line.raw().to_vec())?;
    Ok(())
}
