//! `paste`: put lines of the clipboard into the list as one block, as
//! moves of lines lsty itself yanked or as copies, in one write.

use std::collections::HashSet;

use super::add::{Place, insert_point};
use super::env::Env;
use super::newline::{copy_error, invalid_text};
use super::relink::Relinks;
use super::result::Outcome;
use super::run::run;
use super::spawn::spawn;
use super::staged::{LineRef, Path};
use super::tx::Tx;
use super::typed::soft_tags;
use crate::errs::{Code, Error, Nr, Result, Source};
use crate::todotxt::{Line, Priority, trim};

/// A dependent that a delete unlinked: where its line is now, and its
/// text before the delete and after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relink {
    /// The number of its line in the todo file; `None` when the line is
    /// known to be gone.
    pub nr: Option<Nr>,
    /// The line before the delete took `dep:` from it.
    pub before: Vec<u8>,
    /// The line as the delete left it.
    pub after: Vec<u8>,
}

/// A line to paste. An own line comes from lsty's own yank; the other
/// kind is foreign text, which becomes one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteLine {
    /// A line lsty yanked.
    Own {
        /// The line.
        raw: Vec<u8>,
        /// The dependents that a delete of the line unlinked.
        relink: Vec<Relink>,
    },
    /// Foreign text: the whole of it becomes one item.
    Foreign(Vec<u8>),
}

/// An own line that went in as a move.
pub(super) struct Moved<'a> {
    pub(super) raw: &'a [u8],
    pub(super) relink: &'a [Relink],
}

/// Adds the lines as one block at `place`: an own line whose original is
/// gone is a move, any other line a copy. The lines pass the rules of the
/// text a person typed unless they are moves, and a dependency cycle
/// is warned about, not refused.
///
/// # Errors
///
/// A usage error when there are no lines, `invalid_value` for a copy
/// that cannot be made or a foreign text with no description, and
/// `no_item` or `wrong_state` for the anchor of `place`. Nothing is
/// written then.
pub fn paste(
    env: &Env<'_>,
    lines: &[PasteLine],
    place: &Place,
) -> Result<Outcome> {
    let mut env = env.clone();
    env.cycle_warn_only = true;
    run(&env, "paste", false, |tx| stage(tx, &env, lines, place))
}

/// What the todo file holds, to tell a copy from a move.
struct Holdings {
    ids: HashSet<Vec<u8>>,
    raws: HashSet<Vec<u8>>,
}

impl Holdings {
    fn of(tx: &Tx) -> Self {
        let snap = tx.snapshot();
        let ids = snap
            .items()
            .iter()
            .filter_map(|item| snap.id_of(item))
            .filter(|id| !id.is_empty())
            .map(<[u8]>::to_vec)
            .collect();
        let raws = snap
            .todo()
            .lines
            .iter()
            .filter(|line| !line.is_blank())
            .map(|line| line.raw().to_vec())
            .collect();
        Self { ids, raws }
    }

    /// Whether the line, or the item it names, is held.
    fn has(&self, line: &Line) -> bool {
        match line.id() {
            Some(id) => self.ids.contains(id),
            None => self.raws.contains(line.raw()),
        }
    }

    /// Notes that the line is now held.
    fn take(&mut self, line: &Line) {
        match line.id() {
            Some(id) => self.ids.insert(id.to_vec()),
            None => self.raws.insert(line.raw().to_vec()),
        };
    }
}

/// What staging the lines learns on the way.
struct Pasting<'a> {
    env: &'a Env<'a>,
    after: usize,
    held: Holdings,
    refs: Vec<LineRef>,
    /// The own lines that went in as moves.
    moved: Vec<Moved<'a>>,
    /// The number of lines a foreign text was joined from, when several.
    joined: usize,
    warns: Vec<Vec<u8>>,
}

