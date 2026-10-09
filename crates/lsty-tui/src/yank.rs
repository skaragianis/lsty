//! Yank and paste: the last yank, what a delete of a yanked line did to
//! its dependents, and the keys that put lines on the clipboard and
//! take them from it.

use std::collections::{HashMap, VecDeque};

use lsty_core::errs::Source;
use lsty_core::keys::Action;
use lsty_core::list::{ItemId, Snapshot};
use lsty_core::ops::{self, Change, ChangeKind, PasteLine, Place, Relink};
use lsty_core::todotxt::{Line, trim};

use crate::actions::Chosen;
use crate::history::Again;
use crate::insert::first_touched;
use crate::messages;
use crate::model::Model;
use crate::msg::Cmd;
use crate::track::Remap;

/// A line of the last yank, and the dependents a delete of it unlinked.
#[derive(Clone, Debug)]
struct YankLine {
    raw: Vec<u8>,
    /// The dependents a delete unlinked, followed through later writes.
    links: Vec<Relink>,
}

/// The last yank: the text put on the clipboard and its lines.
#[derive(Clone, Debug)]
pub(crate) struct Yank {
    text: Vec<u8>,
    lines: Vec<YankLine>,
}

impl Yank {
    /// The yank of the items `ids`, in the order given.
    fn of(snap: &Snapshot, ids: &[ItemId]) -> Self {
        let lines: Vec<YankLine> = ids
            .iter()
            .filter_map(|id| snap.item(*id))
            .map(|item| YankLine {
                raw: snap.line(item).raw().to_vec(),
                links: Vec::new(),
            })
            .collect();
        let raws: Vec<&[u8]> =
            lines.iter().map(|l| l.raw.as_slice()).collect();
        Self {
            text: raws.join(&b'\n'),
            lines,
        }
    }

    /// The lines as lines to paste.
    fn to_paste(&self) -> Vec<PasteLine> {
        self.lines
            .iter()
            .map(|line| PasteLine::Own {
                raw: line.raw.clone(),
                relink: line.links.clone(),
            })
            .collect()
    }

    /// Keeps with the blanked lines of `changes` that are in the yank
    /// the dependents that the changes unlinked.
    fn keep(&mut self, changes: &[Change]) {
        let unlinked = unlinked_by_id(changes);
        if unlinked.is_empty() {
            return;
        }
        let mut queued: HashMap<_, VecDeque<_>> = HashMap::new();
        for change in changes {
            if change.kind != ChangeKind::Blanked {
                continue;
            }
            let Some(links) =
                id_of(&change.before).and_then(|id| unlinked.get(&id))
            else {
                continue;
            };
            queued
                .entry(change.before.as_slice())
                .or_default()
                .push_back(links);
        }
        for line in self.lines.iter_mut().filter(|l| l.links.is_empty()) {
            let next = queued
                .get_mut(line.raw.as_slice())
                .and_then(VecDeque::pop_front);
            if let Some(links) = next {
                line.links.clone_from(links);
            }
        }
    }

    fn drop_links(&mut self) {
        for line in &mut self.lines {
            line.links.clear();
        }
    }

    /// Follows the kept dependents to their lines in a new reading.
    fn follow(&mut self, moved: &Remap) {
        for link in self.lines.iter_mut().flat_map(|l| &mut l.links) {
            link.nr = link.nr.and_then(|nr| {
                moved.track((Source::Todo, nr)).map(|(_, nr)| nr)
            });
        }
    }
}

/// The id of the line `raw`, when it has one.
fn id_of(raw: &[u8]) -> Option<Vec<u8>> {
    Line::parse(raw).tags().id.map(<[u8]>::to_vec)
}

/// For each id, the lines that the changes took `dep:` of it from.
fn unlinked_by_id(changes: &[Change]) -> HashMap<Vec<u8>, Vec<Relink>> {
    let mut by_id: HashMap<Vec<u8>, Vec<Relink>> = HashMap::new();
    for change in changes.iter().filter(|c| c.kind == ChangeKind::Changed) {
        let before = Line::parse(&change.before);
        let after = Line::parse(&change.after);
        let still = after.tags().deps;
        for id in before
            .tags()
            .deps
            .into_iter()
            .filter(|d| !still.contains(d))
        {
            by_id.entry(id.to_vec()).or_default().push(Relink {
                nr: change.nr_after,
                before: change.before.clone(),
                after: change.after.clone(),
            });
        }
    }
    by_id
}

