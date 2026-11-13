//! What a key would do over the tasks it is about: one rule for every key
//! that writes tasks or opens a field on them, and for those with nothing to
//! do in some state, asked by the key itself and by its hint. The core keeps
//! its own checks, for the CLI and for files changed on disk.

use lsty_core::{
    errs::Source,
    keys::{Action, Category, Matcher, Mode},
    list::{Snapshot, State},
};

use crate::{
    field::Field,
    focus::Focus,
    hints,
    history::{Again, Step},
    messages,
    model::Model,
    panel::Panel,
    rows::Row,
};

/// The `lsty` command a key resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Do,
    Wontdo,
    Start,
    Stop,
    Reopen,
    Delete,
    /// `cycle_priority` and its way back: `pri`, each task from its own.
    Step {
        forward: bool,
    },
}

impl Command {
    /// How the short form of a press that passed tasks over says what it
    /// did and what it passed over, for the commands that can.
    pub(crate) const fn words(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Do => Some(("done", "already done")),
            Self::Wontdo => Some(("won't do", "already done")),
            Self::Start => Some(("started", "already underway")),
            Self::Stop => Some(("stopped", "not underway")),
            Self::Reopen | Self::Delete | Self::Step { .. } => None,
        }
    }
}

/// Why a press writes nothing, when the key says so itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// No task is open, and some are archived.
    Closed,
    /// Some archived, some open, but for `x` and `T`; or any, for `J` and `K`.
    Archived,
    /// `t` and `T` on tasks that are all done.
    AllDone,
    /// `t` on todo and done tasks.
    SomeDone,
    /// The key has nothing to work on, and says nothing of its own.
    Quiet,
    /// `J` and `K` in a view that is not in file order.
    Sorted,
    /// `go` on an archived task without an id.
    NoId,
    /// The key has nothing to work on, and says this.
    Says(&'static str),
    /// `t` in the window after it stopped tasks whose lines changed since.
    Changed,
}

/// What a press would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The key does its work, or says why not: a key that does not write
    /// tasks, or a prompt that may open.
    Applies,
    /// The command runs over the tasks it changes, at least one, and passes
    /// over the rest.
    Resolved(Command),
    /// The press is refused, by the key.
    Refused(Refusal),
    /// The command plans over all the tasks of the list on screen, without
    /// the lock, and says why it writes nothing: it names the blockers or
    /// the task in its message.
    CoreRefuses(Command),
}

impl Answer {
    /// The command a press runs, and whether it writes: none when the key
    /// does its own work or refuses.
    pub(crate) const fn runs(self) -> Option<(Command, bool)> {
        match self {
            Self::Applies | Self::Refused(_) => None,
            Self::CoreRefuses(command) => Some((command, false)),
            Self::Resolved(command) => Some((command, true)),
        }
    }
}

/// What the rule reads of the tasks of a press.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Summary {
    /// The row of the first task.
    lo: usize,
    /// The count it was taken with.
    count: usize,
    total: usize,
    archived: usize,
    todo: usize,
    doing: usize,
    done: usize,
    blocked_todo: usize,
    blocked_doing: usize,
}

impl Summary {
    /// One pass over the tasks of `rows`, the first at row `lo`, reading
    /// fields only.
    pub(crate) fn of(snap: &Snapshot, lo: usize, rows: &[Row]) -> Self {
        let mut s = Self {
            lo,
            ..Self::default()
        };
        for task in rows.iter().filter_map(|row| snap.task(row.id)) {
            s.total += 1;
            let blocked = usize::from(task.blocked());
            match task.state() {
                _ if task.source() == Source::Done => s.archived += 1,
                State::Todo => {
                    s.todo += 1;
                    s.blocked_todo += blocked;
                }
                State::Doing => {
                    s.doing += 1;
                    s.blocked_doing += blocked;
                }
                State::Done => s.done += 1,
            }
        }
        s
    }
}

impl Model {
    /// The key `action` is: for `.`, the one it repeats, so that a `.` of `p`
    /// is a paste, aimed at a position, and a `.` of `x` writes on its lines.
    pub(crate) fn aimed(&self, action: Action) -> Action {
        match &self.again {
            Some(again) if action == Action::Repeat => again.action(),
            _ => action,
        }
    }

    /// Whether a press of `action` with `count` would write on lines: after
    /// keys were lost, such a press says the list changed instead.
    pub(crate) fn writes_lost(&self, action: Action, count: usize) -> bool {
        writes_line(self.aimed(action))
            && !hints::dim(self.answers_at(count)(action))
    }

    /// What a press with `count` reads.
    pub(crate) fn summary(&self, count: usize) -> Summary {
        let snap = &self.snap;
        let s = match self.selected_rows() {
            Some((lo, hi)) => Summary::of(snap, lo, self.rows_between(lo, hi)),
            None => Summary::of(snap, self.cursor, self.picked_rows(count)),
        };
        Summary { count, ..s }
    }