fn stage(
    tx: &mut Tx,
    env: &Env<'_>,
    lines: &[PasteLine],
    place: &Place,
) -> Result<()> {
    if lines.is_empty() {
        return Err(Error::new(Code::Usage, "nothing to paste"));
    }
    let mut p = Pasting {
        env,
        after: insert_point(tx, place)?,
        held: Holdings::of(tx),
        refs: Vec::new(),
        moved: Vec::new(),
        joined: 0,
        warns: Vec::new(),
    };
    for line in lines {
        let (raw, path) = p.prepare(line)?;
        p.refs.push(tx.insert(Source::Todo, p.after, raw, path));
    }
    tx.primary(&p.refs);
    tx.say(p.summary(lines.len()));
    for warning in std::mem::take(&mut p.warns) {
        tx.say(warning);
    }
    Relinks::new().restore(tx, &p.moved);
    Ok(())
}

impl<'a> Pasting<'a> {
    /// The text a line goes in as, and the rules it passes.
    fn prepare(&mut self, line: &'a PasteLine) -> Result<(Vec<u8>, Path)> {
        match line {
            PasteLine::Foreign(text) => {
                self.foreign(text).map(|raw| (raw, Path::Text))
            }
            PasteLine::Own { raw, relink } => self.own(raw, relink),
        }
    }

    /// An own line goes in as a copy when the list holds it, and as a
    /// move when it does not.
    fn own(
        &mut self,
        raw: &'a [u8],
        relink: &'a [Relink],
    ) -> Result<(Vec<u8>, Path)> {
        let parsed = Line::parse(raw);
        if self.held.has(&parsed) {
            let raw = self.copied(&parsed, &parsed)?;
            return Ok((raw, Path::Text));
        }
        self.held.take(&parsed);
        self.moved.push(Moved { raw, relink });
        Ok((raw.to_vec(), Path::Action))
    }

    /// The line of foreign text: joined into one line, with the priority
    /// in capitals.
    fn foreign(&mut self, text: &[u8]) -> Result<Vec<u8>> {
        let (text, lines) = join_foreign(text);
        if lines > 1 {
            self.joined = lines;
        }
        let text = capital_priority(text);
        let raw = self.copied(&Line::parse(text), &Line::parse([]))?;
        if !Line::parse(&raw).has_text() {
            return Err(invalid_text("empty task", ""));
        }
        Ok(raw)
    }

    /// A new open item made from `line`, with the dates of its tags
    /// written out unless `held` has them already.
    fn copied(&mut self, line: &Line, held: &Line) -> Result<Vec<u8>> {
        let Some(spawned) = spawn(self.env, line) else {
            return Err(copy_error(line.raw()));
        };
        let today = self.env.clock.today();
        let (raw, warns) = soft_tags(spawned.raw(), held, today);
        self.warns.extend(warns);
        Ok(raw)
    }

    fn summary(&self, n: usize) -> String {
        let plural = if n == 1 { "" } else { "s" };
        let head = format!("pasted {n} item{plural}");
        if !self.moved.is_empty() {
            format!("{head} ({} moved)", self.moved.len())
        } else if self.joined > 0 {
            format!("{head} (joined from {} lines)", self.joined)
        } else {
            head
        }
    }
}

/// Foreign text as one line: line breaks and tabs are spaces and the
/// ends are trimmed. Also the number of lines it had.
fn join_foreign(text: &[u8]) -> (Vec<u8>, usize) {
    let unix = text
        .iter()
        .enumerate()
        .filter(|&(i, &b)| !(b == b'\r' && text.get(i + 1) == Some(&b'\n')))
        .map(|(_, &b)| b)
        .collect::<Vec<u8>>();
    let trimmed = trim(&unix);
    let lines = trimmed.split(|&b| b == b'\n').count();
    let flat: Vec<u8> = trimmed
        .iter()
        .map(|&b| {
            if matches!(b, b'\n' | b'\r' | b'\t') {
                b' '
            } else {
                b
            }
        })
        .collect();
    (trim(&flat).to_vec(), lines)
}

/// The text with a priority of its own, `(b)`, written as `(B)`.
fn capital_priority(text: Vec<u8>) -> Vec<u8> {
    let [b'(', letter, b')', b' ', rest @ ..] = text.as_slice() else {
        return text;
    };
    let Some(own) = Priority::new(letter.to_ascii_uppercase()) else {
        return text;
    };
    let mut out = vec![b'(', own.letter(), b')', b' '];
    out.extend_from_slice(rest);
    out
}
