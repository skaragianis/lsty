use std::collections::BTreeMap;

use super::snapshot::Snapshot;
use crate::{
    errs::Source,
    todotxt::{Kind, name_levels},
};

/// How many tasks carry a name: the open ones of the todo file, and all
/// of them in both files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    /// The open tasks of the todo file.
    pub open: usize,
    /// The tasks of the todo file and of the done file.
    pub total: usize,
}

/// A count and whether any task of the todo file carries the name.
#[derive(Default)]
struct Tally {
    count: Count,
    listed: bool,
}

impl Snapshot {
    /// Counts each name of `kind` and every level above it over the tasks
    /// at or below that level, each task once. Only names at or above a
    /// name that a task of the todo file carries are listed; the done
    /// file adds to their totals.
    pub fn rolled_counts(&self, kind: Kind) -> BTreeMap<Vec<u8>, Count> {
        let mut tallies: BTreeMap<&[u8], Tally> = BTreeMap::new();
        let mut keys: Vec<&[u8]> = Vec::new();
        for task in self.tasks() {
            keys.clear();
            let names = self.line(task).tokens().filter(|t| t.kind == kind);
            keys.extend(names.flat_map(|t| name_levels(t.value)));
            keys.sort_unstable();
            keys.dedup();
            let in_todo = task.source() == Source::Todo;
            for key in &keys {
                let t = tallies.entry(key).or_default();
                t.count.total += 1;
                t.count.open += usize::from(in_todo && task.is_open());
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
