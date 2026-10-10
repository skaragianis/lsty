//! `paste`: put lines of the clipboard into the list as one block, as
//! moves of lines lsty itself yanked or as copies, in one write.

use std::collections::HashSet;

use super::{
    add::{Place, insert_point},
    env::Env,
    newline::{copy_error, invalid_text},
    relink::{Moved, Relink, relink},
    result::Outcome,
    run::{Rules, run_with},
    spawn::spawn,
    staged::Path,
    tx::Tx,
    typed::soft_tags,
};
use crate::{
    errs::{Code, Error, Result, Source},
    list::Snapshot,
    todotxt::{Line, trim},
};

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
    run_with(env, "paste", false, Rules::WarnCycles, |tx| {
        stage(tx, env, lines, place)
    })
}

/// What the paste itself has taken, and the pasted own lines that the todo
/// file already holds, to tell a copy from a move.
struct Holdings {
    ids: HashSet<Vec<u8>>,
    raws: HashSet<Vec<u8>>,
}

impl Holdings {
    fn of(tx: &Tx, lines: &[PasteLine]) -> Self {
        let wanted: HashSet<&[u8]> = lines
            .iter()
            .filter_map(|line| match line {
                PasteLine::Own { raw, .. } => Some(raw.as_slice()),
                PasteLine::Foreign(_) => None,
            })
            .collect();
        let raws = tx
            .snapshot()
            .todo()
            .lines
            .iter()
            .filter(|line| !line.is_blank() && wanted.contains(line.raw()))
            .map(|line| line.raw().to_vec())
            .collect();
        Self {
            ids: HashSet::new(),
            raws,
        }
    }

    /// Whether the line, or the item it names, is held.
    fn has(&self, snap: &Snapshot, line: &Line) -> bool {
        match line.tags().id {
            Some(id) => {
                self.ids.contains(id)
                    || snap.items_with_id(id).next().is_some()
            }
            None => self.raws.contains(line.raw()),
        }
    }

    /// Notes that the line is now held.
    fn take(&mut self, line: &Line) {
        match line.tags().id {
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
        held: Holdings::of(tx, lines),
        moved: Vec::new(),
        joined: 0,
        warns: Vec::new(),
    };
    for line in lines {
        let (raw, path) = p.prepare(tx, line)?;
        let line = tx.insert(Source::Todo, p.after, raw, path);
        tx.primary.push(line);
    }
    tx.say(p.summary(lines.len()));
    for warning in p.warns {
        tx.say(warning);
    }
    relink(tx, &p.moved);
    Ok(())
}

impl<'a> Pasting<'a> {
    /// The text a line goes in as, and the rules it passes.
    fn prepare(
        &mut self,
        tx: &Tx,
        line: &'a PasteLine,
    ) -> Result<(Vec<u8>, Path)> {
        match line {
            PasteLine::Foreign(text) => {
                self.foreign(text).map(|raw| (raw, Path::Text))
            }
            PasteLine::Own { raw, relink } => self.own(tx, raw, relink),
        }
    }

    /// An own line goes in as a copy when the list holds it, and as a
    /// move when it does not.
    fn own(
        &mut self,
        tx: &Tx,
        raw: &'a [u8],
        relink: &'a [Relink],
    ) -> Result<(Vec<u8>, Path)> {
        let parsed = Line::parse(raw);
        if self.held.has(tx.snapshot(), &parsed) {
            let raw = self.copied(&parsed, &parsed)?;
            return Ok((raw, Path::Text));
        }
        self.held.take(&parsed);
        self.moved.push(Moved {
            id: parsed.tags().id.unwrap_or_default().to_vec(),
            relink,
        });
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
    let text = trim(text);
    let lines = text.split(|&b| b == b'\n').count();
    let flat = text
        .iter()
        .enumerate()
        .filter(|&(i, &b)| !(b == b'\r' && text.get(i + 1) == Some(&b'\n')))
        .map(|(_, &b)| {
            if matches!(b, b'\n' | b'\r' | b'\t') {
                b' '
            } else {
                b
            }
        })
        .collect();
    (flat, lines)
}

/// The text with a priority of its own, `(b)`, written as `(B)`.
fn capital_priority(mut text: Vec<u8>) -> Vec<u8> {
    if let [b'(', letter, b')', b' ', ..] = text.as_mut_slice() {
        letter.make_ascii_uppercase();
    }
    text
}
