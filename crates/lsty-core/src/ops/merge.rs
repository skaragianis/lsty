//! The merge of an edit with the line as it is on disk now: the changes
//! from the line the edit started from to the line typed are applied to
//! the disk's line.

use crate::errs::lossy;
use crate::todotxt::{
    Kind, Line, Parts as LineParts, Priority, TokenView, build,
};

/// The keys of which only the first token counts.
const SINGLE_KEYS: [&str; 6] = ["due", "t", "rec", "pri", "started", "owner"];

/// A change that a stamp-like word would turn into a stamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StampWord;

/// A part of a line that the merge keeps apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Part {
    Priority,
    Text,
    /// The first tag with this key, of those that hold one value.
    Key(&'static str),
}

impl Part {
    /// How a message names the part.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Priority => "priority",
            Self::Text => "text",
            Self::Key(key) => key,
        }
    }
}

/// A part that both sides changed to different values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Clash {
    pub(super) part: Part,
    /// The value on disk, as a message shows it.
    pub(super) disk: String,
}

/// The merged line, and the parts that clash; the merged line has the
/// draft's version of each of those.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Merged {
    pub(super) line: Vec<u8>,
    pub(super) clashes: Vec<Clash>,
}

/// A line as the merge sees it.
struct Parts<'a> {
    priority: Option<Priority>,
    text: Vec<u8>,
    /// The projects, contexts and tags, in order.
    tokens: Vec<TokenView<'a>>,
}

impl<'a> Parts<'a> {
    fn of(line: &'a Line) -> Self {
        Self {
            priority: line.effective_priority(),
            text: line.text(),
            tokens: line.tokens().filter(|t| t.kind != Kind::Word).collect(),
        }
    }

    /// The first token of the key.
    fn first_of(&self, key: &str) -> Option<&TokenView<'a>> {
        self.tokens.iter().find(|t| t.is_tag(key.as_bytes()))
    }
}

/// Applies the changes from `base` to `draft` onto `disk`.
///
/// # Errors
///
/// [`StampWord`] when a removal or an append would turn a word into a
/// stamp.
pub(super) fn merge(
    base: &[u8],
    draft: &[u8],
    disk: &[u8],
) -> Result<Merged, StampWord> {
    let (b, d, k) = (Line::parse(base), Line::parse(draft), Line::parse(disk));
    let (pb, pd, pk) = (Parts::of(&b), Parts::of(&d), Parts::of(&k));
    let clashes = clashes_of(&pb, &pd, &pk);
    let line = apply_text(k.clone(), &pb, &pd)?;
    let line = apply_tokens(line, &pb, &pd, &pk, &clashes)?;
    let line = apply_priority(line, &pb, &pd)?;
    Ok(Merged {
        line: line.raw().to_vec(),
        clashes,
    })
}

/// Whether both sides changed the part to different values.
fn clash<T: PartialEq>(base: &T, draft: &T, disk: &T) -> bool {
    draft != base && disk != base && draft != disk
}

fn clashes_of(b: &Parts<'_>, d: &Parts<'_>, k: &Parts<'_>) -> Vec<Clash> {
    let mut out = Vec::new();
    if clash(&b.priority, &d.priority, &k.priority) {
        let disk = k.priority.map_or_else(
            || "none".to_owned(),
            |p| char::from(p.letter()).to_string(),
        );
        out.push(Clash {
            part: Part::Priority,
            disk,
        });
    }
    if clash(&b.text, &d.text, &k.text) {
        out.push(Clash {
            part: Part::Text,
            disk: crate::errs::quote_bytes(&k.text),
        });
    }
    for key in SINGLE_KEYS {
        let [was, mine, theirs] =
            [b, d, k].map(|p| p.first_of(key).map(|t| t.value));
        if clash(&was, &mine, &theirs) {
            out.push(Clash {
                part: Part::Key(key),
                disk: theirs.map_or_else(|| "none".to_owned(), lossy),
            });
        }
    }
    out
}

/// Puts the draft's free text where the first free word of the disk's
/// line stood, taking the other free words out.
///
/// # Errors
///
/// [`StampWord`] when the placed text would start with a done mark,
/// priority or date that the disk's line does not have.
fn apply_text(
    disk: Line,
    b: &Parts<'_>,
    d: &Parts<'_>,
) -> Result<Line, StampWord> {
    if d.text == b.text {
        return Ok(disk);
    }
    let line = place_text(&disk, d)?;
    if line.same_stamps(&disk) {
        Ok(line)
    } else {
        Err(StampWord)
    }
}

