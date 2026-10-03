use std::collections::BTreeMap;

use super::snapshot::{Item, Snapshot};
use crate::todotxt::Kind;

/// How many items carry one exact project or context name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameCount {
    /// The name without its sigil, as written.
    pub name: Vec<u8>,
    /// The open items that carry it.
    pub open: usize,
    /// All items that carry it.
    pub total: usize,
}

impl Snapshot {
    /// Counts the exact names of `kind` that `items` carry, in byte order.
    /// An item counts once for a name it repeats.
    pub fn names(&self, items: &[&Item], kind: Kind) -> Vec<NameCount> {
        let mut counts: BTreeMap<&[u8], (usize, usize)> = BTreeMap::new();
        for item in items {
            let mut names: Vec<&[u8]> = self
                .line(item)
                .tokens()
                .filter(|t| t.kind == kind)
                .map(|t| t.value)
                .collect();
            names.sort_unstable();
            names.dedup();
            for name in names {
                let entry = counts.entry(name).or_default();
                entry.1 += 1;
                entry.0 += usize::from(item.is_open());
            }
        }
        counts
            .into_iter()
            .map(|(name, (open, total))| NameCount {
                name: name.to_vec(),
                open,
                total,
            })
            .collect()
    }
}
