//! Giving the dependents of a moved task their `dep:` back.

use std::collections::{HashMap, HashSet};

use super::{
    staged::{LineRef, Path},
    tx::Tx,
};
use crate::{
    errs::{Nr, Source, lossy},
    todotxt::{Line, share},
};

/// The `dep:` links of the todo file as staged: for each id, the ids it
/// waits for.
pub(super) type Graph = HashMap<Vec<u8>, Vec<Vec<u8>>>;

/// A dependent that a delete unlinked: where its line is now, and its
/// text before the delete and after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relink {
    /// The number of its line in the todo file; `None` when the line is
    /// known to be gone.
    pub nr: Option<Nr>,
    /// An outside edit moved the line or made it a twin: it is not to be
    /// written on, whatever it reads.
    pub lost: bool,
    /// The line before the delete took `dep:` from it.
    pub before: Vec<u8>,
    /// The line as the delete left it.
    pub after: Vec<u8>,
}

/// An own line that went in as a move.
pub(super) struct Moved<'a> {
    /// Its id, or empty.
    pub(super) id: Vec<u8>,
    pub(super) relink: &'a [Relink],
}

/// Restores the dependents of the moved lines and says so.
pub(super) fn relink(tx: &mut Tx, moved: &[Moved<'_>]) {
    let mut graph = None;
    let mut nrs = Vec::new();
    let mut warns = Vec::new();
    for line in moved {
        for dependent in line.relink {
            match tx.restore(&mut graph, &line.id, dependent) {
                Ok(nr) if nrs.contains(&nr) => {}
                Ok(nr) => nrs.push(nr),
                Err(warning) => warns.push(warning),
            }
        }
    }
    if !nrs.is_empty() {
        tx.say(format!(
            "relinked {}",
            nrs.iter()
                .map(|nr| tx.line_name(Source::Todo, nr))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for warning in warns {
        tx.say(warning);
    }
}

/// The id of a line, or empty.
fn id_text(raw: &[u8]) -> Vec<u8> {
    Line::parse(raw).tags().id.map_or_default(<[u8]>::to_vec)
}

fn graph_of(tx: &Tx) -> Graph {
    let mut graph = Graph::new();
    for &i in tx.todo.order() {
        let Some(row) = tx.todo.rows().get(i) else {
            continue;
        };
        let line = Line::parse_shared(&row.raw);
        let tags = line.tags();
        if let Some(id) = tags.id {
            let deps = tags.deps.into_iter().map(<[u8]>::to_vec);
            graph.entry(id.to_vec()).or_default().extend(deps);
        }
    }
    graph
}

impl Tx {
    /// Puts the dependent of `relink` back as it was: its number, or the
    /// warning why not.
    fn restore(
        &mut self,
        graph: &mut Option<Graph>,
        id: &[u8],
        relink: &Relink,
    ) -> Result<Nr, Vec<u8>> {
        let text = lossy(id);
        let Some(nr) = relink.nr else {
            return gone(&text);
        };
        let changed = |name: String| {
            let why = format!("{name} changed since; dep:{text} not restored");
            Err(why.into_bytes())
        };
        let landed = self.landing.as_ref().map_or_else(
            || Some(nr.index()),
            |landing| landing.line(Source::Todo, nr.index()),
        );
        let Some(index) = landed else {
            return changed(self.unlanded(relink, nr));
        };
        let Some(row) = self.todo.rows().get(index) else {
            return gone(&text);
        };
        if Line::is_blank_text(&row.orig) {
            return gone(&text);
        }
        let nr = self.written_at(index, nr);
        if *row.raw == *relink.before {
            return Ok(nr);
        }
        if relink.lost || *row.orig != *relink.after {
            return changed(self.line_name(Source::Todo, nr));
        }
        let dependent = id_text(&relink.before);
        let graph = graph.get_or_insert_with(|| graph_of(self));
        if closes_cycle(graph, id, &dependent) {
            let nr = self.line_name(Source::Todo, nr);
            let why =
                format!("{nr}: dep:{text} would close a cycle; not restored");
            return Err(why.into_bytes());
        }
        if !dependent.is_empty() {
            graph.entry(dependent).or_default().push(id.to_vec());
        }
        if let Some(row) = self.todo.rows_mut().get_mut(index) {
            row.raw = share(&relink.before);
            row.path = row.path.max(Path::Action);
        }
        let at = LineRef {
            source: Source::Todo,
            index,
        };
        self.side.push(at);
        Ok(nr)
    }
}

impl Tx {
    /// The number the line of row `index` has when written, else `nr`.
    fn written_at(&self, index: usize, nr: Nr) -> Nr {
        let at = self.todo.position(index);
        at.and_then(Nr::from_index).unwrap_or(nr)
    }

    /// How a warning names a dependent with no known line: by the place of
    /// the one line that reads as the delete left it, else by its id, else
    /// by the line the record holds.
    fn unlanded(&self, relink: &Relink, nr: Nr) -> String {
        let rows = self.todo.rows().iter().enumerate();
        let mut same = rows
            .filter(|(_, r)| !r.removed && *r.raw == *relink.after)
            .map(|(index, _)| index);
        let id = id_text(&relink.before);
        match (same.next(), same.next()) {
            (Some(index), None) => {
                self.line_name(Source::Todo, self.written_at(index, nr))
            }
            _ if !id.is_empty() => format!("id:{}", lossy(&id)),
            _ => nr.to_string(),
        }
    }
}

fn gone(id: &str) -> Result<Nr, Vec<u8>> {
    Err(format!("a dependent of {id} is gone").into_bytes())
}

/// Whether `id` already waits for `dependent`, through any number of
/// tasks: a link from `dependent` to `id` would close a loop.
pub(super) fn closes_cycle(
    graph: &Graph,
    id: &[u8],
    dependent: &[u8],
) -> bool {
    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut stack: Vec<&[u8]> = vec![id];
    while let Some(at) = stack.pop() {
        if at == dependent {
            return true;
        }
        if !seen.insert(at) {
            continue;
        }
        let next = graph.get(at).into_iter().flatten();
        stack.extend(next.map(Vec::as_slice));
    }
    false
}
