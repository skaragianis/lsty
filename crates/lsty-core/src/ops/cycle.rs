//! The cycle check of the rule layer: a `dep:` that puts its item on a
//! dependency cycle is refused, whichever operation staged it.

use super::env::Env;
use super::idrules::At;
use super::staged::Row;
use super::tx::Tx;
use crate::errs::{Code, Error, Field, Ref, Source};
use crate::list::{ItemId, Snapshot};
use crate::todotxt::Line;

/// A staged line that gained `dep:` tokens, and the ids they name.
struct DepRow {
    at: At,
    added: Vec<Vec<u8>>,
}

impl Tx {
    /// Refuses every new `dep:` that closes a loop, or warns about it
    /// when the caller asked for that.
    pub(super) fn check_cycles(
        &mut self,
        env: &Env<'_>,
    ) -> crate::errs::Result<()> {
        let rows = self.dep_rows();
        if rows.is_empty() {
            return Ok(());
        }
        let after = self.snapshot_after(env);
        for row in &rows {
            let Some(path) = self.loop_of(&after, row) else {
                continue;
            };
            let handle = self.row_handle(row.at);
            if !env.cycle_warn_only {
                return Err(cycle_error(&handle, &after, &path));
            }
            self.say(format!("{handle} is now part of a dependency cycle"));
        }
        Ok(())
    }

    fn dep_rows(&self) -> Vec<DepRow> {
        [Source::Todo, Source::Done]
            .into_iter()
            .flat_map(|source| {
                let rows = self.file(source).all.iter().enumerate();
                rows.map(move |(index, row)| DepRow {
                    at: At { source, index },
                    added: new_deps(row),
                })
            })
            .filter(|row| !row.added.is_empty())
            .collect()
    }

    /// The loop through the row's item that one of its new `dep:`
    /// tokens closes: the item, then the items around back to it.
    fn loop_of(&self, after: &Snapshot, row: &DepRow) -> Option<Vec<ItemId>> {
        let line = self.file(row.at.source).line_ref(row.at.index);
        let found = self.items_after(after, &[line]);
        let id = *found.first()?;
        let item = after.item(id)?;
        for &pre in item.blockers() {
            let named = after.item(pre).and_then(|p| after.id_of(p));
            if !named.is_some_and(|n| row.added.iter().any(|a| a == n)) {
                continue;
            }
            if let Some(mut back) = after.path_to(pre, id) {
                back.insert(0, id);
                return Some(back);
            }
        }
        None
    }
}

/// The ids of the `dep:` tokens the row gained.
fn new_deps(row: &Row) -> Vec<Vec<u8>> {
    if row.removed || (!row.added && row.raw == row.orig) {
        return Vec::new();
    }
    let was = Line::parse(row.orig.clone());
    let have = was.tags().deps;
    let now = Line::parse(row.raw.clone());
    now.tags()
        .deps
        .into_iter()
        .filter(|id| !have.contains(id))
        .map(<[u8]>::to_vec)
        .collect()
}

fn cycle_error(handle: &str, after: &Snapshot, path: &[ItemId]) -> Error {
    let refs: Vec<Ref> = path
        .iter()
        .filter_map(|id| after.item(*id))
        .map(|item| after.ref_of(item))
        .collect();
    let inner = path.get(1..path.len().saturating_sub(1)).unwrap_or(&[]);
    let mut steps = vec![handle.to_owned()];
    steps.extend(
        inner
            .iter()
            .filter_map(|id| after.item(*id))
            .map(|item| item.nr().to_string()),
    );
    steps.push(handle.to_owned());
    let err = Error::new(
        Code::Cycle,
        format!("{} would be a dependency cycle", steps.join(" → ")),
    );
    let err = match refs.first() {
        Some(first) => err.with_item(first.clone()),
        None => err,
    };
    err.with_field("path", Field::Refs(refs))
}
