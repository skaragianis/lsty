//! What an operation reports.

use super::env::Env;
use super::run::Sealed;
use super::staged::{LineRef, SealedFile};
use super::tx::{Rebuilt, Tx};
use super::undo::UndoRecord;
use std::sync::Arc;

use crate::errs::{Nr, Source};
use crate::list::{Item, ItemId, Params, Snapshot};
use crate::todotxt::File;

/// The message of an operation that changed nothing and said nothing.
const NOTHING_CHANGED: &str = "nothing changed";

/// What an operation did. The ids in it belong to the list the
/// operation read (`before`) or to the list it left (all the others), so
/// a caller can render the items from either list.
#[derive(Debug)]
pub struct Outcome {
    action: String,
    items: Vec<ItemId>,
    before: Vec<ItemId>,
    spawned: Vec<ItemId>,
    changed: Vec<ItemId>,
    renumbered: bool,
    messages: Vec<Vec<u8>>,
    files_changed: Vec<Source>,
    undo: Option<UndoRecord>,
    read: Snapshot,
    after: Arc<Snapshot>,
}

impl Outcome {
    /// The operation's name.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The lines the operation is about, as they are after it, in the
    /// order of the targets. They point into [`Outcome::after`].
    pub fn items(&self) -> &[ItemId] {
        &self.items
    }

    /// The primary lines as they were read, in the order of the
    /// targets; an added line has none. They point into
    /// [`Outcome::read`].
    pub fn before(&self) -> &[ItemId] {
        &self.before
    }

    /// Lines created as a by-product, in file order. They point into
    /// [`Outcome::after`].
    pub fn spawned(&self) -> &[ItemId] {
        &self.spawned
    }

    /// Lines changed as a side effect, in file order. They point into
    /// [`Outcome::after`].
    pub fn changed(&self) -> &[ItemId] {
        &self.changed
    }

    /// Whether a line the operation was not about has another number.
    pub const fn renumbered(&self) -> bool {
        self.renumbered
    }

    /// What the operation said, then the warnings about touched items.
    pub fn messages(&self) -> &[Vec<u8>] {
        &self.messages
    }

    /// The files whose bytes the operation changed, todo file first.
    pub fn files_changed(&self) -> &[Source] {
        &self.files_changed
    }

    /// Whether nothing was changed.
    pub fn no_op(&self) -> bool {
        self.files_changed.is_empty()
    }

    /// What an undo needs. `None` for a no-op, and for an operation
    /// that ran without [`Env::record_undo`](super::Env::record_undo).
    pub const fn undo(&self) -> Option<&UndoRecord> {
        self.undo.as_ref()
    }

    /// The lists as the operation read them, which `before` points
    /// into.
    pub const fn read(&self) -> &Snapshot {
        &self.read
    }

    /// The lists as the operation left them, which `items`, `spawned`
    /// and `changed` point into.
    pub fn after(&self) -> &Snapshot {
        &self.after
    }

    /// The items of `before`.
    #[cfg(test)]
    pub(super) fn items_before(&self) -> impl Iterator<Item = &Item> {
        self.before.iter().filter_map(|id| self.read.item(*id))
    }

    /// The items of `items`.
    pub fn items_after(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter_map(|id| self.after.item(*id))
    }
}

