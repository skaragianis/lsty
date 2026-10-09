use std::collections::BTreeMap;

use super::snapshot::Snapshot;
use crate::errs::Source;
use crate::todotxt::{Kind, name_parent};

/// How many items carry a name: the open ones of the todo file, and all
/// of them in both files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    /// The open items of the todo file.
    pub open: usize,
    /// The items of the todo file and of the done file.
    pub total: usize,
}

/// A count and whether any item of the todo file carries the name.
#[derive(Default)]
struct Tally {
    count: Count,
    listed: bool,
}

/// The names an item contributes for one name it carries: the name and
/// every level above it.
fn rolled<'a>(mut name: &'a [u8], out: &mut Vec<&'a [u8]>) {
    while !name.is_empty() {
        out.push(name);
        name = name_parent(name);
    }
}

impl Snapshot {
    /// Counts each name of `kind` and every level above it over the items
    /// at or below that level, each item once. Only names at or above a
    /// name that an item of the todo file carries are listed; the done
    /// file adds to their totals.
    pub fn rolled_counts(&self, kind: Kind) -> BTreeMap<Vec<u8>, Count> {
        let mut tallies: BTreeMap<&[u8], Tally> = BTreeMap::new();
        let mut keys: Vec<&[u8]> = Vec::new();
        for item in self.items() {
            keys.clear();
            let names = self.line(item).tokens().filter(|t| t.kind == kind);
            for tok in names {
                rolled(tok.value, &mut keys);
            }
            keys.sort_unstable();
            keys.dedup();
            let in_todo = item.source() == Source::Todo;
            for key in &keys {
                let t = tallies.entry(key).or_default();
                t.count.total += 1;
                t.count.open += usize::from(in_todo && item.is_open());
                t.listed |= in_todo;
            }
        }
        tallies
            .into_iter()
            .filter(|(_, t)| t.listed)
            .map(|(name, t)| (name.to_vec(), t.count))
            .collect()
    }
}
