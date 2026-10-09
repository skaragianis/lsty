//! INSERT mode: the raw line of one item, or of a new item, edited in a
//! field. A commit is one write through the text path of the
//! operations.

use lsty_core::dates::Clock;
use lsty_core::errs::{self, Code, Nr, Source};
use lsty_core::keys::Action;
use lsty_core::list::Snapshot;
use lsty_core::ops::{
    self, AddParams, Conflict, Edited, Outcome, Pin, Place as Where, Seen,
    Settle, Spot, Tracked,
};
use lsty_core::todotxt::Line;

use crate::actions::{archived_refusal, places};
use crate::complete::Completion;
use crate::field::Field;
use crate::messages;
use crate::model::Model;
use crate::segments::{cleared, described, line_field};
use crate::sidebar::{CONTEXTS, PROJECTS};
use crate::track::{Place, Remap};

/// What the status line says after a draft was thrown away.
pub(crate) const DISCARDED: &str = "edit discarded — u restores";

/// What the status line says when the line being edited changed on disk.
const CHANGED: &str = "this item changed on disk";

/// What a draft is the line of.
#[derive(Debug)]
enum Subject {
    /// The line of an item.
    Item {
        /// The line of the item, as last followed; while the field is
        /// open the pin has it.
        at: Place,
        /// The line the draft started from: the base of a merge.
        raw: Vec<u8>,
        /// The line of the item is gone (a draft that was set aside).
        lost: bool,
    },
    /// A new item.
    New {
        /// The line of the item the new one goes next to, as last
        /// followed; while the field is open the pin has it.
        anchor: Option<Place>,
        /// The new item goes below its anchor, not above it.
        below: bool,
        /// What the new item started with.
        prefill: String,
    },
}

/// A line being edited: an item's, or a new item's.
#[derive(Debug)]
pub(crate) struct Draft {
    pub(crate) field: Field,
    subject: Subject,
    /// The first row of a field taller than the list that is shown.
    pub(crate) window: usize,
    /// The line of the item, or of the anchor of a new item, followed
    /// across readings while the field is open.
    pub(crate) pin: Option<Pin>,
    /// The popup and the ids the text waits to give.
    pub(crate) comp: Completion,
}

impl Draft {
    /// A draft of the line of the item at `at`.
    fn of_item(field: Field, at: Place, raw: Vec<u8>) -> Self {
        Self::with(
            field,
            Subject::Item {
                at,
                raw,
                lost: false,
            },
        )
    }

    /// A draft of a new item next to `anchor`.
    fn of_new(
        field: Field,
        anchor: Option<Place>,
        below: bool,
        prefill: String,
    ) -> Self {
        let new = Subject::New {
            anchor,
            below,
            prefill,
        };
        Self::with(field, new)
    }

    fn with(field: Field, subject: Subject) -> Self {
        Self {
            field,
            subject,
            window: 0,
            pin: None,
            comp: Completion::default(),
        }
    }

    /// Whether an item is edited, and not a new item written.
    pub(crate) const fn is_item(&self) -> bool {
        matches!(self.subject, Subject::Item { .. })
    }

    /// Whether a new item goes below its anchor, not above it.
    pub(crate) const fn below(&self) -> bool {
        matches!(self.subject, Subject::New { below: true, .. })
    }

    /// The place the pin points at, when a line is there.
    fn pin_place(&self) -> Option<Place> {
        let Some(Spot::Live(at)) = self.pin.as_ref().map(Pin::spot) else {
            return None;
        };
        Some((Source::Todo, Nr::from_index(at)?))
    }

    /// The line of the item being edited; none for a new item.
    pub(crate) fn item_place(&self) -> Option<Place> {
        match &self.subject {
            Subject::Item { at, .. } => Some(self.pin_place().unwrap_or(*at)),
            Subject::New { .. } => None,
        }
    }

    /// The line of the item a new item goes next to.
    pub(crate) fn anchor_place(&self) -> Option<Place> {
        match &self.subject {
            Subject::Item { .. } => None,
            Subject::New { anchor, .. } if self.pin.is_none() => *anchor,
            Subject::New { .. } => self.pin_place(),
        }
    }

    /// Whether the field stands in the row of its item.
    pub(crate) fn replacing(&self) -> bool {
        self.is_item() && self.pin_is_live()
    }

    /// Whether the pin points at a line that is there.
    pub(crate) fn pin_is_live(&self) -> bool {
        self.pin
            .as_ref()
            .map(Pin::spot)
            .is_some_and(|spot| matches!(spot, Spot::Live(_)))
    }

