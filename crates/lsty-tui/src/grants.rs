//! The ids that a draft means to give to the items it waits for: made
//! when a candidate of `dep:` is accepted, written on commit.

use lsty_core::{
    errs::{self, Code},
    list::{ItemId, Snapshot},
    ops::{self, Grant},
    todotxt::{Id, Line},
};

use crate::{insert::Draft, messages, model::Model, track::Remap};

/// The key whose value names an item the text waits for.
pub(crate) const DEP: &str = "dep:";

impl Draft {
    /// The ids the draft means to give.
    fn held(&self) -> Vec<Id> {
        self.comp.grants.iter().map(|g| g.id().clone()).collect()
    }

    /// The grants whose id the text still waits for.
    pub(crate) fn live_grants(&self, text: &str) -> Vec<Grant> {
        let line = Line::parse(text);
        let tags = line.tags();
        self.comp
            .grants
            .iter()
            .filter(|g| tags.deps.contains(&g.id().as_str().as_bytes()))
            .cloned()
            .collect()
    }

    /// Answers a refusal that names the id another writer gave a
    /// prerequisite: the draft waits for that id now.
    pub(crate) fn take_id(&mut self, e: &errs::Error) {
        let (Some(id), Some(chosen)) = (e.text("id"), e.text("chosen")) else {
            return;
        };
        self.field
            .swap_token(&format!("{DEP}{chosen}"), &format!("{DEP}{id}"));
        self.comp.grants.retain(|g| g.id().as_str() != chosen);
    }

    /// Follows the item of each grant to its new line. A grant whose
    /// line is gone stays, so that the commit says so.
    pub(crate) fn follow_grants(&mut self, moved: &Remap, snap: &Snapshot) {
        for g in &mut self.comp.grants {
            let at = (errs::Source::Todo, g.nr());
            if let Some((source, nr)) = moved.track(at)
                && snap.at(source, nr).is_some()
            {
                g.moved_to(nr);
            }
        }
    }
}

impl Model {
    /// Answers a refusal that names an id another writer took from a
    /// grant of the draft: the grant gets a fresh id, in the draft and
    /// in the grant. Whether it did.
    pub(crate) fn regrant(&mut self, e: &errs::Error) -> errs::Result<bool> {
        let Some(taken) =
            e.text("taken").filter(|_| e.code() == Code::IdTaken)
        else {
            return Ok(false);
        };
        let Some(draft) = &mut self.edit else {
            return Ok(false);
        };
        let held = draft.held();
        let Some(grant) = draft
            .comp
            .grants
            .iter_mut()
            .find(|g| g.id().as_str() == taken)
        else {
            return Ok(false);
        };
        let id = ops::fresh_id(&self.snap, &held, self.deps.ids.as_ref())?;
        draft
            .field
            .swap_token(&format!("{DEP}{taken}"), &format!("{DEP}{id}"));
        grant.set_id(id);
        Ok(true)
    }

    /// Makes the id that `item` gets when the draft is written, and
    /// returns the word that waits for it.
    pub(crate) fn give_id(&mut self, item: ItemId) -> errs::Result<String> {
        let seen = self.seen_lines();
        let Some(draft) = &mut self.edit else {
            return Ok(String::new());
        };
        let id =
            ops::fresh_id(&self.snap, &draft.held(), self.deps.ids.as_ref())?;
        let word = format!("{DEP}{id}");
        let grant = Grant::new(&self.snap, &seen, item, id)
            .ok_or_else(|| errs::Error::new(Code::NoItem, messages::GONE))?;
        draft.comp.grants.push(grant);
        Ok(word)
    }
}