/// What a paste puts in.
struct Pasting {
    lines: Vec<PasteLine>,
    /// The lines are the last yank's.
    used: bool,
    /// The clipboard could not be read.
    fell: bool,
}

impl Model {
    /// `yank`: copies the items to the clipboard and keeps them as the
    /// last yank. In VISUAL mode the selection ends.
    pub(crate) fn yank(&mut self, count: usize) -> Vec<Cmd> {
        let ids = self.items_of(count);
        if ids.is_empty() {
            return Vec::new();
        }
        let yank = Yank::of(&self.snap, &ids);
        let (text, n) = (yank.text.clone(), yank.lines.len());
        self.ending(&ids, |m| {
            m.clip = Some(yank);
            m.message = Some(messages::yanked(n));
            true
        });
        vec![Cmd::Copy(text)]
    }

    /// `delete`: yanks the items it deletes, once the delete is written.
    pub(crate) fn delete_items(&mut self, count: usize) -> Vec<Cmd> {
        let ids = self.items_of(count);
        if ids.is_empty() {
            return Vec::new();
        }
        let yank = Yank::of(&self.snap, &ids);
        let text = yank.text.clone();
        let was = self.clip.replace(yank);
        if self.act(Action::Delete, count) {
            return vec![Cmd::Copy(text)];
        }
        self.clip = was;
        Vec::new()
    }

    /// Keeps, with the lines of the last yank that a write blanked, the
    /// dependents that it unlinked.
    pub(crate) fn keep_links(&mut self, record: &ops::UndoRecord) {
        let Some(yank) = &mut self.clip else {
            return;
        };
        for file in record.files.iter().filter(|f| f.source == Source::Todo) {
            yank.keep(&file.changes);
        }
    }

    /// Follows the kept dependents to their lines in a new reading.
    pub(crate) fn remap_clip(&mut self, moved: &Remap) {
        if let Some(yank) = &mut self.clip {
            yank.follow(moved);
        }
    }

    /// `paste_below` and `paste_above`: puts the clipboard's lines next
    /// to the item under the cursor, in one write.
    pub(crate) fn paste_items(&mut self, action: Action) {
        let Some(source) = self.paste_source() else {
            self.message = Some(messages::NOTHING_TO_PASTE.to_owned());
            return;
        };
        let place = self.paste_place(action);
        let result =
            self.attempt_soft(|env| ops::paste(env, &source.lines, &place));
        let (outcome, clock) = match result {
            Ok(done) => done,
            Err(e) => return self.fail(&e),
        };
        let first = first_touched(&outcome);
        self.adopt(outcome, clock);
        self.again = Some(Again::Item { action, count: 0 });
        if source.used
            && let Some(yank) = &mut self.clip
        {
            yank.drop_links();
        }
        if source.fell
            && let Some(message) = &mut self.message
        {
            message.push_str("; ");
            message.push_str(messages::UNREADABLE);
        }
        self.cursor_to(first);
    }

    /// Where the lines go: next to the item under the cursor, or at the
    /// end of the todo file when there is none to go next to.
    fn paste_place(&self, action: Action) -> Place {
        let Some(row) = self.rows.get(self.cursor) else {
            return Place::End;
        };
        if row.source == Source::Done {
            return Place::End;
        }
        let Some(item) = self.chosen(&[row.id]).first().map(Chosen::target)
        else {
            return Place::End;
        };
        if action == Action::PasteAbove {
            Place::Before(item)
        } else {
            Place::After(item)
        }
    }

    /// What is pasted: the lines of the last yank, or the foreign text
    /// of the clipboard. `None` when there is nothing to paste.
    fn paste_source(&self) -> Option<Pasting> {
        let read = self.deps.clipboard.read().ok();
        let yanked = |fell| {
            self.clip.as_ref().map(|y| Pasting {
                lines: y.to_paste(),
                used: true,
                fell,
            })
        };
        let Some(text) = read else {
            return yanked(true);
        };
        if self.clip.as_ref().is_some_and(|y| y.text == text) {
            return yanked(false);
        }
        if trim(&text).is_empty() {
            return None;
        }
        Some(Pasting {
            lines: vec![PasteLine::Foreign(text)],
            used: false,
            fell: false,
        })
    }
}