    /// Whether a new item has nothing typed beyond its prefill.
    fn blank(&self) -> bool {
        let text = words(&self.field.text());
        let prefill = match &self.subject {
            Subject::New { prefill, .. } => words(prefill),
            Subject::Item { .. } => String::new(),
        };
        text.is_empty() || text == prefill
    }

    /// Moves the places to a new reading. The line of an item that is
    /// gone is lost; an anchor that is gone is none. An open field has
    /// a pin that does this.
    fn follow(&mut self, moved: &Remap, snap: &Snapshot) {
        self.follow_grants(moved, snap);
        if self.pin.is_some() {
            return;
        }
        match &mut self.subject {
            Subject::Item { at, lost, .. } => match moved.track(*at) {
                Some(now) => *at = now,
                None => *lost = true,
            },
            Subject::New { anchor, .. } => {
                *anchor = anchor.and_then(|at| moved.track(at));
            }
        }
    }

    /// Keeps the places as the pin has them, for a draft whose pin is
    /// let go.
    fn settle_places(&mut self, replacing: bool) {
        let place = self.pin_place();
        match &mut self.subject {
            Subject::Item { at, lost, .. } => {
                if let Some(now) = place {
                    *at = now;
                }
                *lost = !replacing;
            }
            Subject::New { anchor, .. } => {
                if self.pin.is_some() {
                    *anchor = place;
                }
            }
        }
    }
}

/// The words of `text` with one space between them.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The place of the first line an operation wrote.
pub(crate) fn first_touched(outcome: &Outcome) -> Option<Place> {
    places(outcome.after(), outcome.items()).first().copied()
}

impl Model {
    /// The place of the item under the cursor.
    fn item_under_cursor(&self) -> Option<Place> {
        let row = self.rows.get(self.cursor)?;
        Some((row.source, row.nr))
    }