    /// The count typed so far; 0 when none, or where a digit is no count.
    pub(crate) fn typed_count(&self) -> usize {
        self.matchers
            .get(&self.key_mode())
            .map_or(0, Matcher::count)
    }

    /// The field that takes the keys now, in the order of `key_mode`.
    fn open_field(&self) -> Option<&Field> {
        self.theme_field()
            .or_else(|| self.help_field())
            .or_else(|| self.line.as_ref().map(|l| &l.field))
            .or_else(|| self.edit.as_ref().map(|d| &d.field))
    }

    /// What the hints ask of the rule, with the count typed so far. A second
    /// summary only with a count, for the keys that do not read it. Over
    /// help or a picker the rule has no say: those keys move there, whatever
    /// lies behind.
    pub(crate) fn answers(&self) -> impl Fn(Action) -> Answer + '_ {
        self.answers_at(self.typed_count())
    }

    /// What the hints would ask with `count` typed. `gx` reads the count
    /// the list holds, for its hint alone: it writes no line.
    pub(crate) fn answers_at(
        &self,
        count: usize,
    ) -> impl Fn(Action) -> Answer + '_ {
        let ruled = !matches!(self.key_mode(), Mode::Help | Mode::Picker);
        let typed = self.summary(count);
        let one = (count > 1).then(|| self.summary(1));
        move |action| {
            if !ruled {
                return Answer::Applies;
            }
            if action == Action::Repeat {
                return repeat(self, count);
            }
            let cursor = one.as_ref().filter(|_| !action.reads_count());
            decide(action, self, cursor.unwrap_or(&typed))
        }
    }
}

/// Whether `action` writes on the lines it is about, or opens a field to.
/// `o`, `O`, `p` and `P` take a line as a position only.
pub(crate) const fn writes_line(action: Action) -> bool {
    matches!(action, Action::Repeat)
        || matches!(action.category(), Category::Tasks | Category::Editing)
            && !matches!(
                action,
                Action::Yank
                    | Action::PasteBelow
                    | Action::PasteAbove
                    | Action::OpenLink
                    | Action::InsertBelow
                    | Action::InsertAbove
            )
}

/// `x`: any open task, `lsty do`, which refuses a blocked one; all done,
/// `lsty reopen`.
const fn complete(s: &Summary) -> Answer {
    if s.todo + s.doing == 0 {
        Answer::Resolved(Command::Reopen)
    } else if s.blocked_todo + s.blocked_doing > 0 {
        Answer::CoreRefuses(Command::Do)
    } else {
        Answer::Resolved(Command::Do)
    }
}

/// `t`: any todo task, `lsty start`, which refuses a done or blocked one
/// and takes over an agent's claim; else any doing, `lsty stop`.
const fn toggle_start(s: &Summary) -> Answer {
    if s.todo == 0 && s.doing == 0 {
        Answer::Refused(Refusal::AllDone)
    } else if s.todo == 0 {
        Answer::Resolved(Command::Stop)
    } else if s.done > 0 {
        Answer::Refused(Refusal::SomeDone)
    } else if s.blocked_todo > 0 {
        Answer::CoreRefuses(Command::Start)
    } else {
        Answer::Resolved(Command::Start)
    }
}

/// `t` in the window after it stopped these tasks, where it would start
/// them: it puts their claims back, which a blocked task does not refuse.
fn puts_back(m: &Model, s: &Summary) -> bool {
    s.todo > 0
        && s.done == 0
        && m.stopped_over(m.rows_between(s.lo, s.lo + s.total - 1))
            == Some(true)
}

/// The `t` that puts claims back: a step to apply, whose lines still stand
/// in the reading as it left them. Over lines changed since, the press
/// writes nothing and says so without the lock.
fn rewinds(m: &Model, top: Option<&Step>) -> bool {
    top.is_some_and(|top| top.lands(&m.snap))
}

/// `stop`, bound by a user: `lsty stop`, which says so when none is underway.
const fn stop(s: &Summary) -> Answer {
    if s.doing == 0 {
        Answer::CoreRefuses(Command::Stop)
    } else {
        Answer::Resolved(Command::Stop)
    }
}

/// `T`: any open task, `lsty do --wontdo`.
const fn wontdo(s: &Summary) -> Answer {
    if s.todo + s.doing > 0 {
        Answer::Resolved(Command::Wontdo)
    } else {
        Answer::Refused(Refusal::AllDone)
    }
}

