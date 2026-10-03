//! What an operation reports.

use super::env::Env;
use super::run::Sealed;
use super::staged::{LineRef, SealedFile};
use super::tx::Tx;
use super::undo::UndoRecord;
use crate::errs::{Nr, Source};
use crate::list::{Item, ItemId, Params, Snapshot};
use crate::todotxt::File;

/// The message of an operation that changed nothing and said nothing.
const NOTHING_CHANGED: &str = "nothing changed";

/// What an operation did. The ids in it belong to [`Outcome::read`]
/// (`before`) or [`Outcome::after`] (all the others), so a caller can
/// render the items from either list.
#[derive(Debug)]
pub struct Outcome {
    /// The operation's name.
    pub action: String,
    /// The lines the operation is about, as they are after it, in the
    /// order of the targets.
    pub items: Vec<ItemId>,
    /// The same lines as they were; the length and order of `items`.
    pub before: Vec<ItemId>,
    /// Lines created as a by-product, in file order.
    pub spawned: Vec<ItemId>,
    /// Lines changed as a side effect, in file order.
    pub changed: Vec<ItemId>,
    /// Whether a line the operation was not about has another number.
    pub renumbered: bool,
    /// What the operation said, then the warnings about touched items.
    pub messages: Vec<Vec<u8>>,
    /// Whether nothing was written.
    pub no_op: bool,
    /// What an undo needs; `None` for a no-op.
    pub undo: Option<UndoRecord>,
    read: Snapshot,
    after: Snapshot,
}

impl Outcome {
    /// The lists as the operation read them, which `before` points
    /// into.
    pub const fn read(&self) -> &Snapshot {
        &self.read
    }

    /// The lists as the operation left them, which `items`, `spawned`
    /// and `changed` point into.
    pub const fn after(&self) -> &Snapshot {
        &self.after
    }

    /// The items of `items`.
    pub fn items_after(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter_map(|id| self.after.item(*id))
    }

    /// The items of `before`.
    pub fn items_before(&self) -> impl Iterator<Item = &Item> {
        self.before.iter().filter_map(|id| self.read.item(*id))
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
        let Self { tx, todo, done } = self;
        let after = snapshot_after(env, &todo.bytes, &done.bytes);
        let no_op = !todo.changed && !done.changed;
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
        let undo = (!no_op).then(|| tx.undo(action, written, [&todo, &done]));
        Outcome {
            action: action.to_owned(),
            before: tx.items_before(),
            renumbered: tx.renumbered(),
            items,
            spawned,
            changed,
            messages,
            no_op,
            undo,
            read: tx.snap,
            after,
        }
    }
}

fn snapshot_after(env: &Env<'_>, todo: &[u8], done: &[u8]) -> Snapshot {
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
    /// The items the lines of `refs` are in `after`.
    fn items_after(&self, after: &Snapshot, refs: &[LineRef]) -> Vec<ItemId> {
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
            .filter_map(|r| self.file(r.source).all.get(r.index)?.item)
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
