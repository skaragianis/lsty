//! INSERT mode: the raw line of one task, or of a new task, edited in a
//! field. A commit is one write through the text path of the
//! operations.

use lsty_core::{
    dates::Clock,
    errs::{self, Code, Nr, Source},
    keys::Action,
    list::{Snapshot, Task},
    ops::{
        self, AddParams, Blocks, Conflict, Edited, Outcome, Pin,
        Place as Where, Seen, Settle, Spot, Tracked,
    },
    todotxt::{Kind, Line},
};

use crate::{
    actions::{archived_refusal, places},
    candidates::tokens_of,
    complete::Completion,
    field::{Field, is_sep},
    grants::DEP,
    messages,
    model::Model,
    rows::Row,
    segments::{cleared, described, line_field},
    text::head,
    track::{Place, Remap},
};

/// What the status line says after a draft was thrown away.
const DISCARDED: &str = "edit discarded — u restores";

/// What the status line says when the line being edited changed on disk.
const CHANGED: &str = "this task changed on disk";

/// What a draft is the line of.
#[derive(Debug)]
enum Subject {
    /// The line of a task.
    Task {
        /// The line the draft started from: the base of a merge.
        raw: Vec<u8>,
    },
    /// A new task.
    New {
        /// The new task goes below its anchor, not above it.
        below: bool,
        /// What the new task started with.
        prefill: String,
        /// The task at the anchor waits for the new task.
        blocks: bool,
    },
}

/// A line being edited: a task's, or a new task's.
#[derive(Debug)]
pub(crate) struct Draft {
    pub(crate) field: Field,
    subject: Subject,
    /// The line of the task, or of the anchor of a new task, as last
    /// followed; none when it is gone. While the field is open the pin
    /// has it.
    place: Option<Place>,
    /// The first row of a field taller than the list that is shown.
    pub(crate) window: usize,
    /// The line of the task, or of the anchor of a new task, followed
    /// across readings while the field is open.
    pin: Option<Pin>,
    /// The popup and the ids the text waits to give.
    pub(crate) comp: Completion,
}

impl Draft {
    fn new(field: Field, place: Option<Place>, subject: Subject) -> Self {
        Self {
            field,
            subject,
            place,
            window: 0,
            pin: None,
            comp: Completion::default(),
        }
    }

    /// Whether a task is edited, and not a new task written.
    const fn is_task(&self) -> bool {
        matches!(self.subject, Subject::Task { .. })
    }

    /// Whether a new task goes below its anchor, not above it.
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

    /// The line of the task being edited, or of the anchor of a new
    /// task.
    pub(crate) fn place(&self) -> Option<Place> {
        if self.pin.is_some() {
            self.pin_place()
        } else {
            self.place
        }
    }

    /// Whether the field stands in the row of its task.
    pub(crate) fn replacing(&self) -> bool {
        self.is_task() && self.pin_is_live()
    }

    /// Whether the pin points at a line that is there.
    pub(crate) fn pin_is_live(&self) -> bool {
        self.pin_place().is_some()
    }

    /// Whether the anchor of a new task is to wait for it.
    pub(crate) const fn blocks(&self) -> bool {
        matches!(self.subject, Subject::New { blocks: true, .. })
    }

    /// Adds `word` to what a new task started with, and puts the cursor
    /// in front of it all.
    fn prefill_with(&mut self, word: &str) {
        if let Subject::New { prefill, .. } = &mut self.subject {
            prefill.push(' ');
            prefill.push_str(word);
            self.field = line_field(prefill, 0);
        }
    }

    /// Whether a new task has nothing typed beyond its prefill.
    fn blank(&self) -> bool {
        fn words(s: &str) -> impl Iterator<Item = &str> {
            s.split(is_sep).filter(|w| !w.is_empty())
        }
        let Subject::New { prefill, .. } = &self.subject else {
            return false;
        };
        let text = self.field.text();
        text.trim().is_empty() || words(&text).eq(words(prefill))
    }

    /// Moves the place to a new reading; a line that is gone is none.
    /// An open field has a pin that does this.
    fn follow(&mut self, moved: &Remap, snap: &Snapshot) {
        self.follow_grants(moved, snap);
        if self.pin.is_none() {
            self.place = self.place.and_then(|at| moved.track(at));
        }
    }

    /// Keeps the place as the pin has it, for a draft whose pin is let
    /// go.
    fn settle_place(&mut self) {
        if self.pin.is_some() {
            self.place = self.pin_place();
            self.pin = None;
        }
    }
}

/// The place of the first line an operation wrote.
pub(crate) fn first_touched(outcome: &Outcome) -> Option<Place> {
    places(outcome.after(), outcome.tasks()).first().copied()
}

