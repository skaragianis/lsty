//! What the moves between the todo file and the done file share: pairing
//! a moved line with the copy an interrupted move left behind, and
//! putting a line into the done file.

use std::sync::Arc;

use super::{
    staged::{LineRef, Path},
    tx::Tx,
};
use crate::{
    errs::{Result, Source, lossy, lossy_path},
    fsio,
    list::{Item, ItemId},
};

impl Tx {
    /// The name of the file of `source`, without its folder.
    pub(super) fn file_name(&self, source: Source) -> String {
        lossy_path(fsio::base_name(self.snap.found().path(source)))
    }

    /// The id of an item, if it has one.
    pub(super) fn id_bytes(&self, id: ItemId) -> Option<Vec<u8>> {
        let item = self.snap.item(id)?;
        self.snap.id_of(item).map(<[u8]>::to_vec)
    }

    /// The first line of `source` that carries `id` and that no other
    /// line has taken, now taken. A line without an id has no copy.
    pub(super) fn copy_in(
        &mut self,
        source: Source,
        id: ItemId,
    ) -> Option<ItemId> {
        let key = self.id_bytes(id)?;
        let found = self
            .snap
            .items_with_id(&key)
            .filter(|item| item.source() == source)
            .map(Item::item_id)
            .find(|id| !self.paired.contains(id))?;
        self.paired.insert(found);
        Some(found)
    }

    /// Makes `now` the line the operation is about in place of `old`.
    pub(super) fn retarget(&mut self, old: LineRef, now: LineRef) {
        match self.primary.iter().position(|r| *r == old) {
            Some(at) => self.primary[at] = now,
            None => self.primary.push(now),
        }
    }

    /// Says that a retry has finished what a crash left half done.
    pub(super) fn say_interrupted(&mut self, id: &[u8]) {
        let id = lossy(id);
        self.say(format!("finished an interrupted move of id:{id}"));
    }

    /// Stages `raw` as the line of the item `id` in `dest`: over the copy
    /// an interrupted move left, or else at the end of the file. The
    /// source line is blanked. Gives the new line and whether it went
    /// over a copy.
    pub(super) fn send_to(
        &mut self,
        dest: Source,
        id: ItemId,
        raw: impl Into<Arc<[u8]>>,
    ) -> Result<(LineRef, bool)> {
        let old = self.line_ref(id)?;
        let (line, over) = self.stage_in(dest, id, raw)?;
        self.retarget(old, line);
        Ok((line, over))
    }

    /// [`Tx::send_to`] without the retargeting.
    pub(super) fn stage_in(
        &mut self,
        dest: Source,
        id: ItemId,
        raw: impl Into<Arc<[u8]>>,
    ) -> Result<(LineRef, bool)> {
        let raw = raw.into();
        let over = self.copy_in(dest, id);
        let line = match over {
            Some(copy) => {
                self.set(copy, &raw, Path::Action);
                self.line_ref(copy)?
            }
            None => self.append(dest, raw, Path::Action),
        };
        self.file_mut(line.source).rows_mut()[line.index].origin = Some(id);
        self.blank(id);
        Ok((line, over.is_some()))
    }

    /// The line number a staged line has in the file as it will be
    /// written.
    pub(super) fn written_nr(&self, line: LineRef) -> usize {
        self.file(line.source)
            .position(line.index)
            .map_or(0, |at| at + 1)
    }
}
