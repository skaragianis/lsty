//! INSERT mode: the raw line of one task, or of a new task, edited in a
//! field. A commit is one write through the text path of the
//! operations.

use lsty_core::{
    dates::Clock,
    errs::{self, Code, Source},
    keys::{Action, Mods, Named, Stroke},
    list::{Snapshot, Task},
    ops::{self, AddParams, Landing, Outcome, Place as Where, Seen, Target},
    todotxt::Kind,
};

use crate::{
    actions::places,
    candidates::tokens_of,
    complete::Completion,
    field::{Field, is_sep},
    grants::DEP,
    messages,
    model::Model,
    rows::Row,
    segments::{at_end, cleared, described, line_field},
    text::head,
    track::{Place, Remap, landed_at},
};

/// What the status line says after a draft was thrown away.
const DISCARDED: &str = "edit discarded — u restores";

/// What a draft is the line of.
#[derive(Debug)]
enum Subject {
    /// The line of a task.
    Task {
        /// The line the draft started from.
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
    /// The line of the task, or of the anchor of a new task, in the
    /// reading shown; once `lost`, about where it was.
    pub(crate) at: Option<Place>,
    /// The line is not the one the field opened on, so a write on it
    /// refuses: the last reading it stood in, and where.
    lost: Option<(Snapshot, Place)>,
    /// A commit was refused, so `<Esc>` drops the text now: what the status
    /// line says.
    refused: Option<&'static str>,
    /// The first row of a field taller than the list that is shown.
    pub(crate) window: usize,
    /// The popup and the ids the text waits to give.
    pub(crate) comp: Completion,
}

impl Draft {
    fn new(field: Field, at: Option<Place>, subject: Subject) -> Self {
        Self {
            field,
            subject,
            at,
            lost: None,
            refused: None,
            window: 0,
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

    /// The line of the task being edited, or of the anchor of a new
    /// task, while it is the line the field opened on.
    pub(crate) fn place(&self) -> Option<Place> {
        self.at.filter(|_| self.lost.is_none())
    }

    /// Whether the field stands in the row of its task.
    pub(crate) const fn replacing(&self) -> bool {
        self.is_task() && self.lost.is_none()
    }

    /// Whether a commit has to refuse: it would write on a line that
    /// changed.
    pub(crate) const fn refuses(&self) -> bool {
        self.lost.is_some() && (self.is_task() || self.blocks())
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

    /// Whether committing writes nothing: a new task is [`Self::blank`], or
    /// a task's line is as it opened.
    pub(crate) fn unchanged(&self) -> bool {
        self.blank()
            || matches!(&self.subject, Subject::Task { raw }
                if self.field.text() == errs::lossy(raw))
    }

    /// Takes the draft to a later reading. A line that is not kept is lost
    /// until the rule keeps it again from the last reading it stood in; a new
    /// task goes about where its anchor was.
    fn follow(
        &mut self,
        moved: &Remap,
        landing: &Landing,
        old: &Snapshot,
        now: &Snapshot,
    ) {
        for grant in &mut self.comp.grants {
            grant.land(landing);
        }
        let Some(at) = self.at else {
            return;
        };
        let kept = match &self.lost {
            Some((known, was)) => landed_at(&Landing::of(known, now), *was),
            None => landed_at(landing, at),
        };
        match (kept, &self.lost) {
            (Some(_), Some(_)) => {
                self.lost = None;
                self.refused = None;
            }
            (None, None) => self.lost = Some((old.clone(), at)),
            _ => {}
        }
        if moved.track(at).is_none()
            && let Subject::New { below, .. } = &mut self.subject
        {
            *below = false;
        }
        self.at = kept.or_else(|| Some(moved.place(at)));
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
        if self.refused(action, 1) {
            return;
        }
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
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
        if self.refused(Action::InsertDependent, 1) {
            return;
        }
        let Some(task) = self
            .rows
            .get(self.cursor)
            .and_then(|row| self.snap.task(row.id))
        else {
            return self.open_new(true);
        };
        let task_id = task.task_id();
        let id = self.snap.id_of(task).map(errs::lossy);
        self.open_new(true);
        let word = match id {
            Some(id) => {
                self.link_id(task_id, &id);
                Ok(format!("{DEP}{id}"))
            }
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
        if self.refused(Action::InsertPrerequisite, 1) {
            return;
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

    fn begin(&mut self, draft: Draft) {
        self.kept = None;
        self.edit = Some(draft);
        self.reset_keys();
        self.scroll();
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
            // A new task with nothing typed: end INSERT mode, keep nothing.
            Action::Cancel if self.edit.as_ref().is_some_and(Draft::blank) => {
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
        if draft.unchanged() {
            return self.end_edit();
        }
        let text = draft.field.text();
        match &draft.subject {
            Subject::Task { .. } => self.commit_old(&text),
            Subject::New { .. } => self.commit_new(&text),
        }
    }

    fn commit_old(&mut self, text: &str) {
        let Some(draft) = &self.edit else {
            return;
        };
        let Some((source, nr)) = draft.place() else {
            return self.refuse_field(messages::REFUSED);
        };
        let target = Target::at(source, i64::from(nr.get()));
        let grants = draft.live_grants(text);
        let result = self.attempt(|env| {
            ops::edit_typed(env, &target, text.as_bytes(), &grants)
        });
        self.finish(result);
    }

    fn commit_new(&mut self, text: &str) {
        let Some(draft) = &self.edit else {
            return;
        };
        if draft.refuses() {
            return self.refuse_field(messages::REFUSED);
        }
        let place = draft.at.map_or(Where::End, |(_, nr)| Where::Beside {
            seen: Seen::of(&self.snap),
            at: nr.index(),
            below: draft.below(),
        });
        let blocks = draft
            .place()
            .filter(|_| draft.blocks())
            .map(|(source, nr)| Target::at(source, i64::from(nr.get())));
        let params = AddParams {
            texts: vec![text.to_owned()],
            place,
            grants: draft.live_grants(text),
            blocks,
            ..AddParams::default()
        };
        let result = self.attempt(|env| ops::add(env, &params));
        let added = self.finish(result);
        if let Some(at) = added.filter(|_| self.deps.config.keep_adding) {
            self.open_new_at(Some(at), true);
            self.message = self.message.take().map(|said| {
                said.replace(messages::STICKY, messages::STICKY_IN_FIELD)
            });
        }
    }

    /// Refuses a commit of a field whose line changed: says why the field
    /// stays open and how to leave it, and from now on `<Esc>` drops the text.
    fn refuse_field(&mut self, said: &'static str) {
        if let Some(draft) = &mut self.edit {
            draft.refused = Some(said);
        }
        self.alert(said.to_owned());
    }

    /// Keeps the refusal on the status line while the field stays open.
    pub(crate) fn keep_refusal(&mut self) {
        let refused = self.edit.as_ref().and_then(|d| d.refused);
        if let Some(said) = refused.filter(|_| self.message.is_none()) {
            self.refuse_field(said);
        }
    }

    /// Whether `stroke` is a way out of a field that was refused.
    pub(crate) fn escapes_refused(&self, stroke: Stroke) -> bool {
        self.edit.as_ref().is_some_and(|d| d.refused.is_some())
            && self.popup().is_none()
            && (stroke == Stroke::named(Named::Esc)
                || stroke == Stroke::char('c').with_mods(Mods::CTRL))
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
                self.fail_edit(&e);
                let dep = e.text(ops::DEAD_DEP).is_some();
                let said = if dep {
                    messages::DEP_REFUSED
                } else {
                    messages::REFUSED
                };
                if e.code() == Code::ExpectMismatch {
                    self.refuse_field(said);
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
        let Some(hint) = e.hint() else {
            return;
        };
        if let Some(message) = &mut self.message {
            message.push_str(" — ");
            message.push_str(hint);
        }
    }

    /// `cancel`: ends INSERT mode, writes nothing and keeps the draft
    /// for `u`. A draft that `u` could not give back goes the way of a
    /// refused commit.
    fn cancel(&mut self) {
        if let Some(draft) = &mut self.edit {
            if draft.refused.is_some() {
                return self.end_edit();
            }
            if draft.refuses() {
                return self.refuse_field(messages::REFUSED);
            }
            draft.comp.popup = None;
            draft.comp.under = None;
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
        if draft.refuses() {
            self.alert(ops::LIST_CHANGED.to_owned());
            return;
        }
        let at = draft.place();
        if let Some(place) = at.filter(|&p| self.row_at(p).is_none()) {
            self.sticky.hold(place);
            self.rebuild();
        }
        self.cursor_to(at);
        self.begin(draft);
    }

    /// Takes the drafts to a new reading, and says so once when the line
    /// being edited changed. A write of the session drops the kept draft,
    /// and the field it wrote from is about to end.
    pub(crate) fn remap_drafts(
        &mut self,
        moved: &Remap,
        old: &Snapshot,
        outside: Option<&Landing>,
    ) {
        let Some(landing) = outside else {
            self.kept = None;
            return;
        };
        if self.edit.is_none() && self.kept.is_none() {
            return;
        }
        let warned = self.edit.as_ref().is_some_and(Draft::refuses);
        for draft in self.edit.iter_mut().chain(self.kept.iter_mut()) {
            draft.follow(moved, landing, old, &self.snap);
        }
        let refuses = self.edit.as_ref().is_some_and(Draft::refuses);
        let armed = self.edit.as_ref().is_some_and(|d| d.refused.is_some());
        if refuses && !warned && !armed {
            self.alert(ops::LIST_CHANGED.to_owned());
        } else if warned && !refuses {
            self.message = None;
        }
        self.refresh_popup();
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
        Action::EditAppend => at_end(text),
        Action::EditLineStart => line_field(text, 0),
        Action::ChangeDescription => {
            let (cleared, at) = cleared(text);
            line_field(&cleared, at)
        }
        _ => described(text),
    }
}