impl Sealed {
    /// Builds the result of the operation, which wrote `written`.
    pub(super) fn report(
        self,
        env: &Env<'_>,
        action: &str,
        written: &[Source],
    ) -> Outcome {
        let Self { mut tx, todo, done } = self;
        let after = tx.after_of(env, &todo.bytes, &done.bytes);
        let files_changed: Vec<Source> =
            [(Source::Todo, &todo), (Source::Done, &done)]
                .into_iter()
                .filter(|(_, file)| file.changed)
                .map(|(source, _)| source)
                .collect();
        let no_op = files_changed.is_empty();
        let items = tx.items_after(&after, &tx.primary);
        let spawned = sorted(tx.items_after(&after, &tx.spawned));
        let changed = sorted(tx.items_after(&after, &tx.side));
        let touched = [&items, &spawned, &changed]
            .into_iter()
            .flatten()
            .filter_map(|id| after.item(*id));
        let mut messages = tx.messages.clone();
        messages.extend(
            after
                .warnings_for(touched)
                .into_iter()
                .map(String::into_bytes),
        );
        if no_op && tx.messages.is_empty() {
            messages.push(NOTHING_CHANGED.as_bytes().to_vec());
        }
        let undo = (env.record_undo && !no_op)
            .then(|| tx.undo(action, written, [&todo, &done]));
        Outcome {
            action: action.to_owned(),
            before: tx.items_before(),
            renumbered: tx.renumbered(),
            items,
            spawned,
            changed,
            messages,
            files_changed,
            undo,
            read: tx.snap,
            after,
        }
    }
}

pub(super) fn snapshot_after(
    env: &Env<'_>,
    todo: &[u8],
    done: &[u8],
) -> Snapshot {
    let params = Params {
        clock: env.clock.clone(),
        stale_after: env.config.claim_stale_after,
    };
    Snapshot::build(
        env.files.clone(),
        File::parse(todo),
        File::parse(done),
        &params,
    )
}

impl Tx {
    /// The lists as they would be written now. The list is built once
    /// and kept for as long as the staged bytes stay the same.
    pub(super) fn snapshot_after(&mut self, env: &Env<'_>) -> Arc<Snapshot> {
        let (todo, done) = (self.todo.seal(), self.done.seal());
        self.after_of(env, &todo.bytes, &done.bytes)
    }

    /// The lists as `todo` and `done` would leave them.
    pub(super) fn after_of(
        &mut self,
        env: &Env<'_>,
        todo: &[u8],
        done: &[u8],
    ) -> Arc<Snapshot> {
        if let Some(kept) = &self.after
            && kept.todo == todo
            && kept.done == done
        {
            return Arc::clone(&kept.snap);
        }
        let snap = Arc::new(snapshot_after(env, todo, done));
        self.after = Some(Rebuilt {
            todo: todo.to_vec(),
            done: done.to_vec(),
            snap: Arc::clone(&snap),
        });
        snap
    }

    /// The items the lines of `refs` are in `after`.
    pub(super) fn items_after(
        &self,
        after: &Snapshot,
        refs: &[LineRef],
    ) -> Vec<ItemId> {
        let todo = self.todo.positions();
        let done = self.done.positions();
        refs.iter()
            .filter_map(|r| {
                let at = match r.source {
                    Source::Todo => &todo,
                    Source::Done => &done,
                };
                let at = at.get(r.index).copied().flatten()?;
                let item = after.at(r.source, Nr::from_index(at)?)?;
                Some(item.item_id())
            })
            .collect()
    }

    /// The items the primary lines were when read.
    fn items_before(&self) -> Vec<ItemId> {
        self.primary
            .iter()
            .filter_map(|r| self.file(r.source).all.get(r.index)?.was())
            .collect()
    }

    /// Whether a line that was read, and is not primary, has another
    /// number.
    fn renumbered(&self) -> bool {
        [Source::Todo, Source::Done].into_iter().any(|source| {
            let skip: Vec<usize> = self
                .primary
                .iter()
                .filter(|r| r.source == source)
                .map(|r| r.index)
                .collect();
            self.file(source).shifted(&skip)
        })
    }

    fn undo(
        &self,
        action: &str,
        written: &[Source],
        sealed: [&SealedFile; 2],
    ) -> UndoRecord {
        let bytes = |source| match source {
            Source::Todo => &sealed[0].bytes,
            Source::Done => &sealed[1].bytes,
        };
        UndoRecord {
            action: action.to_owned(),
            files: written
                .iter()
                .map(|&s| self.file(s).record(self.snap.file(s), bytes(s)))
                .collect(),
        }
    }
}

/// `ids` in file order, the todo file first.
fn sorted(mut ids: Vec<ItemId>) -> Vec<ItemId> {
    ids.sort();
    ids
}
