//! Giving the dependents of a moved item their `dep:` back.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::paste::{Moved, Relink};
use super::staged::{LineRef, Path};
use super::tx::Tx;
use crate::errs::Source;
use crate::todotxt::Line;

/// The `dep:` links of the todo file as staged: for each id, the ids it
/// waits for.
pub(super) type Graph = HashMap<Vec<u8>, Vec<Vec<u8>>>;

/// What restoring the dependents of all moved lines made.
pub(super) struct Relinks {
    /// Built when a dependent first needs the cycle check.
    graph: Option<Graph>,
    /// The numbers of the restored dependents, once each.
    nrs: Vec<String>,
    warns: Vec<Vec<u8>>,
}

/// What became of one dependent.
enum Restored {
    Back,
    Warned(Vec<u8>),
}

impl Relinks {
    pub(super) fn new() -> Self {
        Self {
            graph: None,
            nrs: Vec::new(),
            warns: Vec::new(),
        }
    }

    /// Restores the dependents of the moved lines and says so.
    pub(super) fn restore(mut self, tx: &mut Tx, moved: &[Moved<'_>]) {
        for line in moved {
            let id = id_text(line.raw);
            for relink in line.relink {
                let done = tx.restore(&mut self.graph, &id, relink);
                self.add(relink, done);
            }
        }
        if !self.nrs.is_empty() {
            tx.say(format!("relinked {}", self.nrs.join(", ")));
        }
        for warning in self.warns {
            tx.say(warning);
        }
    }

    fn add(&mut self, relink: &Relink, restored: Restored) {
        match restored {
            Restored::Back => {
                let nr =
                    relink.nr.map(|nr| nr.to_string()).unwrap_or_default();
                if !self.nrs.contains(&nr) {
                    self.nrs.push(nr);
                }
            }
            Restored::Warned(text) => self.warns.push(text),
        }
    }
}

/// The id of a line, or empty.
fn id_text(raw: &[u8]) -> Vec<u8> {
    Line::parse(raw.to_vec())
        .id()
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

fn graph_of(tx: &Tx) -> Graph {
    let mut graph = Graph::new();
    for &i in tx.todo.order() {
        let Some(row) = tx.todo.rows().get(i) else {
            continue;
        };
        let line = Line::parse_shared(&row.raw);
        if let Some(id) = line.id() {
            let deps = line.tags().deps.into_iter().map(<[u8]>::to_vec);
            graph.entry(id.to_vec()).or_default().extend(deps);
        }
    }
    graph
}

impl Tx {
    /// Puts the dependent of `relink` back as it was.
    fn restore(
        &mut self,
        graph: &mut Option<Graph>,
        id: &[u8],
        relink: &Relink,
    ) -> Restored {
        let text = String::from_utf8_lossy(id).into_owned();
        let Some(nr) = relink.nr else {
            return gone(&text);
        };
        let index = nr.index();
        let Some(row) = self.todo.rows().get(index) else {
            return gone(&text);
        };
        if Line::parse_shared(&row.orig).is_blank() {
            return gone(&text);
        }
        if *row.raw == *relink.before {
            return Restored::Back;
        }
        if *row.orig != *relink.after {
            let why = format!("{nr} changed since; dep:{text} not restored");
            return Restored::Warned(why.into_bytes());
        }
        let dependent = id_text(&relink.before);
        let graph = graph.get_or_insert_with(|| graph_of(self));
        if closes_cycle(graph, id, &dependent) {
            let why =
                format!("{nr}: dep:{text} would close a cycle; not restored");
            return Restored::Warned(why.into_bytes());
        }
        if !dependent.is_empty() {
            graph.entry(dependent).or_default().push(id.to_vec());
        }
        if let Some(row) = self.todo.row_mut(index) {
            row.raw = Arc::from(relink.before.as_slice());
            row.path = row.path.max(Path::Action);
        }
        let at = LineRef {
            source: Source::Todo,
            index,
        };
        self.side(&[at]);
        Restored::Back
    }
}

fn gone(id: &str) -> Restored {
    Restored::Warned(format!("a dependent of {id} is gone").into_bytes())
}

/// Whether `dependent` already waits for `id`, through any number of
/// items: a link from `dependent` to `id` would close a loop.
pub(super) fn closes_cycle(
    graph: &Graph,
    id: &[u8],
    dependent: &[u8],
) -> bool {
    if dependent.is_empty() {
        return false;
    }
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
