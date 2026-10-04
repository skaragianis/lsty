//! INSERT mode: the raw line of one item, or of a new item, edited in a
//! field. A commit is one write through the text path of the
//! operations.

use lsty_core::dates::Clock;
use lsty_core::errs::{self, Code, Nr, Source};
use lsty_core::keys::{Action, Fed, Outcome as Pressed};
use lsty_core::ops::{self, AddParams, Outcome, Place as Where, Target};

use crate::actions::{Chosen, archived_refusal, places};
use crate::field::Field;
use crate::messages;
use crate::model::Model;
use crate::segments::{cleared, described, line_field};
use crate::sidebar::{CONTEXTS, PROJECTS};
use crate::track::{Place, Remap};

/// What the status line says after a draft was thrown away.
pub(crate) const DISCARDED: &str = "edit discarded — u restores";

/// A line being edited: an item's, or a new item's.
#[derive(Debug)]
pub(crate) struct Draft {
    pub(crate) field: Field,
    /// The line as it was, when an item is edited.
    raw: Vec<u8>,
    /// The line of the item being edited; none for a new item.
    pub(crate) at: Option<Place>,
    /// The line of the item a new item goes next to.
    pub(crate) anchor: Option<Place>,
    /// A new item goes below its anchor, not above it.
    pub(crate) below: bool,
    /// What a new item started with.
    prefill: String,
    /// The line of the item is gone.
    lost: bool,
    /// The first row of a field taller than the list that is shown.
    pub(crate) window: usize,
}

impl Draft {
    /// Whether the field stands in the row of its item.
    pub(crate) const fn replacing(&self) -> bool {
        self.at.is_some()
    }

    /// Whether the line of the item is gone.
    pub(crate) const fn is_lost(&self) -> bool {
        self.lost
    }

    /// Whether a new item has nothing typed beyond its prefill.
    fn blank(&self) -> bool {
        let text = words(&self.field.text());
        text.is_empty() || text == words(&self.prefill)
    }

    /// Moves the places to a new reading. The line of an item that is
    /// gone is lost; an anchor that is gone is none.
    fn follow(&mut self, moved: &Remap) {
        if let Some(at) = self.at {
            match moved.track(at) {
                Some(now) => self.at = Some(now),
                None => self.lost = true,
            }
        }
        self.anchor = self.anchor.and_then(|at| moved.track(at));
    }
}

/// The words of `text` with one space between them.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The place of the first line an operation wrote.
fn first_touched(outcome: &Outcome) -> Option<Place> {
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
        self.begin(Draft {
            field,
            raw,
            at: Some(at),
            anchor: None,
            below: false,
            prefill: String::new(),
            lost: false,
            window: 0,
        });
    }

    /// `insert_below` and `insert_above`: opens the field of a new item
    /// next to the one under the cursor, or at the end of the todo file
    /// when that is archived or there is none.
    pub(crate) fn open_new(&mut self, below: bool) {
        let anchor = self
            .item_under_cursor()
            .filter(|(source, _)| *source == Source::Todo);
        let prefill = self.prefill();
        let mut field = Field::new(&prefill);
        field.cursor = 0;
        self.begin(Draft {
            field,
            raw: Vec::new(),
            at: None,
            anchor,
            below,
            prefill,
            lost: false,
            window: 0,
        });
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

    fn begin(&mut self, draft: Draft) {
        self.kept = None;
        self.edit = Some(draft);
        self.settle_mode();
        self.scroll();
    }

    pub(crate) fn end_edit(&mut self) {
        self.edit = None;
        self.settle_mode();
        self.scroll();
    }

    /// Takes the keys that the command table made of a stroke while the
    /// field is open.
    pub(crate) fn edit_keys(&mut self, fed: Fed) {
        if let Some(draft) = &mut self.edit {
            for stroke in fed.text {
                draft.field.typed(stroke);
            }
        }
        if let Pressed::Matched { action, .. } = fed.outcome {
            self.edit_action(action);
        }
        self.scroll();
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the field takes only the editing actions"
    )]
    fn edit_action(&mut self, action: Action) {
        let width = self.field_box().1;
        match action {
            Action::Commit => self.commit(),
            Action::Cancel => self.cancel(),
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

    /// `commit`: ends INSERT mode and writes what was typed.
    fn commit(&mut self) {
        let Some(draft) = &self.edit else {
            return;
        };
        let text = draft.field.text();
        if draft.at.is_some() {
            self.commit_old(&text);
        } else {
            self.commit_new(text);
        }
    }

    fn commit_old(&mut self, text: &str) {
        let Some(draft) = &self.edit else {
            return;
        };
        let raw = draft.raw.clone();
        if text == errs::lossy(&raw) {
            self.end_edit();
            return;
        }
        let nr = draft.at.and_then(|(_, nr)| Nr::new(nr));
        let Some(nr) = nr.filter(|_| !draft.lost) else {
            self.alert(messages::GONE.to_owned());
            return;
        };
        let result = self.attempt_soft(|env| {
            ops::edit_typed(env, nr, &raw, text.as_bytes())
        });
        self.finish(result);
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
        let params = AddParams {
            texts: vec![text],
            place,
            ..AddParams::default()
        };
        let result = self.attempt(|env| ops::add(env, &params));
        self.finish(result);
    }

    /// Where the new item goes: next to its anchor, guarded by the
    /// text the anchor has now, or at the end of the todo file.
    fn add_place(&self) -> Where {
        let Some(draft) = &self.edit else {
            return Where::End;
        };
        let target = draft.anchor.and_then(|at| self.anchor_target(at));
        match target {
            Some(target) if draft.below => Where::After(target),
            Some(target) => Where::Before(target),
            None => Where::End,
        }
    }

    fn anchor_target(&self, at: Place) -> Option<Target> {
        let item = self.snap.at(at.0, Nr::new(at.1)?)?;
        let chosen = self.chosen(&[item.item_id()]);
        chosen.first().map(Chosen::target)
    }

    /// Takes what a write did and ends the field, or shows why it was
    /// refused and keeps the field open.
    fn finish(&mut self, result: errs::Result<(Outcome, Clock)>) {
        match result {
            Err(e) => self.fail_edit(&e),
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
        let Some(hint) = e.hint().filter(|_| e.code() != Code::LockTimeout)
        else {
            return;
        };
        if let Some(message) = &mut self.message {
            message.push_str(" — ");
            message.push_str(hint);
        }
    }

    /// `cancel`: ends INSERT mode, writes nothing and keeps the draft
    /// for `u`.
    fn cancel(&mut self) {
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
        let item_gone = draft.at.is_some_and(|(source, nr)| {
            Nr::new(nr)
                .and_then(|nr| self.snap.at(source, nr))
                .is_none()
        });
        if draft.lost || item_gone {
            self.alert(messages::GONE.to_owned());
            return;
        }
        if let Some(at) = draft.at.or(draft.anchor) {
            self.cursor_to(at);
        }
        self.begin(draft);
    }

    /// Follows the lines of the drafts to a new reading; a write of the
    /// session drops the kept draft.
    pub(crate) fn remap_drafts(&mut self, moved: &Remap, wrote: bool) {
        if wrote {
            self.kept = None;
        }
        for draft in self.edit.iter_mut().chain(self.kept.iter_mut()) {
            draft.follow(moved);
        }
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