/// `+` and `-`: a step is never already there, and `lsty pri` refuses a
/// done task.
const fn step(s: &Summary, forward: bool) -> Answer {
    let command = Command::Step { forward };
    if s.done > 0 {
        Answer::CoreRefuses(command)
    } else {
        Answer::Resolved(command)
    }
}

/// `J` and `K`: a task to move, the file's order, nothing archived, and a
/// todo row to move past, at row `past`.
fn moving(m: &Model, s: &Summary, past: Option<usize>) -> Answer {
    let past = past.and_then(|at| m.rows.get(at));
    if s.total == 0 {
        Answer::Refused(Refusal::Quiet)
    } else if m.sorted() {
        Answer::Refused(Refusal::Sorted)
    } else if s.archived > 0 {
        Answer::Refused(Refusal::Archived)
    } else {
        let todo_row = past.is_some_and(|row| row.source != Source::Done);
        lit_if(todo_row, Refusal::Quiet)
    }
}

/// A key that does its work, or says `why` not.
const fn lit_if(lit: bool, why: Refusal) -> Answer {
    if lit {
        Answer::Applies
    } else {
        Answer::Refused(why)
    }
}

/// `.`: as the key it repeats over the tasks it stored or the count typed over
/// them, whose word it does not take. `J` and `K` take a distance, not tasks.
/// Nothing to repeat says nothing. `typed` is the count typed.
fn repeat(m: &Model, typed: usize) -> Answer {
    let Some(again) = &m.again else {
        return Answer::Refused(Refusal::Quiet);
    };
    let s = m.summary(repeats_over(again, typed));
    // `.` ends the window of `t` before it runs, so it starts them anew.
    let back = again.action() == Action::ToggleStart && puts_back(m, &s);
    let answer = if back {
        toggle_start(&s)
    } else {
        decide(again.action(), m, &s)
    };
    match answer {
        Answer::Resolved(_) => Answer::Applies,
        answer @ (Answer::Applies
        | Answer::Refused(_)
        | Answer::CoreRefuses(_)) => answer,
    }
}

/// How many tasks `.` repeats `again` over: the count typed, else the one
/// it stored; one for a key whose count is not a number of tasks.
pub(crate) fn repeats_over(again: &Again, typed: usize) -> usize {
    let one =
        matches!(again, Again::Task { action, .. } if !action.reads_count());
    if one { 1 } else { again.count(typed) }
}

/// Whether `action` has nothing to move over, select or show: no task
/// beside the list, or a panel with no entries.
fn nothing_to_move(action: Action, m: &Model, s: &Summary) -> bool {
    let beside_list = s.total == 0 && m.focus == Focus::List;
    let empty = m.focused_panel().is_some_and(|p| p.entries.is_empty());
    (beside_list && action == Action::EnterVisual)
        || (action.is_motion() && (beside_list || empty))
}

/// `t` that puts a claim back: the step it rewinds must still be there.
fn put_back_answer(m: &Model) -> Answer {
    if rewinds(m, m.hist.undo.last()) {
        Answer::Resolved(Command::Start)
    } else {
        Answer::Refused(Refusal::Changed)
    }
}

/// `go`: lit when the task has as many links as the count typed; the panel
/// opens the link it has selected.
fn open_link(m: &Model, s: &Summary) -> Answer {
    if s.total == 0 {
        return Answer::Refused(Refusal::Quiet);
    }
    let wanted = if m.focus == Focus::Detail { 1 } else { s.count };
    let lit = m.cursor_links().len() >= wanted.max(1);
    lit_if(lit, Refusal::Says(messages::NO_LINK))
}