impl Model {
    /// `edit_description`, `edit_append`, `edit_line_start` and
    /// `change_description`: opens the field on the line of the task
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
        let Some(task) = self.snap.task(row.id) else {
            return;
        };
        let raw = self.snap.line(task).raw().to_vec();
        let at = row.place();
        let text = errs::lossy(&raw);
        let field = field_for(action, &text);
        self.begin(Draft::new(field, Some(at), Subject::Task { raw }));
    }

    /// `insert_below` and `insert_above`: opens the field of a new task
    /// next to the one under the cursor, or at the end of the todo file
    /// when that is archived or there is none.
    pub(crate) fn open_new(&mut self, below: bool) {
        let anchor = self
            .rows
            .get(self.cursor)
            .map(Row::place)
            .filter(|(source, _)| *source == Source::Todo);
        self.open_new_at(anchor, below);
    }

    /// `insert_dependent`: opens the field of a new task below the one
    /// under the cursor, with the `dep:` that makes it wait for that
    /// one. Where `insert_below` has no anchor, it is `insert_below`.
    /// An archived task is the prerequisite only with an id, as for
    /// `dep`, and the new task goes at the end of the todo file.
    pub(crate) fn open_dependent(&mut self) {
        let Some(task) = self
            .rows
            .get(self.cursor)
            .and_then(|row| self.snap.task(row.id))
        else {
            return self.open_new(true);
        };
        let task_id = task.task_id();
        let id = self.snap.id_of(task).map(errs::lossy);
        if id.is_none() && task.source() == Source::Done {
            let name = messages::name_of(self.snap.line(task));
            return self.alert(messages::archived_no_id(&name));
        }
        self.open_new(true);
        let word = match id {
            Some(id) => Ok(format!("{DEP}{id}")),
            None => self.give_id(task_id),
        };
        match word {
            Ok(word) => {
                if let Some(draft) = &mut self.edit {
                    draft.prefill_with(&word);
                }
            }
            Err(e) => self.fail(&e),
        }
    }

    /// `insert_prerequisite`: opens the field of a new task above the
    /// one under the cursor, which waits for it once it is written.
    /// Where `insert_above` has no anchor, it is `insert_above`.
    pub(crate) fn open_prerequisite(&mut self) {
        let chosen = self.rows.get(self.cursor).map(|r| self.chosen(&[r.id]));
        if let Some(text) = chosen.as_deref().and_then(archived_refusal) {
            return self.alert(text);
        }
        self.open_new(false);
        if let Some(Subject::New { blocks, .. }) =
            self.edit.as_mut().map(|d| &mut d.subject)
        {
            *blocks = true;
        }
    }

    /// The task at the anchor of the open field.
    fn anchor_task(&self) -> Option<&Task> {
        let (source, nr) = self.edit.as_ref()?.place()?;
        self.snap.at(source, nr)
    }

    /// What the status line says while the field of a new task will
    /// make the task at its anchor wait: the start of its text.
    pub(crate) fn blocking(&self) -> Option<String> {
        self.edit.as_ref().filter(|d| d.blocks())?;
        let task = self.anchor_task()?;
        let name = messages::name_of(self.snap.line(task));
        Some(format!("blocks {}", head(&name, 24)))
    }

    fn open_new_at(&mut self, anchor: Option<Place>, below: bool) {
        let prefill = self.prefill(anchor);
        let field = line_field(&prefill, 0);
        self.begin(Draft::new(
            field,
            anchor,
            Subject::New {
                below,
                prefill,
                blocks: false,
            },
        ));
    }

    /// What a new task starts with: the projects (with `inherit_project`)
    /// and then the contexts (with `inherit_context`) of the anchor task.
    fn prefill(&self, anchor: Option<Place>) -> String {
        let Some(line) = anchor
            .and_then(|(source, nr)| self.snap.at(source, nr))
            .map(|task| self.snap.line(task))
        else {
            return String::new();
        };
        let config = &self.deps.config;
        let mut out = String::new();
        for (kind, on) in [
            (Kind::Project, config.inherit_project),
            (Kind::Context, config.inherit_context),
        ] {
            for name in tokens_of(line, kind).filter(|_| on) {
                out.push(' ');
                out.push_str(&name);
            }
        }
        out
    }

    fn begin(&mut self, mut draft: Draft) {
        self.kept = None;
        draft.pin = self.pin_for(&draft);
        self.edit = Some(draft);
        self.reset_keys();
        self.scroll();
    }

    /// The pin of the line a draft edits, or of the anchor of a new
    /// task.
    fn pin_for(&self, draft: &Draft) -> Option<Pin> {
        let (source, nr) = draft.place()?;
        if let Subject::Task { raw } = &draft.subject {
            return Some(Pin::new(nr, raw.clone(), Settle::LineOrBlank));
        }
        let task = self.snap.at(source, nr)?;
        let raw = self.snap.line(task).raw().to_vec();
        Some(Pin::new(task.nr(), raw, Settle::Line))
    }

    pub(crate) fn end_edit(&mut self) {
        self.edit = None;
        self.reset_keys();
        self.scroll();
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the field takes only the editing actions"
    )]
    pub(crate) fn edit_action(&mut self, action: Action) {
        let width = self.field_box().1;
        match action {
            // A new task with nothing typed: end INSERT mode, write and
            // keep nothing.
            Action::Commit | Action::Cancel
                if self.edit.as_ref().is_some_and(Draft::blank) =>
            {
                self.end_edit();
            }
            Action::Commit => self.commit(),
            Action::Cancel => self.cancel(),
            Action::Accept => self.take_candidate(),
            Action::Close => self.close_popup(),
            Action::Next => self.step_popup(1),
            Action::Prev => self.step_popup(-1),
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
    /// `keep_adding`, a new task written opens the field of another below
    /// it.
    fn commit(&mut self) {
        let Some(draft) = &self.edit else {
            return;
        };
        let text = draft.field.text();
        match &draft.subject {
            Subject::Task { raw } if text == errs::lossy(raw) => {
                self.end_edit();
            }
            Subject::Task { .. } => self.commit_old(&text),
            Subject::New { .. } => self.commit_new(text),
        }
    }

    fn commit_old(&mut self, text: &str) {
        let Some(draft) = &self.edit else {
            return;
        };
        let Some(pin) = draft.pin.clone() else {
            self.alert(messages::GONE.to_owned());
            return;
        };
        let grants = draft.live_grants(text);
        let tracked = Tracked::new(pin, Seen::of(&self.snap));
        let result = self.attempt(|env| {
            ops::edit_pinned(env, &tracked, text.as_bytes(), &grants)
        });
        match result {
            Ok((Edited::Written(outcome), clock)) => {
                self.finish(Ok((*outcome, clock)));
            }
            Ok((Edited::Conflict(conflict), _)) => self.rebase(&conflict),
            Err(e) => {
                self.finish(Err(e));
            }
        }
    }

    fn commit_new(&mut self, text: String) {
        let Some(draft) = &self.edit else {
            return;
        };
        let place = self.add_place();
        let grants = draft.live_grants(&text);
        let blocks = if let Where::Beside { anchor, .. } = &place
            && draft.blocks()
        {
            Some(Blocks::Line(anchor.clone()))
        } else {
            None
        };
        let params = AddParams {
            texts: vec![text],
            place,
            grants,
            blocks,
            ..AddParams::default()
        };
        let result = self.attempt(|env| ops::add(env, &params));
        let added = self.finish(result);
        if let Some(at) = added.filter(|_| self.deps.config.keep_adding) {
            self.open_new_at(Some(at), true);
        }
    }

    /// Where the new task goes: next to its anchor, wherever that is
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
            let merged = errs::lossy(conflict.merged());
            draft.field = line_field(&merged, draft.field.cursor);
            if let Subject::Task { raw } = &mut draft.subject {
                *raw = conflict.disk().to_vec();
            }
            draft.comp.popup = None;
            draft.comp.under = None;
            draft.pin = draft.pin.take().map(|p| p.rebased(conflict.disk()));
        }
        self.fail_edit(&conflict.error());
    }

    /// Takes what a write did and ends the field, or shows why it was
    /// refused and keeps the field open. Gives the first place the write
    /// touched.
    fn finish(
        &mut self,
        result: errs::Result<(Outcome, Clock)>,
    ) -> Option<Place> {
        match result {
            Err(e) => {
                let e = match self.regrant(&e) {
                    Ok(true) => {
                        self.commit();
                        return None;
                    }
                    Ok(false) => e,
                    Err(other) => other,
                };
                if let Some(draft) = &mut self.edit {
                    draft.take_id(&e);
                }
                if e.text("chosen").is_some() && e.code() == Code::NoTask {
                    let gone = messages::PREREQUISITE_GONE;
                    self.alert(format!("{gone}{}", messages::DELETE_DEP));
                } else {
                    self.fail_edit(&e);
                }
                None
            }
            Ok((outcome, clock)) => {
                let first = first_touched(&outcome);
                self.adopt(outcome, clock);
                self.end_edit();
                self.cursor_to(first);
                first
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
        if let Some(draft) = &mut self.edit {
            draft.comp.popup = None;
            draft.comp.under = None;
            draft.settle_place();
        }
        self.kept = self.edit.take();
        self.reset_keys();
        self.scroll();
        self.message = Some(DISCARDED.to_owned());
    }

    /// `u` with a draft kept: opens the field again as it was.
    pub(crate) fn restore_draft(&mut self) {
        let Some(draft) = self.kept.take() else {
            return;
        };
        let at = draft.place;
        let there =
            at.is_some_and(|(source, nr)| self.snap.at(source, nr).is_some());
        if draft.is_task() && !there {
            self.alert(messages::GONE.to_owned());
            return;
        }
        self.cursor_to(at);
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
        let edited = draft.is_task();
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

    /// Puts the cursor on the row of the line at `at`, when it is shown;
    /// nothing moves without a place.
    pub(crate) fn cursor_to(&mut self, at: Option<Place>) {
        let Some(at) = at else { return };
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
        Action::EditAppend => line_field(text, usize::MAX),
        Action::EditLineStart => line_field(text, 0),
        Action::ChangeDescription => {
            let (cleared, at) = cleared(text);
            line_field(&cleared, at)
        }
        _ => described(text),
    }
}
