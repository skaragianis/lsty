//! Yank and paste: the last yank, what a delete of a yanked line did to
//! its dependents, and the keys that put lines on the clipboard and
//! take them from it.

use std::collections::{HashMap, HashSet, VecDeque};

use lsty_core::{
    errs::Source,
    keys::Action,
    list::{Snapshot, TaskId},
    ops::{self, Change, ChangeKind, Landing, PasteLine, Place, Relink, Seen},
    todotxt::{Line, trim},
};

use crate::{
    hints::dim,
    history::Again,
    insert::first_touched,
    messages::{self, Tone},
    model::Model,
    msg::Cmd,
    press::{Refusal, decide},
    track::{Remap, landed_at},
};

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
    /// The yank of the tasks `ids`, in the order given.
    fn of(snap: &Snapshot, ids: &[TaskId]) -> Self {
        let lines: Vec<YankLine> = ids
            .iter()
            .filter_map(|id| snap.task(*id))
            .map(|task| YankLine {
                raw: snap.line(task).raw().to_vec(),
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
    /// the dependents that the changes unlinked, and takes them from the
    /// lines of the yank that the changes bring back with their
    /// dependents linked.
    fn keep(&mut self, changes: &[Change]) {
        let unlinked = unlinked_by_id(changes);
        let relinked = relinked_ids(changes);
        let back = |c: &&Change| {
            c.kind == ChangeKind::Added
                && id_of(&c.after).is_some_and(|id| relinked.contains(&id))
        };
        for change in changes.iter().filter(back) {
            for line in self.lines.iter_mut().filter(|l| l.raw == change.after)
            {
                line.links.clear();
            }
        }
        let mut queued = queued(changes, &unlinked);
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

    /// Follows the kept dependents to their lines in a new reading. `outside`
    /// compares it with the one before, when no write of this session made
    /// it: what that moved or twinned is not written on any more.
    fn follow(&mut self, moved: &Remap, outside: Option<&Landing>) {
        for link in self.lines.iter_mut().flat_map(|l| &mut l.links) {
            let at = link.nr.map(|nr| (Source::Todo, nr));
            link.lost |= outside
                .zip(at)
                .is_some_and(|(l, at)| landed_at(l, at).is_none());
            link.nr = at.and_then(|at| moved.track(at)).map(|(_, nr)| nr);
        }
    }
}

/// The unlinked dependents of lines, by the text the lines had.
type Queued<'a> = HashMap<&'a [u8], VecDeque<&'a Vec<Relink>>>;

/// The dependents that `changes` unlinked from each blanked or removed line,
/// by the text it had, in the order of the changes.
fn queued<'a>(
    changes: &'a [Change],
    unlinked: &'a HashMap<Vec<u8>, Vec<Relink>>,
) -> Queued<'a> {
    let gone = |c: &&Change| {
        matches!(c.kind, ChangeKind::Blanked | ChangeKind::Removed)
    };
    let mut queued = Queued::new();
    for change in changes.iter().filter(gone) {
        if let Some(links) =
            id_of(&change.before).and_then(|id| unlinked.get(&id))
        {
            queued
                .entry(change.before.as_slice())
                .or_default()
                .push_back(links);
        }
    }
    queued
}

/// The id of the line `raw`, when it has one.
fn id_of(raw: &[u8]) -> Option<Vec<u8>> {
    Line::parse(raw).tags().id.map(<[u8]>::to_vec)
}

/// The ids that `from` waits on and `to` does not.
fn lost_deps(from: &[u8], to: &[u8]) -> Vec<Vec<u8>> {
    let (from, to) = (Line::parse(from), Line::parse(to));
    let (had, still) = (from.tags().deps, to.tags().deps);
    had.into_iter()
        .filter(|d| !still.contains(d))
        .map(<[u8]>::to_vec)
        .collect()
}

/// For each id, the lines that the changes took `dep:` of it from.
fn unlinked_by_id(changes: &[Change]) -> HashMap<Vec<u8>, Vec<Relink>> {
    let mut by_id: HashMap<Vec<u8>, Vec<Relink>> = HashMap::new();
    for change in changes.iter().filter(|c| c.kind == ChangeKind::Changed) {
        for id in lost_deps(&change.before, &change.after) {
            by_id.entry(id).or_default().push(Relink {
                nr: change.nr_after,
                lost: false,
                before: change.before.clone(),
                after: change.after.clone(),
            });
        }
    }
    by_id
}

/// The ids that the changes gave `dep:` of to a line.
fn relinked_ids(changes: &[Change]) -> HashSet<Vec<u8>> {
    changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Changed)
        .flat_map(|c| lost_deps(&c.after, &c.before))
        .collect()
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
    /// `yank`: copies the tasks to the clipboard and keeps them as the
    /// last yank. In VISUAL mode the selection ends.
    pub(crate) fn yank(&mut self, count: usize) -> Vec<Cmd> {
        if self.refused(Action::Yank, count) {
            return Vec::new();
        }
        let ids = self.tasks_of(count);
        let yank = Yank::of(&self.snap, &ids);
        let (text, n) = (yank.text.clone(), yank.lines.len());
        self.ending(&ids, |m| {
            m.clip = Some(yank);
            m.message = Some(messages::yanked(n));
            true
        });
        self.copy_out(text)
    }

    /// Puts `text` on the clipboard: the terminal is asked to, and the
    /// programs that write it are run; a failure of those is noted.
    pub(crate) fn copy_out(&mut self, text: Vec<u8>) -> Vec<Cmd> {
        if let Err(e) = self.deps.clipboard.write(&text) {
            self.note(e.message());
            self.tone = self.tone.max(Tone::Warning);
        }
        vec![Cmd::Copy(text)]
    }

    /// `delete`: yanks the tasks it deletes, once the delete is written.
    pub(crate) fn delete_tasks(&mut self, count: usize) -> Vec<Cmd> {
        if dim(decide(Action::Delete, self, &self.summary(count))) {
            self.act(Action::Delete, count);
            return Vec::new();
        }
        let yank = Yank::of(&self.snap, &self.tasks_of(count));
        let text = yank.text.clone();
        let was = self.clip.replace(yank);
        if self.act(Action::Delete, count) {
            return self.copy_out(text);
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
    pub(crate) fn remap_clip(
        &mut self,
        moved: &Remap,
        outside: Option<&Landing>,
    ) {
        if let Some(yank) = &mut self.clip {
            yank.follow(moved, outside);
        }
    }

    /// `paste_below` and `paste_above`: puts the clipboard's lines next
    /// to the task under the cursor, in one write.
    pub(crate) fn paste_tasks(&mut self, action: Action) {
        let Some(source) = self.paste_source() else {
            return self.refuse(Refusal::Says(messages::NO_PASTE), &[]);
        };
        let place = self.paste_place(action);
        let result =
            self.attempt(|env| ops::paste(env, &source.lines, &place));
        let (outcome, clock) = match result {
            Ok(done) => done,
            Err(e) => return self.fail(&e),
        };
        let first = first_touched(&outcome);
        self.adopt(outcome, clock);
        self.again = Some(Again::Task { action, count: 0 });
        if source.used
            && let Some(yank) = &mut self.clip
        {
            yank.drop_links();
        }
        if source.fell {
            self.note(messages::UNREADABLE);
            self.tone = self.tone.max(Tone::Warning);
        }
        self.cursor_to(first);
    }

    /// Where the lines go: next to the task under the cursor, or about
    /// where it was, or at the end of the todo file when there is none to
    /// go next to. The task is only a position, so nothing it became can
    /// refuse a paste.
    fn paste_place(&self, action: Action) -> Place {
        let task = self.rows.get(self.cursor).and_then(|row| {
            self.snap
                .task(row.id)
                .filter(|_| row.source != Source::Done)
        });
        let Some(task) = task else {
            return Place::End;
        };
        Place::Beside {
            seen: Seen::of(&self.snap),
            at: task.nr().index(),
            below: action != Action::PasteAbove,
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
