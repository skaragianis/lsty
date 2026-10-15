//! The cycle check of the rule layer: a `dep:` that puts its task on a
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
    list::{Snapshot, TaskId},
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
            if rules != Rules::WarnCycles {
                return Err(cycle_error(&after, &path));
            }
            let handle = self.row_handle(row.at);
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

    /// The loop through the row's task that one of its new `dep:`
    /// tokens closes: the task, then the tasks around back to it.
    fn loop_of(&self, after: &Snapshot, row: &DepRow) -> Option<Vec<TaskId>> {
        let line = self.file(row.at.source).line_ref(row.at.index);
        let found = self.tasks_after(after, &[line]);
        let id = *found.first()?;
        let task = after.task(id)?;
        for &pre in task.blockers() {
            let named = after.task(pre).and_then(|p| after.id_of(p));
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

/// The refusal, with the numbers the lines will have.
fn cycle_error(after: &Snapshot, path: &[TaskId]) -> Error {
    let refs: Vec<Ref> = path
        .iter()
        .filter_map(|id| after.task(*id))
        .map(|task| after.ref_of(task))
        .collect();
    let steps: Vec<String> = refs.iter().map(|r| r.nr.to_string()).collect();
    let mut err = Error::new(
        Code::Cycle,
        format!("{} would be a dependency cycle", steps.join(" → ")),
    );
    if let Some(first) = refs.first() {
        err = err.with_task(first.clone());
    }
    err.with_field("path", Field::Refs(refs))
}