    /// `edit_description`, `edit_append`, `edit_line_start` and
    /// `change_description`: opens the field on the line of the item
    /// under the cursor.
    pub(crate) fn open_edit(&mut self, action: Action) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        let chosen = self.chosen(&[row.id]);
        if let Some(text) = archived_refusal(&chosen) {
            self.alert(text);
            return;
        }
        let Some(item) = self.snap.item(row.id) else {
            return;
        };
        let raw = self.snap.line(item).raw().to_vec();
        let at = (row.source, row.nr);
        let text = errs::lossy(&raw);
        let field = field_for(action, &text);
        self.begin(Draft::of_item(field, at, raw));
    }

    /// `insert_below` and `insert_above`: opens the field of a new item
    /// next to the one under the cursor, or at the end of the todo file
    /// when that is archived or there is none.
    pub(crate) fn open_new(&mut self, below: bool) {
        let anchor = self
            .item_under_cursor()
            .filter(|(source, _)| *source == Source::Todo);
        self.open_new_at(anchor, below);
    }

    fn open_new_at(&mut self, anchor: Option<Place>, below: bool) {
        let prefill = self.prefill();
        let mut field = Field::new(&prefill);
        field.cursor = 0;
        self.begin(Draft::of_new(field, anchor, below, prefill));
    }

    /// What a new item starts with while a project or a context is
    /// chosen in the sidebar.
    fn prefill(&self) -> String {
        let tags: Vec<String> = [PROJECTS, CONTEXTS]
            .into_iter()
            .filter_map(|i| {
                let panel = self.panels.get(i)?;
                let name = panel.active.as_deref()?;
                Some(format!("{}{}", panel.sigil(), errs::lossy(name)))
            })
            .collect();
        if tags.is_empty() {
            String::new()
        } else {
            format!(" {}", tags.join(" "))
        }
    }

    fn begin(&mut self, mut draft: Draft) {
        self.kept = None;
        draft.pin = self.pin_for(&draft);
        self.edit = Some(draft);
        self.settle_mode();
        self.scroll();
    }

    /// The pin of the line a draft edits, or of the anchor of a new
    /// item.
    fn pin_for(&self, draft: &Draft) -> Option<Pin> {
        if let Subject::Item { at, raw, .. } = &draft.subject {
            return Some(Pin::new(at.1, raw.clone(), Settle::LineOrBlank));
        }
        let (source, nr) = draft.anchor_place()?;
        let item = self.snap.at(source, nr)?;
        let raw = self.snap.line(item).raw().to_vec();
        Some(Pin::new(item.nr(), raw, Settle::Line))
    }

    pub(crate) fn end_edit(&mut self) {
        self.edit = None;
        self.settle_mode();
        self.scroll();
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the field takes only the editing actions"
    )]
    pub(crate) fn edit_action(&mut self, action: Action) {
        let width = self.field_box().1;
        match action {
            Action::Commit => self.commit(),
            Action::Cancel => self.cancel(),
            Action::Accept | Action::Close | Action::Next | Action::Prev => {
                self.popup_action(action);
            }
            Action::CursorRowUp | Action::CursorRowDown => {
                if let Some(draft) = &mut self.edit {
                    draft.field.vertical(width, action == Action::CursorRowUp);
                }
            }
            _ => {
                if let Some(draft) = &mut self.edit {
                    draft.field.act(action);
                }
            }
        }
    }

    /// `commit`: ends INSERT mode and writes what was typed; with
    /// `keep_adding`, a new item written opens the field of another below
    /// it.
    fn commit(&mut self) {
        let Some(draft) = &self.edit else {
            return;
        };
        let text = draft.field.text();
        if draft.is_item() {
            self.commit_old(&text);
        } else {
            self.commit_new(text);
        }
    }

    fn commit_old(&mut self, text: &str) {
        let Some(draft) = &self.edit else {
            return;
        };
        let Subject::Item { raw, .. } = &draft.subject else {
            return;
        };
        if text == errs::lossy(raw) {
            self.end_edit();
            return;
        }
        let Some(pin) = draft.pin.clone() else {
            self.alert(messages::GONE.to_owned());
            return;
        };
        let grants = draft.live_grants(text);
        let tracked = Tracked::new(pin, Seen::of(&self.snap));
        let result = self.attempt_soft(|env| {
            ops::edit_pinned(env, &tracked, text.as_bytes(), &grants)
        });
        match result {
            Ok((Edited::Written(outcome), clock)) => {
                self.finish(Ok((*outcome, clock)));
            }
            Ok((Edited::Conflict(conflict), _)) => self.rebase(&conflict),
            Err(e) => self.finish(Err(e)),
        }
    }

    fn commit_new(&mut self, text: String) {
        let Some(draft) = &self.edit else {
            return;
        };
        if draft.blank() {
            self.end_edit();
            return;
        }
        let place = self.add_place();
        let grants = draft.live_grants(&text);
        let params = AddParams {
            texts: vec![text],
            place,
            grants,
            ..AddParams::default()
        };
        let result = self.attempt(|env| ops::add(env, &params));
        let added = result.as_ref().ok().and_then(|(o, _)| first_touched(o));
        self.finish(result);
        if let Some(at) = added.filter(|_| self.deps.config.keep_adding) {
            self.open_new_at(Some(at), true);
        }
    }

    /// Where the new item goes: next to its anchor, wherever that is
    /// now, or at the end of the todo file.
    fn add_place(&self) -> Where {
        let Some(draft) = &self.edit else {
            return Where::End;
        };
        match draft.pin.clone() {
            Some(pin) => Where::Beside {
                anchor: Tracked::new(pin, Seen::of(&self.snap)),
                below: draft.below(),
            },
            None => Where::End,
        }
    }

    /// A draft that clashes with the line on disk stays open as the
    /// merged line, against the line on disk now.
    fn rebase(&mut self, conflict: &Conflict) {
        if let Some(draft) = &mut self.edit {
            let mut field = line_field(&errs::lossy(conflict.merged()), 0);
            field.rest_at(draft.field.cursor);
            draft.field = field;
            if let Subject::Item { raw, .. } = &mut draft.subject {
                *raw = conflict.disk().to_vec();
            }
            draft.comp.popup = None;
            draft.pin = draft.pin.take().map(|p| p.rebased(conflict.disk()));
        }
        self.fail_edit(&conflict.error());
    }

    /// Takes what a write did and ends the field, or shows why it was
    /// refused and keeps the field open.
    fn finish(&mut self, result: errs::Result<(Outcome, Clock)>) {
        match result {
            Err(e) => {
                if let Some(draft) = &mut self.edit {
                    draft.take_id(&e);
                    draft.drop_stale(&e);
                }
                self.fail_edit(&e);
            }
            Ok((outcome, clock)) => {
                let first = first_touched(&outcome);
                self.adopt(outcome, clock);
                self.end_edit();
                if let Some(at) = first {
                    self.cursor_to(at);
                }
            }
        }
    }

    /// Shows why a draft was refused, with what to do about it.
    fn fail_edit(&mut self, e: &errs::Error) {
        self.fail(e);
        let Some(hint) = e
            .hint()
            .filter(|h| !h.is_empty() && e.code() != Code::LockTimeout)
        else {
            return;
        };
        if let Some(message) = &mut self.message {
            message.push_str(" — ");
            message.push_str(hint);
        }
    }

    /// `cancel`: ends INSERT mode, writes nothing and keeps the draft
    /// for `u`; a new item with nothing typed is not worth keeping.
    fn cancel(&mut self) {
        if self
            .edit
            .as_ref()
            .is_some_and(|d| !d.is_item() && d.blank())
        {
            self.end_edit();
            return;
        }
        if let Some(draft) = &mut self.edit {
            draft.comp.popup = None;
            draft.settle_places(draft.replacing() || !draft.is_item());
            draft.pin = None;
        }
        self.kept = self.edit.take();
        self.settle_mode();
        self.scroll();
        self.message = Some(DISCARDED.to_owned());
    }

    /// `u` with a draft kept: opens the field again as it was.
    pub(crate) fn restore_draft(&mut self) {
        let Some(draft) = self.kept.take() else {
            return;
        };
        let (at, lost) = match &draft.subject {
            Subject::Item { at, lost, .. } => (Some(*at), *lost),
            Subject::New { anchor, .. } => (*anchor, false),
        };
        let item_gone = draft.is_item()
            && at.is_some_and(|(source, nr)| {
                self.snap.at(source, nr).is_none()
            });
        if lost || item_gone {
            self.alert(messages::GONE.to_owned());
            return;
        }
        if let Some(at) = at {
            self.cursor_to(at);
        }
        self.begin(draft);
    }

    /// Follows the lines of the drafts to a new reading; a write of the
    /// session drops the kept draft.
    pub(crate) fn remap_drafts(
        &mut self,
        moved: &Remap,
        old: &Snapshot,
        wrote: bool,
    ) {
        if wrote {
            self.kept = None;
        } else {
            self.follow_pin(moved, old);
        }
        for draft in self.edit.iter_mut().chain(self.kept.iter_mut()) {
            draft.follow(moved, &self.snap);
        }
        self.refresh_popup();
    }

    /// Moves the pin of the open field to the new reading, and says so
    /// once when the line being edited itself changed.
    fn follow_pin(&mut self, moved: &Remap, old: &Snapshot) {
        let Some(draft) = &mut self.edit else {
            return;
        };
        let Some(was) = draft.pin.take() else {
            return;
        };
        let (before, now) = (&old.todo().lines, &self.snap.todo().lines);
        let next = was.follow_mapped(moved.todo_map(), before, now);
        let edited = draft.is_item();
        let changed = match (was.spot(), next.spot()) {
            (Spot::Live(from), Spot::Live(to)) if edited => {
                let (was_line, line) = (before.get(from), now.get(to));
                was_line.map(Line::raw) != line.map(Line::raw)
                    && line.map(Line::raw) != Some(was.base())
            }
            _ => false,
        };
        draft.pin = Some(next);
        if changed {
            self.message = Some(CHANGED.to_owned());
        }
    }

    /// The place of the todo file that the pin of the open field points
    /// at, or just after it.
    pub(crate) fn pin_spot(&self) -> Option<Place> {
        let pin = self.edit.as_ref()?.pin.as_ref()?;
        let index = match pin.spot() {
            Spot::Live(at) | Spot::Blank(at) => at,
            Spot::After(after) => after.map_or(0, |at| at + 1),
        };
        Some((Source::Todo, Nr::from_index(index)?))
    }

    /// Puts pasted text into the field.
    pub(crate) fn paste(&mut self, text: &str) {
        if let Some(draft) = &mut self.edit {
            draft.field.paste(text);
            self.scroll();
        }
    }

    /// Puts the cursor on the row of the line at `at`, when it is shown.
    pub(crate) fn cursor_to(&mut self, at: Place) {
        if let Some(i) = self.row_at(at) {
            self.cursor = i;
        }
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        self.scroll();
    }
}

/// The field of a line as an action that opens it wants it.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "the other actions put the cursor at the description"
)]
fn field_for(action: Action, text: &str) -> Field {
    match action {
        Action::EditAppend => {
            let mut field = described(text);
            field.cursor = field.text.len();
            field
        }
        Action::EditLineStart => {
            let mut field = described(text);
            field.act(Action::LineStart);
            field
        }
        Action::ChangeDescription => {
            let (cleared, at) = cleared(text);
            line_field(&cleared, at)
        }
        _ => described(text),
    }
}