fn place_text(disk: &Line, d: &Parts<'_>) -> Result<Line, StampWord> {
    let words: Vec<usize> = disk
        .tokens()
        .enumerate()
        .filter(|(_, t)| t.kind == Kind::Word)
        .map(|(i, _)| i)
        .collect();
    let Some((&first, rest)) = words.split_first() else {
        return Ok(with_text(disk, &d.text));
    };
    let mut line = disk.clone();
    for &i in rest.iter().rev() {
        line = line.remove_token(i).map_err(|_| StampWord)?;
    }
    if d.text.is_empty() {
        return line.remove_token(first).map_err(|_| StampWord);
    }
    line.replace_token(first, &d.text).ok_or(StampWord)
}

/// The line with `text` at the start of its description.
fn with_text(line: &Line, text: &[u8]) -> Line {
    if text.is_empty() {
        return line.clone();
    }
    let desc = line.description();
    let head = line.raw().get(..line.raw().len() - desc.len());
    let mut joined = text.to_vec();
    joined.push(b' ');
    joined.extend_from_slice(desc);
    let mut raw = head.unwrap_or_default().to_vec();
    raw.extend_from_slice(crate::todotxt::trim(&joined));
    Line::parse(raw)
}

fn apply_tokens(
    line: Line,
    b: &Parts<'_>,
    d: &Parts<'_>,
    k: &Parts<'_>,
    clashes: &[Clash],
) -> Result<Line, StampWord> {
    let skip = |tok: &TokenView<'_>| {
        clashes.iter().any(
            |c| matches!(c.part, Part::Key(key) if tok.is_tag(key.as_bytes())),
        )
    };
    let added = surplus(&d.tokens, &b.tokens, skip);
    let removed = surplus(&b.tokens, &d.tokens, skip);
    let mut line = line;
    for tok in removed {
        let at = line.tokens().position(|t| t.text == tok);
        if let Some(at) = at {
            line = line.remove_token(at).map_err(|_| StampWord)?;
        }
    }
    for tok in added {
        if !k.tokens.iter().any(|t| t.text == tok) {
            line = line.append_token(tok).map_err(|_| StampWord)?;
        }
    }
    apply_keys(line, d, clashes)
}

/// The tokens of `from` that exceed their count in `other`, in order,
/// leaving out those that `skip` names.
fn surplus<'a>(
    from: &[TokenView<'a>],
    other: &[TokenView<'a>],
    skip: impl Fn(&TokenView<'_>) -> bool,
) -> Vec<&'a [u8]> {
    let mut left: Vec<&[u8]> = other.iter().map(|t| t.text).collect();
    let mut out = Vec::new();
    for tok in from {
        if let Some(at) = left.iter().position(|t| *t == tok.text) {
            left.swap_remove(at);
        } else if !skip(tok) {
            out.push(tok.text);
        }
    }
    out
}

/// Gives each clashing key the draft's first token, in the place of the
/// first one on disk.
fn apply_keys(
    mut line: Line,
    d: &Parts<'_>,
    clashes: &[Clash],
) -> Result<Line, StampWord> {
    for clash in clashes {
        if let Part::Key(key) = clash.part {
            line = set_key(line, key, d.first_of(key).map(|t| t.text))?;
        }
    }
    Ok(line)
}

fn set_key(
    line: Line,
    key: &str,
    token: Option<&[u8]>,
) -> Result<Line, StampWord> {
    let at: Vec<usize> = line
        .tokens()
        .enumerate()
        .filter(|(_, t)| t.is_tag(key.as_bytes()))
        .map(|(i, _)| i)
        .collect();
    let Some((&first, rest)) = at.split_first() else {
        return match token {
            Some(token) => line.append_token(token).map_err(|_| StampWord),
            None => Ok(line),
        };
    };
    let mut line = line;
    for &i in rest.iter().rev() {
        line = line.remove_token(i).map_err(|_| StampWord)?;
    }
    match token {
        Some(token) => line.replace_token(first, token).ok_or(StampWord),
        None => line.remove_token(first).map_err(|_| StampWord),
    }
}

/// The line with the draft's priority.
/// Sets the draft's priority when the draft changed it.
///
/// # Errors
///
/// [`StampWord`] when a word at the start of the description would then
/// read as a done mark, priority or date.
fn apply_priority(
    line: Line,
    b: &Parts<'_>,
    d: &Parts<'_>,
) -> Result<Line, StampWord> {
    if d.priority == b.priority {
        return Ok(line);
    }
    let parts = LineParts {
        done: line.is_done(),
        priority: d.priority,
        completed: line.completed(),
        created: line.created(),
        description: line.description(),
    };
    let out = Line::parse(build(&parts));
    let kept = out.is_done() == parts.done
        && out.priority() == parts.priority
        && out.completed() == parts.completed
        && out.created() == parts.created;
    if kept { Ok(out) } else { Err(StampWord) }
}