/// What a press of `action` over the tasks of `s` would do. Every action is
/// named, so a new one does not compile until it is decided. An archived
/// task refuses every key that writes or opens a field on it, except `x` on
/// archived tasks alone and `go` on one with an id.
#[expect(clippy::too_many_lines, reason = "every action is listed")]
pub(crate) fn decide(action: Action, m: &Model, s: &Summary) -> Answer {
    let rule: fn(&Summary) -> Answer = match action {
        action if nothing_to_move(action, m, s) => {
            return Answer::Refused(Refusal::Quiet);
        }
        Action::ToggleEntry => {
            let lit = m.focused_panel().is_some_and(|p| !p.entries.is_empty());
            return lit_if(lit, Refusal::Quiet);
        }
        Action::Fold => {
            let node = m.focused_panel().and_then(Panel::to_fold);
            return lit_if(node.is_some(), Refusal::Quiet);
        }
        Action::Unfold => {
            let node = m.focused_panel().and_then(Panel::to_unfold);
            return lit_if(node.is_some(), Refusal::Quiet);
        }
        Action::ClearFilters => {
            let lit = m.panels.iter().any(|p| !p.active.is_empty());
            return lit_if(lit, Refusal::Says("no filters to clear"));
        }
        Action::FocusLeft
        | Action::FocusDown
        | Action::FocusUp
        | Action::FocusRight
        | Action::FocusList
        | Action::FocusDetail
        | Action::FocusProjects
        | Action::FocusContexts
        | Action::FocusFilters => {
            return lit_if(m.focus_after(action).is_some(), Refusal::Quiet);
        }
        Action::ToggleStart if puts_back(m, s) => return put_back_answer(m),
        Action::ToggleComplete => complete,
        Action::ToggleStart => toggle_start,
        Action::Stop => stop,
        Action::Wontdo => wontdo,
        Action::Delete => |_| Answer::Resolved(Command::Delete),
        Action::CyclePriority => |s| step(s, true),
        Action::CyclePriorityBack => |s| step(s, false),
        Action::SetDue
        | Action::AddTag
        | Action::RemoveTag
        | Action::EditDescription
        | Action::EditAppend
        | Action::EditLineStart
        | Action::ChangeDescription
        | Action::InsertPrerequisite => |_| Answer::Applies,
        Action::InsertDependent => {
            let task = m.rows.get(m.cursor).and_then(|r| m.snap.task(r.id));
            let no_id = task.is_some_and(|task| {
                task.source() == Source::Done && m.snap.id_of(task).is_none()
            });
            return lit_if(!no_id, Refusal::NoId);
        }
        Action::Yank => return lit_if(s.total > 0, Refusal::Quiet),
        Action::MoveTaskDown => return moving(m, s, Some(s.lo + s.total)),
        Action::MoveTaskUp => return moving(m, s, s.lo.checked_sub(1)),
        Action::OpenLink => return open_link(m, s),
        Action::Undo => {
            let lit = m.kept.is_some() || !m.hist.undo.is_empty();
            return lit_if(lit, Refusal::Says("nothing to undo"));
        }
        Action::Redo => {
            let lit = !m.hist.redo.is_empty();
            return lit_if(lit, Refusal::Says("nothing to redo"));
        }
        Action::Repeat => return repeat(m, s.count),
        Action::DeleteChar
        | Action::DeleteWordBack
        | Action::DeleteWordForward
        | Action::DeleteToStart
        | Action::DeleteToEnd => {
            let lit = m.open_field().is_none_or(|f| f.acts(action));
            return lit_if(lit, Refusal::Quiet);
        }
        Action::CompleteNext | Action::CompletePrev => {
            let lit = m.line.as_ref().is_none_or(|l| !l.no_candidates);
            return lit_if(lit, Refusal::Quiet);
        }
        Action::Next | Action::Prev => {
            let lit = m.popup().is_none_or(|p| p.cands.len() > 1);
            return lit_if(lit, Refusal::Quiet);
        }
        // These always apply.
        Action::CursorDown
        | Action::CursorUp
        | Action::GoTop
        | Action::GoBottom
        | Action::HalfPageDown
        | Action::HalfPageUp
        | Action::PageDown
        | Action::PageUp
        | Action::ScrollDown
        | Action::ScrollUp
        | Action::CentreView
        | Action::TopView
        | Action::BottomView
        | Action::GoViewTop
        | Action::GoViewMiddle
        | Action::GoViewBottom
        | Action::InsertBelow
        | Action::InsertAbove
        // What is pasted is known only by running the clipboard's program.
        | Action::PasteBelow
        | Action::PasteAbove
        | Action::Search
        | Action::ClearSearch
        | Action::CycleSort
        | Action::ToggleSidebar
        | Action::ToggleDetail
        | Action::ToggleWrap
        | Action::EnterVisual
        | Action::ExitVisual
        | Action::FocusNext
        | Action::FocusPrev
        | Action::SelectAndJump
        | Action::CommandLine
        | Action::Help
        | Action::Quit
        | Action::Commit
        | Action::Cancel
        | Action::CursorRowUp
        | Action::CursorRowDown
        | Action::CursorLeft
        | Action::CursorRight
        | Action::WordBack
        | Action::WordForward
        | Action::LineStart
        | Action::LineEnd
        | Action::Accept
        | Action::Close
        | Action::NextLink
        | Action::PrevLink
        | Action::Execute
        | Action::HistoryPrev
        | Action::HistoryNext => return Answer::Applies,
    };
    let open = s.todo + s.doing > 0;
    let x = action == Action::ToggleComplete;
    if s.total == 0 {
        lit_if(action == Action::InsertPrerequisite, Refusal::Quiet)
    } else if s.archived == 0 || (open && (x || action == Action::Wontdo)) {
        rule(s)
    } else if open {
        Answer::Refused(Refusal::Archived)
    } else if s.archived == s.total && x {
        Answer::Resolved(Command::Reopen)
    } else {
        Answer::Refused(Refusal::Closed)
    }
}
