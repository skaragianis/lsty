use std::collections::BTreeMap;

use super::counts::Count;
use super::snapshot::{Item, Snapshot};
use crate::todotxt::Kind;

impl Snapshot {
    /// Counts the exact names of `kind` that `items` carry, in byte order,
    /// each with its name without the sigil as written. An item counts
    /// once for a name it repeats.
    pub fn names(
        &self,
        items: &[&Item],
        kind: Kind,
    ) -> BTreeMap<&[u8], Count> {
        let mut counts: BTreeMap<&[u8], Count> = BTreeMap::new();
        let mut names: Vec<&[u8]> = Vec::new();
        for item in items {
            names.clear();
            names.extend(
                self.line(item)
                    .tokens()
                    .filter(|t| t.kind == kind)
                    .map(|t| t.value),
            );
            names.sort_unstable();
            names.dedup();
            for name in &names {
                let count = counts.entry(name).or_default();
                count.total += 1;
                count.open += usize::from(item.is_open());
            }
        }
        counts
    }
}
