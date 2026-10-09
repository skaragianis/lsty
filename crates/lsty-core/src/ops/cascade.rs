//! The cascade: an id that no line carries any more is removed from the
//! `dep:` tokens of the todo file, in the same write.

use std::collections::{BTreeSet, HashMap};

use super::idrules::At;
use super::staged::Path;
use super::tokens::{cut_where, is_key};
use super::tx::Tx;
use crate::errs::Source;
use crate::todotxt::{Line, TokenView};

/// What every line with a `dep:` tag contains.
const DEP: &[u8] = b"dep:";

impl Tx {
    /// Says a message about an id the operation may remove. When the
    /// cascade unblocks dependents, the message tells which.
    pub(super) fn say_lost(&mut self, id: &[u8], message: impl Into<Vec<u8>>) {
        if !id.is_empty() {
            self.lost.push((self.messages.len(), id.to_vec()));
        }
        self.say(message);
    }

    /// The ids a line carried when read and no line will carry.
    fn lost_ids(&self) -> BTreeSet<Vec<u8>> {
        let mut lost = self.gone_ids();
        if lost.is_empty() {
            return lost;
        }
        for id in self.staged_ids() {
            lost.remove(&id);
        }
        lost
    }

    /// The ids of the lines that were removed or changed.
    fn gone_ids(&self) -> BTreeSet<Vec<u8>> {
        [Source::Todo, Source::Done]
            .into_iter()
            .flat_map(|source| self.file(source).rows().iter())
            .filter(|row| !row.added && (row.removed || row.raw != row.orig))
            .filter_map(|row| {
                let item = self.snap.item(row.was()?)?;
                self.snap.id_of(item).map(<[u8]>::to_vec)
            })
            .collect()
    }

    /// Removes the `dep:` of every lost id from the todo file, and adds
    /// the numbers of the items it freed to the messages about the ids.
    pub(super) fn cascade(&mut self) {
        let lost = self.lost_ids();
        if lost.is_empty() {
            return;
        }
        let mut freed: HashMap<Vec<u8>, Vec<String>> = HashMap::new();
        for index in self.todo.order().to_vec() {
            let Some(cut) = self.unlink(index, &lost) else {
                continue;
            };
            let at = At {
                source: Source::Todo,
                index,
            };
            let nr = self.row_nr(at).map_or(0, crate::errs::Nr::get);
            for id in cut {
                freed.entry(id).or_default().push(nr.to_string());
            }
        }
        for (at, id) in std::mem::take(&mut self.lost) {
            let Some(nrs) = freed.get(&id) else {
                continue;
            };
            if let Some(message) = self.messages.get_mut(at) {
                message.extend_from_slice(b"; unblocked ");
                message.extend_from_slice(nrs.join(", ").as_bytes());
            }
        }
    }

    /// Takes the `dep:` tokens of `lost` ids out of row `index`. Gives
    /// the ids it cut; nothing when the row had none, or when removing
    /// them would change a stamp.
    fn unlink(
        &mut self,
        index: usize,
        lost: &BTreeSet<Vec<u8>>,
    ) -> Option<BTreeSet<Vec<u8>>> {
        let row = self.todo.rows().get(index)?;
        if !row.raw.windows(DEP.len()).any(|w| w == DEP) {
            return None;
        }
        let line = Line::parse_shared(&row.raw);
        let cut: BTreeSet<Vec<u8>> = line
            .tokens()
            .filter(|t| is_lost_dep(t, lost))
            .map(|t| t.value.to_vec())
            .collect();
        if cut.is_empty() {
            return None;
        }
        let (next, _) = cut_where(&line, |t| is_lost_dep(t, lost))?;
        let row = self.todo.row_mut(index)?;
        row.raw = next.shared();
        row.path = row.path.max(Path::Action);
        let line_ref = self.todo.line_ref(index);
        self.side.push(line_ref);
        Some(cut)
    }
}

/// Whether `tok` is a `dep:` of an id in `lost`.
fn is_lost_dep(tok: &TokenView<'_>, lost: &BTreeSet<Vec<u8>>) -> bool {
    is_key(tok, b"dep") && lost.contains(tok.value)
}
