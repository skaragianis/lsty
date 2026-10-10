//! The cycle check of the rule layer: a `dep:` that puts its item on a
//! dependency cycle is refused, or under `Rules::WarnCycles` said as a
//! warning.

use super::{
    env::Env,
    run::Rules,
    staged::{LineRef, Row},
    tx::Tx,
};
use crate::{
    errs::{Code, Error, Field, Ref, Result, Source},
    list::{ItemId, Snapshot},
    todotxt::Line,
};

/// A staged line that gained `dep:` tokens, and the ids they name.
struct DepRow {
    at: LineRef,
    added: Vec<Vec<u8>>,
}

impl Tx {
    /// Refuses every new `dep:` that closes a loop, or warns about it
    /// when the caller asked for that.
    pub(super) fn check_cycles(
        &mut self,
        env: &Env<'_>,
        rules: Rules,
    ) -> Result<()> {
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
            if rules != Rules::WarnCycles {
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
                let rows = self.file(source).rows().iter().enumerate();
                rows.map(move |(index, row)| DepRow {
                    at: LineRef { source, index },
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
    if row.removed || row.raw == row.orig {
        return Vec::new();
    }
    let was = Line::parse_shared(&row.orig);
    let have = was.tags().deps;
    let now = Line::parse_shared(&row.raw);
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
    let first = refs.first().cloned();
    first
        .into_iter()
        .fold(err, Error::with_item)
        .with_field("path", Field::Refs(refs))
}
