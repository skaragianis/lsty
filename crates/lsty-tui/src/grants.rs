//! The ids that a draft means to give to the items it waits for: made
//! when a candidate of `dep:` is accepted, written on commit.

use lsty_core::errs::{self, Code, Field, Nr};
use lsty_core::list::{ItemId, Snapshot};
use lsty_core::ops::{self, Grant};
use lsty_core::todotxt::Line;

use crate::insert::Draft;
use crate::model::Model;
use crate::track::Remap;

/// The key whose value names an item the text waits for.
pub(crate) const DEP: &str = "dep:";

impl Draft {
    /// The grants whose id the text still waits for.
    pub(crate) fn live_grants(&self, text: &str) -> Vec<Grant> {
        let line = Line::parse(text.as_bytes());
        let tags = line.tags();
        self.comp
            .grants
            .iter()
            .filter(|g| tags.deps.contains(&g.id().as_bytes()))
            .cloned()
            .collect()
    }

    /// Answers a refusal that names the id another writer gave a
    /// prerequisite: the draft waits for that id now.
    pub(crate) fn take_id(&mut self, e: &errs::Error) {
        let (Some(id), Some(chosen)) =
            (text_field(e, "id"), text_field(e, "chosen"))
        else {
            return;
        };
        self.field
            .swap_token(&format!("{DEP}{chosen}"), &format!("{DEP}{id}"));
        self.comp.grants.retain(|g| g.id() != chosen);
    }

    /// Drops the grant of a prerequisite that another writer moved or
    /// changed, leaving its token as typed.
    pub(crate) fn drop_stale(&mut self, e: &errs::Error) {
        if !matches!(e.code(), Code::ExpectMismatch | Code::NoItem) {
            return;
        }
        if let Some(chosen) = text_field(e, "chosen") {
            self.comp.grants.retain(|g| g.id() != chosen);
        }
    }

    /// Follows the item of each grant to its new line and drops the
    /// grants whose line is gone.
    pub(crate) fn follow_grants(&mut self, moved: &Remap, snap: &Snapshot) {
        let grants = std::mem::take(&mut self.comp.grants);
        self.comp.grants = grants
            .into_iter()
            .filter_map(|mut g| {
                let at = (errs::Source::Todo, g.nr().get());
                let (source, nr) = moved.track(at)?;
                let nr = Nr::new(nr)?;
                snap.at(source, nr)?;
                g.moved_to(nr);
                Some(g)
            })
            .collect();
    }
}

/// The text of the field `key` of an error.
fn text_field(e: &errs::Error, key: &str) -> Option<String> {
    e.fields()
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| match v {
            Field::Str(s) => Some(s.clone()),
            Field::Null
            | Field::Int(_)
            | Field::Strs(_)
            | Field::Ref(_)
            | Field::Refs(_) => None,
        })
}

impl Model {
    /// Makes the id that `item` gets when the draft is written, and
    /// returns the word that waits for it.
    pub(crate) fn give_id(&mut self, item: ItemId) -> errs::Result<String> {
        let in_todo = self
            .snap
            .item(item)
            .is_some_and(|i| i.source() == errs::Source::Todo);
        let seen = in_todo.then(|| self.seen_lines());
        let Some(draft) = &mut self.edit else {
            return Ok(String::new());
        };
        let staged: Vec<String> = draft
            .comp
            .grants
            .iter()
            .map(|g| g.id().to_owned())
            .collect();
        let id = ops::fresh_id(&self.snap, &staged, self.deps.ids.as_ref())?;
        let word = format!("{DEP}{id}");
        let grant =
            seen.and_then(|seen| Grant::new(&self.snap, &seen, item, id));
        draft.comp.grants.extend(grant);
        Ok(word)
    }
}
