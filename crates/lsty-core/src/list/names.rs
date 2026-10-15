use std::collections::BTreeMap;

use super::{
    counts::Count,
    snapshot::{Snapshot, Task},
};
use crate::todotxt::Kind;

impl Snapshot {
    /// Counts the exact names of `kind` that `tasks` carry, in byte order,
    /// each with its name without the sigil as written. A task counts
    /// once for a name it repeats.
    pub fn names(
        &self,
        tasks: &[&Task],
        kind: Kind,
    ) -> BTreeMap<&[u8], Count> {
        let mut counts: BTreeMap<&[u8], Count> = BTreeMap::new();
        let mut names: Vec<&[u8]> = Vec::new();
        for task in tasks {
            names.clear();
            names.extend(
                self.line(task)
                    .tokens()
                    .filter(|t| t.kind == kind)
                    .map(|t| t.value),
            );
            names.sort_unstable();
            names.dedup();
            for name in &names {
                let count = counts.entry(name).or_default();
                count.total += 1;
                count.open += usize::from(task.is_open());
            }
        }
        counts
    }
}
